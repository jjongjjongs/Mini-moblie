//! The version-4 interpreter. The instruction set is the earlier SGS one with
//! every byte widened to a little-endian 16-bit slot and the opcodes
//! regrouped; `docs/gnex_sgs_v4_notes.md` has the layout and how it was
//! worked out.

use alloc::{format, string::String, vec::Vec};

use crate::{host::Host, program::Program};

pub type VmResult<T> = core::result::Result<T, String>;

/// How many instructions one callback may run before it is taken as stuck.
const STEP_BUDGET: u64 = 20_000_000;

pub struct Vm {
    pub code: Vec<u8>,
    pub code_end: usize,
    pub variables: Vec<crate::program::Variable>,
    pub memory: Vec<i32>,
    pub resources: Vec<Vec<u8>>,
    pub stack: Vec<i32>,
    returns: Vec<usize>,
    mark: usize,
    pc: usize,
    pub exited: bool,
}

impl Vm {
    pub fn new(program: &Program) -> Self {
        Self {
            code: program.data.clone(),
            code_end: program.code_end,
            variables: program.variables.clone(),
            memory: program.memory.clone(),
            resources: program.resources.clone(),
            stack: Vec::new(),
            returns: Vec::new(),
            mark: 0,
            pc: 0,
            exited: false,
        }
    }

    pub fn address(&self, variable: usize, index: i32) -> VmResult<usize> {
        let v = self
            .variables
            .get(variable)
            .ok_or_else(|| format!("variable {variable:#x} does not exist"))?;
        if index < 0 || index as usize >= v.count {
            return Err(format!("variable {variable:#x}[{index}] out of range ({})", v.count));
        }
        Ok(v.offset + index as usize)
    }

    pub fn get(&self, variable: usize, index: i32) -> VmResult<i32> {
        Ok(self.memory[self.address(variable, index)?])
    }

    pub fn set(&mut self, variable: usize, index: i32, value: i32) -> VmResult<()> {
        let address = self.address(variable, index)?;
        self.memory[address] = value;
        Ok(())
    }

    pub fn read(&self, address: i32) -> VmResult<i32> {
        self.memory
            .get(address as usize)
            .copied()
            .ok_or_else(|| format!("read of address {address}"))
    }

    pub fn write(&mut self, address: i32, value: i32) -> VmResult<()> {
        let slot = self
            .memory
            .get_mut(address as usize)
            .ok_or_else(|| format!("write of address {address}"))?;
        *slot = value;
        Ok(())
    }

    pub fn push(&mut self, value: i32) {
        self.stack.push(value);
    }

    pub fn pop(&mut self) -> VmResult<i32> {
        self.stack.pop().ok_or_else(|| "stack underflow".into())
    }

    /// `N` values off the stack, oldest first.
    pub fn args<const N: usize>(&mut self) -> VmResult<[i32; N]> {
        let mut out = [0; N];
        for slot in out.iter_mut().rev() {
            *slot = self.pop()?;
        }
        Ok(out)
    }

    fn slot(&mut self) -> VmResult<u16> {
        if self.pc + 2 > self.code_end {
            return Err(format!("instruction at {:#x} outside the code", self.pc));
        }
        let value = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]);
        self.pc += 2;
        Ok(value)
    }

    fn target(&mut self) -> VmResult<usize> {
        let hi = self.slot()? as usize;
        let lo = self.slot()? as usize;
        Ok(hi << 16 | lo)
    }

    /// One of the four addressing forms: `var[var2[i]]`, `var[var2]`,
    /// `var[i]` and `var`, read off the code.
    fn location(&mut self, mode: u16) -> VmResult<(usize, i32)> {
        let variable = self.slot()? as usize;
        let index = match mode {
            0 => {
                let other = self.slot()? as usize;
                let element = self.slot()? as i32;
                self.get(other, element)?
            }
            1 => {
                let other = self.slot()? as usize;
                self.get(other, 0)?
            }
            2 => self.slot()? as i32,
            _ => 0,
        };
        Ok((variable, index))
    }

    /// Runs the callback at `entry` to its end.
    pub fn run(&mut self, entry: usize, host: &mut Host) -> VmResult<()> {
        if entry == 0 || self.exited {
            return Ok(());
        }
        self.pc = entry;
        self.stack.clear();
        self.returns.clear();
        self.mark = 0;
        let mut steps = 0u64;
        loop {
            steps += 1;
            if steps > STEP_BUDGET {
                return Err(format!("callback {entry:#x} did not end"));
            }
            let at = self.pc;
            let op = self.slot()?;
            match self.step(op, host) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(x) => return Err(format!("{x} (op {op:#x} at {at:#x})")),
            }
        }
    }

    /// One instruction; true when the callback is over.
    fn step(&mut self, op: u16, host: &mut Host) -> VmResult<bool> {
        match op {
            0x00 | 0x88 => {}
            0x01..=0x04 => {
                let (v, i) = self.location(op - 1)?;
                let value = self.get(v, i)?;
                self.push(value);
            }
            0x05 => {
                let value = self.slot()? as i16 as i32;
                self.push(value);
            }
            0x06 => {
                let value = self.slot()? as i32;
                self.push(value);
            }
            0x07 => {
                let hi = self.slot()? as u32;
                let lo = self.slot()? as u32;
                self.push((hi << 16 | lo) as i32);
            }
            0x08..=0x0b => {
                let (v, i) = self.location(op - 8)?;
                let value = self.pop()?;
                self.set(v, i, value)?;
            }
            0x0c => self.mark = self.stack.len(),
            0x0d => self.stack.truncate(self.mark),
            0x0e => {
                let v = self.pop()?;
                self.push(v.wrapping_add(1));
            }
            0x0f => {
                let v = self.pop()?;
                self.push(v.wrapping_sub(1));
            }
            0x10 => {
                let v = self.pop()?;
                self.push(v);
                self.push(v);
            }
            0x11 => {
                let v = self.pop()?;
                self.push(v.wrapping_neg());
            }
            0x12 => {
                let [a, b] = self.args::<2>()?;
                self.push(b);
                self.push(a);
            }
            0x55 => {
                let v = self.pop()?;
                self.push((v == 0) as i32);
            }
            0x13 | 0x14 | 0x50..=0x5e => {
                let [a, b] = self.args::<2>()?;
                let result = match op {
                    0x13 => a.wrapping_add(b),
                    0x14 => a.wrapping_sub(b),
                    0x50 => a.wrapping_mul(b),
                    0x51 | 0x52 => {
                        if b == 0 {
                            return Err("division by zero".into());
                        }
                        if op == 0x51 { a.wrapping_div(b) } else { a.wrapping_rem(b) }
                    }
                    0x53 => a & b,
                    0x54 => a | b,
                    0x56 => a ^ b,
                    0x57 => a >> (b & 31),
                    0x58 => a << (b & 31),
                    0x59 => (a > b) as i32,
                    0x5a => (a < b) as i32,
                    0x5b => (a >= b) as i32,
                    0x5c => (a <= b) as i32,
                    0x5d => (a == b) as i32,
                    _ => (a != b) as i32,
                };
                self.push(result);
            }
            // Assignments: four destinations, each with sources
            // `var[var2[i]]`, `var[var2]`, `var[i]`, `var`, a 16-bit and a
            // 32-bit immediate.
            0x5f..=0x76 => {
                let (destination, source) = ((op - 0x5f) / 6, (op - 0x5f) % 6);
                let (v, i) = self.location(destination)?;
                let value = match source {
                    4 => self.slot()? as i16 as i32,
                    5 => {
                        let hi = self.slot()? as u32;
                        let lo = self.slot()? as u32;
                        (hi << 16 | lo) as i32
                    }
                    _ => {
                        let (sv, si) = self.location(source)?;
                        self.get(sv, si)?
                    }
                };
                self.set(v, i, value)?;
            }
            0x77..=0x7a => {
                let (v, i) = self.location(op - 0x77)?;
                let delta = self.slot()? as i16 as i32;
                let value = self.get(v, i)?.wrapping_add(delta);
                self.set(v, i, value)?;
            }
            0x7b..=0x80 => {
                let immediate = self.slot()? as i16 as i32;
                let target = self.target()?;
                let left = self.pop()?;
                let taken = match op {
                    0x7b => left > immediate,
                    0x7c => left < immediate,
                    0x7d => left >= immediate,
                    0x7e => left <= immediate,
                    0x7f => left == immediate,
                    _ => left != immediate,
                };
                if taken {
                    self.jump(target)?;
                }
            }
            0x81 => {
                let target = self.target()?;
                self.jump(target)?;
            }
            0x82 | 0x83 => {
                let target = self.target()?;
                let condition = self.pop()? != 0;
                if condition == (op == 0x82) {
                    self.jump(target)?;
                }
            }
            0x85 => {
                let target = self.target()?;
                if self.returns.len() >= 64 {
                    return Err("return stack overflow".into());
                }
                self.returns.push(self.pc);
                self.jump(target)?;
            }
            0x86 => match self.returns.pop() {
                Some(x) => self.pc = x,
                None => return Ok(true),
            },
            0x87 => {
                self.exited = true;
                return Ok(true);
            }
            0xff => return Ok(true),
            // Store the top of the stack without taking it off.
            0x89..=0x8c => {
                let (v, i) = self.location(op - 0x89)?;
                let value = *self.stack.last().ok_or("stack underflow")?;
                self.set(v, i, value)?;
            }
            0x8d => {
                let (v, i) = self.location(2)?;
                let address = self.address(v, i)? as i32;
                self.push(address);
            }
            0x8e => {
                let v = self.slot()? as usize;
                let address = self.address(v, 0)? as i32;
                self.push(address);
            }
            0x8f => {
                let address = self.pop()?;
                let value = self.read(address)?;
                self.push(value);
            }
            0x90 | 0x91 => {
                let [address, value] = self.args::<2>()?;
                self.write(address, value)?;
                if op == 0x91 {
                    self.push(value);
                }
            }
            0x92 => {
                let service = self.slot()?;
                crate::services::call(service, self, host)?;
            }
            0x94..=0x97 => {
                let (x, y, resource, mirror, palette) = match op {
                    0x94 => {
                        let [x, y, r] = self.args::<3>()?;
                        (x, y, r, 0, None)
                    }
                    0x95 => {
                        let [x, y, r, m] = self.args::<4>()?;
                        (x, y, r, m, None)
                    }
                    0x96 => {
                        let [x, y, r, p] = self.args::<4>()?;
                        (x, y, r, 0, Some(p))
                    }
                    _ => {
                        let [x, y, r, m, p] = self.args::<5>()?;
                        (x, y, r, m, Some(p))
                    }
                };
                let palette = match palette {
                    Some(address) => Some(
                        host.palettes
                            .get(&address)
                            .cloned()
                            .ok_or_else(|| format!("no packed palette at {address}"))?,
                    ),
                    None => None,
                };
                let data = self.resource(resource)?;
                host.graphics
                    .bitmap(data, x, y, mirror != 0, palette.as_deref())
                    .map_err(|x| format!("{x} (resource {resource})"))?;
            }
            // The top of the stack plus a variable: `var[var2[i]]`, `var[var2]`,
            // `var`, `var[i]`.
            0xe8..=0xf3 => {
                let (mode, kind) = ((op - 0xe8) / 3, (op - 0xe8) % 3);
                let (v, i) = self.location([0, 1, 3, 2][mode as usize])?;
                let value = self.get(v, i)?;
                let top = self.pop()?;
                self.push(match kind {
                    0 => top.wrapping_add(value),
                    1 => top.wrapping_sub(value),
                    _ => top.wrapping_mul(value),
                });
            }
            _ => return Err(format!("unknown opcode {op:#x}")),
        }
        Ok(false)
    }

    fn jump(&mut self, target: usize) -> VmResult<()> {
        if !(crate::program::CODE_START..self.code_end).contains(&target) {
            return Err(format!("jump to {target:#x} outside the code"));
        }
        self.pc = target;
        Ok(())
    }

    pub fn resource(&self, id: i32) -> VmResult<&[u8]> {
        self.resources
            .get(id as usize)
            .map(|x| x.as_slice())
            .ok_or_else(|| format!("resource {id} does not exist"))
    }

    pub fn resource_mut(&mut self, id: i32) -> VmResult<&mut Vec<u8>> {
        self.resources.get_mut(id as usize).ok_or_else(|| format!("resource {id} does not exist"))
    }

    /// A resource read as a NUL-terminated string.
    pub fn string(&self, id: i32) -> VmResult<Vec<u8>> {
        let data = self.resource(id)?;
        Ok(data.split(|&x| x == 0).next().unwrap_or(&[]).to_vec())
    }

    pub fn set_string(&mut self, id: i32, value: &[u8]) -> VmResult<()> {
        let data = self.resource_mut(id)?;
        data.clear();
        data.extend_from_slice(value);
        data.push(0);
        Ok(())
    }
}
