//! SKT GNEX scripts in the earlier layouts, header versions 1 and 2: one byte
//! an instruction slot, 16-bit values, and the platform's services as opcodes
//! of their own rather than a numbered call. The format, the instruction set
//! and the services' contracts are ported from wfeature's `internal/sgsvm`
//! and `internal/platform/skt/script*.go` (MIT, Copyright (c) 2026
//! movingwoo), which runs these versions.
//!
//! 샤이닝스톤 (SKT `0014478632`) is a version-2 script.

mod services;

use alloc::{format, string::String, vec::Vec};

use crate::host::Host;

pub type VmResult<T> = core::result::Result<T, String>;

/// How many instructions one callback may run before it is taken as stuck.
const STEP_BUDGET: u64 = 5_000_000;
/// The handset's operand and return stacks.
const STACK_DEPTH: usize = 65;
const RETURN_DEPTH: usize = 17;
/// An address with this bit names a word of the constant bank rather than
/// the mutable one.
const CONSTANT_BANK: u16 = 0x4000;
const LARGEST_SCRIPT: usize = 128 * 1024;
const WRAPPER: usize = 32;

/// The first byte of a script in one of these layouts.
pub fn is_classic(data: &[u8]) -> bool {
    matches!(unwrap(data).first(), Some(1 | 2))
}

/// The script with the 32-byte download wrapper taken off: a first word of
/// 32 and nothing else.
fn unwrap(data: &[u8]) -> &[u8] {
    if data.len() >= WRAPPER && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == WRAPPER as u32 && data[4..WRAPPER].iter().all(|&x| x == 0)
    {
        &data[WRAPPER..]
    } else {
        data
    }
}

/// One of the script's variables: where its words start in the bank it is
/// kept in, and how many there are.
#[derive(Clone, Copy)]
struct Variable {
    mutable: bool,
    offset: usize,
    len: usize,
}

#[derive(Clone)]
pub struct Resource {
    mutable: bool,
    pub data: Vec<u8>,
}

/// The panel a script's LCD class mask asks for.
pub fn panel(data: &[u8]) -> (u32, u32) {
    let data = unwrap(data);
    // A script that leaves out the usual class 4 but takes class 8 needs at
    // least 176 pixels each way.
    match data.get(1) {
        Some(mask) if mask & 4 == 0 && mask & 8 != 0 => (176, 176),
        _ => (128, 160),
    }
}

pub struct Vm {
    code: Vec<u8>,
    code_start: usize,
    code_end: usize,
    pub title: String,
    entries: [usize; 8],
    variables: Vec<Variable>,
    constants: Vec<i16>,
    mutable: Vec<i16>,
    pub resources: Vec<Resource>,
    /// The immutable resources as they lie end to end in the script: a read
    /// past the end of one runs on into the next.
    original: Vec<u8>,
    original_at: Vec<usize>,
    pc: usize,
    stack: Vec<i16>,
    returns: Vec<usize>,
    saved_stack: usize,
    ended: bool,
    pub exited: bool,
    /// Pictures queued to be drawn together (`0x74`, `0x75`).
    queue: Vec<services::Queued>,
}

impl Vm {
    pub fn parse(data: &[u8]) -> VmResult<Self> {
        let data = unwrap(data);
        if data.len() < 52 || data.len() > LARGEST_SCRIPT {
            return Err(format!("SGS: a script of {} bytes", data.len()));
        }
        let header = match data[0] {
            1 => 52,
            2 => 100,
            x => return Err(format!("SGS: header version {x}")),
        };
        if data.len() < header {
            return Err("SGS: truncated header".into());
        }
        let u16_at = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]) as usize;
        let (vd, vi, rd, ri) = (u16_at(44), u16_at(46), u16_at(48), u16_at(50));
        if vd < header || vi < vd || rd < vi || ri < rd || ri > data.len() || (vi - vd) % 4 != 0 || (ri - rd) % 4 != 0 {
            return Err("SGS: invalid variable or resource regions".into());
        }

        let mut entries = [0; 8];
        for (i, entry) in entries.iter_mut().enumerate() {
            *entry = u16_at(28 + i * 2);
            if *entry != 0 && !(header..vd).contains(entry) {
                return Err(format!("SGS: callback {i} outside the code"));
            }
        }
        let title = data[10..26].split(|&x| x == 0).next().unwrap_or(&[]);
        let (title, _, _) = encoding_rs::EUC_KR.decode(title);

        // Variables: (mutable, count, initialized, 0). An immutable variable,
        // and a mutable one marked initialized, takes its words from the
        // initializer region in order; the immutable ones stay there.
        let mut variables = Vec::new();
        let mut mutable_words = 0;
        let mut mutable_initial = Vec::new();
        let mut cursor = vi;
        for i in 0..(vi - vd) / 4 {
            let d = &data[vd + i * 4..vd + i * 4 + 4];
            let (mutable, count) = (d[0] != 0, d[1] as usize);
            let initialized = !mutable || d[2] != 0;
            let words_at = cursor;
            if initialized {
                if count * 2 > rd - cursor {
                    return Err(format!("SGS: variable {i} initializer truncated"));
                }
                cursor += count * 2;
            }
            if mutable {
                variables.push(Variable {
                    mutable,
                    offset: mutable_words,
                    len: count,
                });
                mutable_words += count;
                for j in 0..count {
                    mutable_initial.push(if initialized { u16_at(words_at + j * 2) as i16 } else { 0 });
                }
            } else {
                variables.push(Variable {
                    mutable,
                    offset: (words_at - vi) / 2,
                    len: count,
                });
            }
        }
        let constants = (0..(cursor - vi) / 2).map(|i| u16_at(vi + i * 2) as i16).collect();

        // Resources: (mutable, kind, u16 size), the bytes following in order.
        let mut resources = Vec::new();
        let mut original = Vec::new();
        let mut original_at = Vec::new();
        let mut cursor = ri;
        for i in 0..(ri - rd) / 4 {
            let d = &data[rd + i * 4..rd + i * 4 + 4];
            let size = u16::from_le_bytes([d[2], d[3]]) as usize;
            let bytes = data.get(cursor..cursor + size).ok_or_else(|| format!("SGS: resource {i} truncated"))?;
            original_at.push(original.len());
            original.extend_from_slice(bytes);
            resources.push(Resource {
                mutable: d[0] != 0,
                data: bytes.to_vec(),
            });
            cursor += size;
        }

        Ok(Self {
            code: data.to_vec(),
            code_start: header,
            code_end: vd,
            title: title.trim().into(),
            entries,
            variables,
            constants,
            mutable: mutable_initial,
            resources,
            original,
            original_at,
            pc: 0,
            stack: Vec::new(),
            returns: Vec::new(),
            saved_stack: 0,
            ended: false,
            exited: false,
            queue: Vec::new(),
        })
    }

    pub fn has_variable(&self, variable: usize) -> bool {
        self.variables.get(variable).is_some_and(|x| x.len > 0)
    }

    fn word_of(&self, variable: usize, index: i32) -> VmResult<(bool, usize)> {
        let v = self
            .variables
            .get(variable)
            .ok_or_else(|| format!("variable {variable} does not exist"))?;
        if index < 0 || index as usize >= v.len {
            return Err(format!("variable {variable} index {index} out of bounds"));
        }
        Ok((v.mutable, v.offset + index as usize))
    }

    pub fn value(&self, variable: usize, index: i32) -> VmResult<i16> {
        let (mutable, at) = self.word_of(variable, index)?;
        let bank = if mutable { &self.mutable } else { &self.constants };
        bank.get(at).copied().ok_or_else(|| format!("variable {variable} outside its bank"))
    }

    pub fn set(&mut self, variable: usize, index: i32, value: i16) -> VmResult<()> {
        let (mutable, at) = self.word_of(variable, index)?;
        let bank = if mutable { &mut self.mutable } else { &mut self.constants };
        *bank.get_mut(at).ok_or_else(|| format!("variable {variable} outside its bank"))? = value;
        Ok(())
    }

    fn address(&self, variable: usize, index: i32) -> VmResult<i16> {
        let (mutable, at) = self.word_of(variable, index)?;
        if at >= CONSTANT_BANK as usize {
            return Err("variable address out of bounds".into());
        }
        Ok(if mutable { at as i16 } else { (at as u16 | CONSTANT_BANK) as i16 })
    }

    fn bank_word(&mut self, address: i16) -> VmResult<&mut i16> {
        if address < 0 {
            return Err(format!("negative variable address {address}"));
        }
        let address = address as u16;
        let at = (address & 0x3fff) as usize;
        let bank = if address & CONSTANT_BANK != 0 {
            &mut self.constants
        } else {
            &mut self.mutable
        };
        bank.get_mut(at).ok_or_else(|| format!("variable address {address:#x} out of bounds"))
    }

    /// The word at an address the script made with `0x4c`/`0x4d`.
    pub fn read(&mut self, address: i16) -> VmResult<i16> {
        self.bank_word(address).map(|x| *x)
    }

    pub fn write(&mut self, address: i16, value: i16) -> VmResult<()> {
        *self.bank_word(address)? = value;
        Ok(())
    }

    pub fn push(&mut self, value: i16) -> VmResult<()> {
        if self.stack.len() >= STACK_DEPTH {
            return Err("operand stack overflow".into());
        }
        self.stack.push(value);
        Ok(())
    }

    pub fn pop(&mut self) -> VmResult<i16> {
        self.stack.pop().ok_or_else(|| "operand stack underflow".into())
    }

    /// `N` values in the order they were pushed.
    pub fn args<const N: usize>(&mut self) -> VmResult<[i16; N]> {
        if self.stack.len() < N {
            return Err("operand stack underflow".into());
        }
        let mut values = [0; N];
        values.copy_from_slice(&self.stack[self.stack.len() - N..]);
        self.stack.truncate(self.stack.len() - N);
        Ok(values)
    }

    pub fn resource(&self, id: i16) -> VmResult<&Resource> {
        self.resources
            .get(id as usize)
            .filter(|_| id >= 0)
            .ok_or_else(|| format!("resource {id} does not exist"))
    }

    pub fn resource_mut(&mut self, id: i16) -> VmResult<&mut Resource> {
        if id < 0 {
            return Err(format!("resource {id} does not exist"));
        }
        self.resources.get_mut(id as usize).ok_or_else(|| format!("resource {id} does not exist"))
    }

    /// A resource's sound, the SMAF file after its two-byte wrapper.
    pub fn sound(&self, id: i32) -> Option<&[u8]> {
        self.resources
            .get(id as usize)
            .map(|x| &x.data[..])
            .filter(|x| x.len() > 2)
            .map(|x| &x[2..])
    }

    /// A byte of a resource, at an offset that may run past its end: an
    /// immutable one into the resources after it in the script, a mutable
    /// one into the mutable resources after it.
    pub fn resource_byte(&self, id: i16, offset: i32) -> VmResult<u8> {
        let resource = self.resource(id)?;
        if offset < 0 {
            return Err(format!("resource {id} offset {offset}"));
        }
        let offset = offset as usize;
        if let Some(&x) = resource.data.get(offset) {
            return Ok(x);
        }
        if !resource.mutable {
            if let Some(&x) = self.original.get(self.original_at[id as usize] + offset) {
                return Ok(x);
            }
        } else {
            let mut remaining = offset - resource.data.len();
            for next in self.resources[id as usize + 1..].iter().filter(|x| x.mutable) {
                if remaining < next.data.len() {
                    return Ok(next.data[remaining]);
                }
                remaining -= next.data.len();
            }
        }
        Err(format!("resource {id} byte {offset} out of bounds"))
    }

    fn byte(&mut self) -> VmResult<u8> {
        if !(self.code_start..self.code_end).contains(&self.pc) {
            return Err(format!("instruction address {:#x} out of bounds", self.pc));
        }
        let value = self.code[self.pc];
        self.pc += 1;
        Ok(value)
    }

    fn word(&mut self) -> VmResult<i16> {
        let hi = self.byte()? as u16;
        let lo = self.byte()? as u16;
        Ok((hi << 8 | lo) as i16)
    }

    fn jump(&mut self, target: usize) -> VmResult<()> {
        if !(self.code_start..self.code_end).contains(&target) {
            return Err(format!("branch target {target:#x} out of bounds"));
        }
        self.pc = target;
        Ok(())
    }

    /// One of the four ways an instruction names a variable's word:
    /// `var[var2[i]]`, `var[var2]`, `var[i]`, `var`.
    fn location(&mut self, mode: u8) -> VmResult<(usize, i32)> {
        let variable = self.byte()? as usize;
        let index = match mode {
            0 => {
                let (other, element) = (self.byte()? as usize, self.byte()? as i32);
                self.value(other, element)? as i32
            }
            1 => {
                let other = self.byte()? as usize;
                self.value(other, 0)? as i32
            }
            2 => self.byte()? as i32,
            _ => 0,
        };
        Ok((variable, index))
    }

    /// Runs callback `entry` (0 initialization, 1 termination, 2 timer, 3
    /// key, then messages and system notices) to its end.
    pub fn run(&mut self, entry: usize, host: &mut Host) -> VmResult<()> {
        let entry = self.entries[entry];
        if entry == 0 || self.exited {
            return Ok(());
        }
        self.pc = entry;
        self.stack.clear();
        self.returns.clear();
        self.saved_stack = 0;
        self.ended = false;
        for _ in 0..STEP_BUDGET {
            let at = self.pc;
            let op = self.byte()?;
            self.step(op, host).map_err(|x| format!("{x} (op {op:#x} at {at:#x})"))?;
            if self.ended {
                return Ok(());
            }
        }
        Err(format!("callback {entry:#x} did not end"))
    }

    fn step(&mut self, op: u8, host: &mut Host) -> VmResult<()> {
        let truth = |x: bool| x as i16;
        match op {
            0x00 | 0x47 => {}
            // Push a word, or store the top into one; 1-4 and 7-10 address it
            // the four ways `location` reads.
            0x01..=0x04 | 0x07..=0x0a => {
                let mode = if op >= 7 { op - 7 } else { op - 1 };
                let (variable, index) = self.location(mode)?;
                if op <= 4 {
                    let value = self.value(variable, index)?;
                    self.push(value)?;
                } else {
                    let value = self.pop()?;
                    self.set(variable, index, value)?;
                }
            }
            0x05 => {
                let value = self.byte()? as i8 as i16;
                self.push(value)?;
            }
            0x06 => {
                let value = self.word()?;
                self.push(value)?;
            }
            0x0b => self.saved_stack = self.stack.len(),
            0x0c => self.stack.truncate(self.saved_stack),
            0x0d => {
                let value = self.pop()?;
                self.push(value.wrapping_add(1))?;
            }
            0x0e => {
                let value = self.pop()?;
                self.push(value.wrapping_sub(1))?;
            }
            0x0f => {
                let value = self.pop()?;
                self.push(value)?;
                self.push(value)?;
            }
            0x10 => {
                let value = self.pop()?;
                self.push(value.wrapping_neg())?;
            }
            0x11 => {
                let [a, b] = self.args::<2>()?;
                self.push(b)?;
                self.push(a)?;
            }
            0x19 => {
                let value = self.pop()?;
                self.push(truth(value == 0))?;
            }
            0x12..=0x22 => {
                let [a, b] = self.args::<2>()?;
                let value = match op {
                    0x12 => a.wrapping_add(b),
                    0x13 => a.wrapping_sub(b),
                    0x14 => a.wrapping_mul(b),
                    0x15 | 0x16 => {
                        if b == 0 {
                            return Err("division by zero".into());
                        }
                        if op == 0x15 {
                            (a as i32 / b as i32) as i16
                        } else {
                            (a as i32 % b as i32) as i16
                        }
                    }
                    0x17 => a & b,
                    0x18 => a | b,
                    0x1a => a ^ b,
                    0x1b => a >> (b & 31).min(15),
                    0x1c => ((a as i32) << (b & 31)) as i16,
                    0x1d => truth(a > b),
                    0x1e => truth(a < b),
                    0x1f => truth(a >= b),
                    0x20 => truth(a <= b),
                    0x21 => truth(a == b),
                    _ => truth(a != b),
                };
                self.push(value)?;
            }
            // Assignments: five sources (the four locations and a signed
            // byte) into four destinations.
            0x23..=0x36 => {
                let (destination, source) = ((op - 0x23) / 5, (op - 0x23) % 5);
                let (variable, index) = self.location(destination)?;
                let value = if source == 4 {
                    self.byte()? as i8 as i16
                } else {
                    let (other, element) = self.location(source)?;
                    self.value(other, element)?
                };
                self.set(variable, index, value)?;
            }
            0x37..=0x3a => {
                let (variable, index) = self.location(op - 0x37)?;
                let delta = self.byte()? as i8 as i16;
                let value = self.value(variable, index)?;
                self.set(variable, index, value.wrapping_add(delta))?;
            }
            // Compare the top with a signed byte and branch.
            0x3b..=0x40 => {
                let value = self.byte()? as i8 as i16;
                let target = self.word()? as u16 as usize;
                let left = self.pop()?;
                let taken = [left > value, left < value, left >= value, left <= value, left == value, left != value][(op - 0x3b) as usize];
                if taken {
                    self.jump(target)?;
                }
            }
            // Jump, jump if true, jump if false, call.
            0x41..=0x44 => {
                let target = self.word()? as u16 as usize;
                let taken = match op {
                    0x42 => self.pop()? != 0,
                    0x43 => self.pop()? == 0,
                    0x44 => {
                        if self.returns.len() >= RETURN_DEPTH {
                            return Err("return stack overflow".into());
                        }
                        self.returns.push(self.pc);
                        true
                    }
                    _ => true,
                };
                if taken {
                    self.jump(target)?;
                }
            }
            0x45 => match self.returns.pop() {
                Some(target) => self.jump(target)?,
                None => self.ended = true,
            },
            0x46 => {
                self.ended = true;
                self.exited = true;
            }
            // Store the top and keep it.
            0x48..=0x4b => {
                let (variable, index) = self.location(op - 0x48)?;
                let value = *self.stack.last().ok_or("operand stack underflow")?;
                self.set(variable, index, value)?;
            }
            // Push a word's address; read through one; write through one.
            0x4c | 0x4d => {
                let (variable, index) = self.location(if op == 0x4c { 2 } else { 3 })?;
                let address = self.address(variable, index)?;
                self.push(address)?;
            }
            0x4e => {
                let address = self.pop()?;
                let value = self.read(address)?;
                self.push(value)?;
            }
            0x4f | 0x50 => {
                let [address, value] = self.args::<2>()?;
                if address as u16 & CONSTANT_BANK != 0 {
                    return Err(format!("write to constant address {:#x}", address as u16));
                }
                self.write(address, value)?;
                if op == 0x50 {
                    self.push(value)?;
                }
            }
            0xb4..=0xb6 => self.vector(op)?,
            0xff => self.ended = true,
            _ => services::call(op, self, host)?,
        }
        Ok(())
    }

    /// An operation over a run of words: on each with a value (`0xb4`), with
    /// another run (`0xb5`), or of two runs into a third (`0xb6`).
    fn vector(&mut self, op: u8) -> VmResult<()> {
        let mut args = [0i16; 5];
        let n = if op == 0xb6 { 5 } else { 4 };
        if self.stack.len() < n {
            return Err("operand stack underflow".into());
        }
        args[..n].copy_from_slice(&self.stack[self.stack.len() - n..]);
        self.stack.truncate(self.stack.len() - n);
        let (count, operation) = (args[n - 2] as i32, args[n - 1]);
        if !(0..=11).contains(&operation) {
            return Err(format!("vector operation {operation}"));
        }
        if op == 0xb4 && (operation == 4 || operation == 5) && args[1] == 0 {
            return Err("division by zero".into());
        }
        for i in 0..count.max(0) as i16 {
            let address = args[0].wrapping_add(i);
            let (left, right) = match op {
                0xb4 => (self.read(address)?, args[1]),
                0xb5 => (self.read(address)?, self.read(args[1].wrapping_add(i))?),
                _ => {
                    let left = self.read(args[1].wrapping_add(i))?;
                    let right = if operation == 0 || operation == 8 {
                        left
                    } else {
                        self.read(args[2].wrapping_add(i))?
                    };
                    (left, right)
                }
            };
            let value = match operation {
                0 => right,
                1 => left.wrapping_add(right),
                2 => left.wrapping_sub(right),
                3 => left.wrapping_mul(right),
                4 | 5 => {
                    if right == 0 {
                        return Err("division by zero".into());
                    }
                    if operation == 4 {
                        (left as i32 / right as i32) as i16
                    } else {
                        (left as i32 % right as i32) as i16
                    }
                }
                6 => left & right,
                7 => left | right,
                8 => !right,
                9 => left ^ right,
                10 => left >> (right & 31).min(15),
                _ => ((left as i32) << (right & 31)) as i16,
            };
            self.write(address, value)?;
        }
        Ok(())
    }
}

/// Bytes up to the first NUL.
fn c_string(data: &[u8]) -> &[u8] {
    data.split(|&x| x == 0).next().unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    /// A version-1 script: header, one callback that adds 2 and 3 into a
    /// mutable variable and ends, one variable, no resources.
    fn script() -> Vec<u8> {
        let mut data = vec![0u8; 52];
        data[0] = 1;
        let code = [0x05, 2, 0x05, 3, 0x12, 0x0a, 0, 0xff];
        let vd = 52 + code.len();
        data[28..30].copy_from_slice(&52u16.to_le_bytes());
        data.extend_from_slice(&code);
        // One mutable variable of one word, uninitialized.
        data.extend_from_slice(&[1, 1, 0, 0]);
        let vi = data.len();
        for (at, value) in [(44, vd), (46, vi), (48, vi), (50, vi)] {
            data[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes());
        }
        data
    }

    #[test]
    fn a_callback_runs() {
        let mut vm = Vm::parse(&script()).unwrap();
        let mut host = Host::new(128, 160, crate::files::Files::new(None));
        vm.run(0, &mut host).unwrap();
        assert_eq!(vm.value(0, 0).unwrap(), 5);
    }

    #[test]
    fn the_wrapper_is_taken_off() {
        let mut wrapped = vec![0u8; 32];
        wrapped[0] = 32;
        wrapped.extend_from_slice(&script());
        assert!(is_classic(&wrapped));
        assert!(Vm::parse(&wrapped).is_ok());
    }
}
