//! The Thumb frontend of the block engine ([`super::JitEngine`]).
//!
//! The stock [`Arm32CpuEngine`](super::Arm32CpuEngine) re-fetches, re-decodes
//! and re-dispatches every guest instruction through a banked register file.
//! This decodes a Thumb instruction once into a [`FastOp`] with its operand
//! fields pre-extracted, which the block engine caches by block and runs - as
//! native code where it has a backend for the host, as these ops otherwise.
//! Whatever [`decode`] declines falls back to a real `arm32_cpu::Cpu` step, so
//! the engine is correct by construction; the differential tests below pin it
//! against `Arm32CpuEngine`.

use arm32_cpu::{reg, util::bit::BitUtilExt};

/// A decoded Thumb operation with operand fields pre-extracted. Register fields
/// are indices into the flat `[u32; 16]` file. Straight-line variants advance
/// PC by 2; the two branch variants are always a block's final op and set PC.
#[derive(Clone, Copy)]
pub(crate) enum FastOp {
    /// Thumb `Shifted` (LSL/LSR/ASR immediate); sets NZC.
    Shift { op: u8, rd: u8, rs: u8, shift: u32 },
    /// `AddSub` with a register second operand; sets NZCV.
    AddSubReg { sub: bool, rd: u8, rs: u8, rn: u8 },
    /// `AddSub` with a 3-bit immediate second operand; sets NZCV.
    AddSubImm { sub: bool, rd: u8, rs: u8, imm: u32 },
    /// `ImmOp` (MOV/CMP/ADD/SUB with 8-bit immediate); sets NZCV.
    ImmOp { op: u8, rd: u8, imm: u32 },
    /// `AluOp` (the 16 data-processing ops).
    AluOp { op: u8, rd: u8, rs: u8 },
    /// High-register ADD/CMP/MOV where the destination is not PC (op 0/1/2).
    HiReg { op: u8, crd: u8, crs: u8 },
    /// `ldr rd, [pc, #imm]`.
    PcLoad { rd: u8, offset: u32 },
    /// `add rd, pc/sp, #imm` (`LoadAddr`).
    LoadAddr { sp: bool, rd: u8, imm: u32 },
    /// `add/sub sp, #imm` (`SpAdd`).
    SpAdd { sub: bool, imm: u32 },
    /// Halfword load/store, immediate offset (`HwXferI`).
    HwXferI { load: bool, rb: u8, rd: u8, offset: u32 },
    /// Word/byte load/store, immediate offset (`SingleXferI`).
    SingleXferI { load: bool, byte: bool, rb: u8, rd: u8, offset: u32 },
    /// Word/byte load/store, register offset (`SingleXferR`).
    SingleXferR { load: bool, byte: bool, ro: u8, rb: u8, rd: u8 },
    /// Halfword/signed load/store, register offset (`HwSgnXfer`).
    HwSgnXfer { s: bool, h: bool, ro: u8, rb: u8, rd: u8 },
    /// SP-relative word load/store (`SpXfer`).
    SpXfer { load: bool, rd: u8, offset: u32 },
    /// Conditional branch: if `cond` holds go to `target`, else on to the next
    /// instruction.
    CondBranch { cond: u8, target: u32 },
    /// Unconditional short branch.
    Branch { target: u32 },
    // PushPop, BranchExchange, BranchLink, BlockXfer and MovPc are built by the
    // JIT frontend, which is not wired up yet; `decode` declines all five.
    /// `push`/`pop` (`PushPop`). `extra` is the R bit: LR for a push, PC for a
    /// pop. A pop that includes PC (`load && extra`) writes a dynamic PC and so
    /// ends the trace. Produced only by the JIT frontend (`decode` declines it).
    #[allow(dead_code)]
    PushPop { load: bool, extra: bool, rlist: u8 },
    /// `bx`/`blx` register (`HiRegBx` op 3): jump to `rm`, switching ARM/Thumb
    /// from bit 0; `link` (BLX) also sets LR. Dynamic PC, so it ends the trace.
    /// Produced only by the JIT frontend.
    #[allow(dead_code)]
    BranchExchange { link: bool, rm: u8 },
    /// Long branch with link (`bl`/`blx` immediate). `target`/`ret` are the
    /// pre-computed jump target and return address; `exchange` (BLX) clears the
    /// Thumb bit. A 32-bit instruction, so it advances PC by 4. Ends the trace.
    /// Produced only by the JIT frontend.
    #[allow(dead_code)]
    BranchLink { exchange: bool, target: u32, ret: u32 },
    /// `ldmia`/`stmia rb!, {rlist}` (`BlockXfer`). Multi-register transfer with
    /// writeback to `rb`; `rlist` covers r0..r7 only (no PC), so it is
    /// straight-line. Produced only by the JIT frontend.
    #[allow(dead_code)]
    BlockXfer { load: bool, rb: u8, rlist: u8 },
    /// `mov pc, rm` (`HiRegBx` op 2 with destination PC). A computed branch that,
    /// unlike `bx`, does *not* interwork: the target is `rm & !1` and execution
    /// stays in Thumb. Dynamic PC, so it ends the trace. `rm` is never PC (that
    /// degenerate form is left to the interpreter). Produced only by the JIT
    /// frontend.
    #[allow(dead_code)]
    MovPc { rm: u8 },
}

/// Whether a decoded op continues the block or ends it (a branch).
pub(crate) enum Decoded {
    Straight(FastOp),
    Terminator(FastOp),
}

/// Whether a compiled op writes a non-linear (dynamic or far) PC and so must be
/// the last op in a JIT trace. `CondBranch` is excluded: it has a fall-through
/// and an in-range target can be linked inside the trace.
/// Used by the JIT frontend, which is not wired up yet.
#[allow(dead_code)]
pub(crate) fn ends_trace(op: &FastOp) -> bool {
    matches!(
        op,
        FastOp::Branch { .. }
            | FastOp::BranchExchange { .. }
            | FastOp::BranchLink { .. }
            | FastOp::MovPc { .. }
            | FastOp::PushPop { load: true, extra: true, .. }
    )
}

/// Decode one Thumb instruction word at `pc` into a fast op, or `None` if it is
/// outside the fast set (caller falls back to the interpreter for it).
pub(crate) fn decode(inst: u16, pc: u32) -> Option<Decoded> {
    let i = inst as u32;
    let hi = inst >> 8;
    // Mirror arm32_cpu's decode groups, but only for the fast subset.
    if hi & 0xe0 == 0x00 && hi & 0x18 != 0x18 {
        // Shifted (LSL/LSR/ASR imm). 0b000xx, excluding 0b00011 (AddSub).
        let op = i.extract(11, 2) as u8;
        return Some(Decoded::Straight(FastOp::Shift {
            op,
            rd: i.extract(0, 3) as u8,
            rs: i.extract(3, 3) as u8,
            shift: i.extract(6, 5),
        }));
    }
    if i & 0xf800 == 0x1800 {
        // AddSub
        let sub = i.get_bit(9) == 1;
        let rd = i.extract(0, 3) as u8;
        let rs = i.extract(3, 3) as u8;
        let rn = i.extract(6, 3);
        return Some(Decoded::Straight(if i.get_bit(10) == 0 {
            FastOp::AddSubReg { sub, rd, rs, rn: rn as u8 }
        } else {
            FastOp::AddSubImm { sub, rd, rs, imm: rn }
        }));
    }
    if i & 0xe000 == 0x2000 {
        // ImmOp
        return Some(Decoded::Straight(FastOp::ImmOp {
            op: i.extract(11, 2) as u8,
            rd: i.extract(8, 3) as u8,
            imm: i.extract(0, 8),
        }));
    }
    if i & 0xfc00 == 0x4000 {
        // AluOp
        return Some(Decoded::Straight(FastOp::AluOp {
            op: i.extract(6, 4) as u8,
            rd: i.extract(0, 3) as u8,
            rs: i.extract(3, 3) as u8,
        }));
    }
    if i & 0xfc00 == 0x4400 {
        // HiRegBx. Fast only for ADD/CMP/MOV (op 0/1/2) with destination != PC;
        // BX (op 3) and PC-writing forms fall back (control flow).
        let op = i.extract(8, 2) as u8;
        let crs = ((i.get_bit(6) * 8) + i.extract(3, 3)) as u8;
        let crd = ((i.get_bit(7) * 8) + i.extract(0, 3)) as u8;
        if op != 3 && crd != reg::PC {
            return Some(Decoded::Straight(FastOp::HiReg { op, crd, crs }));
        }
        return None;
    }
    if i & 0xf800 == 0x4800 {
        return Some(Decoded::Straight(FastOp::PcLoad {
            rd: i.extract(8, 3) as u8,
            offset: i.extract(0, 8),
        }));
    }
    if i & 0xf200 == 0x5000 {
        // SingleXferR
        return Some(Decoded::Straight(FastOp::SingleXferR {
            load: i.get_bit(11) == 1,
            byte: i.get_bit(10) == 1,
            ro: i.extract(6, 3) as u8,
            rb: i.extract(3, 3) as u8,
            rd: i.extract(0, 3) as u8,
        }));
    }
    if i & 0xf200 == 0x5200 {
        // HwSgnXfer
        return Some(Decoded::Straight(FastOp::HwSgnXfer {
            h: i.get_bit(11) == 1,
            s: i.get_bit(10) == 1,
            ro: i.extract(6, 3) as u8,
            rb: i.extract(3, 3) as u8,
            rd: i.extract(0, 3) as u8,
        }));
    }
    if i & 0xe000 == 0x6000 {
        // SingleXferI (word/byte); bit 12 selects byte, bit 11 load.
        return Some(Decoded::Straight(FastOp::SingleXferI {
            load: i.get_bit(11) == 1,
            byte: i.get_bit(12) == 1,
            rb: i.extract(3, 3) as u8,
            rd: i.extract(0, 3) as u8,
            offset: i.extract(6, 5),
        }));
    }
    if i & 0xf000 == 0x8000 {
        // HwXferI
        return Some(Decoded::Straight(FastOp::HwXferI {
            load: i.get_bit(11) == 1,
            rb: i.extract(3, 3) as u8,
            rd: i.extract(0, 3) as u8,
            offset: i.extract(6, 5),
        }));
    }
    if i & 0xf000 == 0x9000 {
        // SpXfer
        return Some(Decoded::Straight(FastOp::SpXfer {
            load: i.get_bit(11) == 1,
            rd: i.extract(8, 3) as u8,
            offset: i.extract(0, 8) * 4,
        }));
    }
    if i & 0xf000 == 0xa000 {
        // LoadAddr
        return Some(Decoded::Straight(FastOp::LoadAddr {
            sp: i.get_bit(11) == 1,
            rd: i.extract(8, 3) as u8,
            imm: i.extract(0, 8),
        }));
    }
    if i & 0xff00 == 0xb000 {
        // SpAdd
        return Some(Decoded::Straight(FastOp::SpAdd {
            sub: i.get_bit(7) == 1,
            imm: i.extract(0, 7) * 4,
        }));
    }
    if i & 0xf000 == 0xd000 {
        // CondBranch (0xdf00 SWI and 0xde00 undefined are excluded).
        let cond = i.extract(8, 4) as u8;
        if cond == 0xe || cond == 0xf {
            return None; // undefined / SWI encodings
        }
        let offset = i.extract(0, 8) as i8 as u32;
        let target = pc.wrapping_add(4).wrapping_add(offset << 1);
        return Some(Decoded::Terminator(FastOp::CondBranch { cond, target }));
    }
    if i & 0xf800 == 0xe000 {
        // Unconditional Branch
        let offset = (i.extract(0, 11) << 1).sign_extend(12);
        return Some(Decoded::Terminator(FastOp::Branch {
            target: pc.wrapping_add(4).wrapping_add(offset),
        }));
    }
    None
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;
    use alloc::vec::Vec;

    use super::{Decoded, decode};
    use crate::engine::{Arm32CpuEngine, ArmEngine, ArmRegister, EngineRunResult, JitEngine, MemoryPermission};

    const CODE: u32 = 0x1000;
    const DATA: u32 = 0x0010_0000;
    const DATA_SIZE: usize = 0x0010_0000;

    /// Map `ArmRegister` for a flat index 0..=15.
    fn reg_of(i: usize) -> ArmRegister {
        use ArmRegister::*;
        [R0, R1, R2, R3, R4, R5, R6, R7, R8, SB, SL, FP, IP, SP, LR, PC][i]
    }

    /// A comparable summary of a run's outcome.
    #[derive(PartialEq, Eq, Debug)]
    enum Outcome {
        End,
        CountExhausted,
        Svc(u32),
        Fault(u32),
        Fatal,
    }

    fn outcome(r: wie_util::Result<EngineRunResult>) -> Outcome {
        match r {
            Ok(EngineRunResult::End) => Outcome::End,
            Ok(EngineRunResult::CountExhausted) => Outcome::CountExhausted,
            Ok(EngineRunResult::Svc { category, .. }) => Outcome::Svc(category),
            Err(wie_util::WieError::InvalidMemoryAccess(a)) => Outcome::Fault(a),
            Err(_) => Outcome::Fatal,
        }
    }

    fn setup<E: ArmEngine>(mut e: E, code: &[u8], regs: &[u32; 15]) -> E {
        e.mem_map(0, 0x10000, MemoryPermission::ReadExecute);
        e.mem_map(DATA, DATA_SIZE, MemoryPermission::ReadWrite);
        e.mem_write(CODE, code).unwrap();
        for (i, &v) in regs.iter().enumerate() {
            e.reg_write(reg_of(i), v);
        }
        e.reg_write(ArmRegister::PC, CODE | 1); // enter Thumb at CODE
        e
    }

    /// Read r0..=r15 and CPSR.
    fn snapshot<E: ArmEngine>(e: &E) -> [u32; 17] {
        let mut s = [0u32; 17];
        for (i, slot) in s.iter_mut().enumerate().take(16) {
            *slot = e.reg_read(reg_of(i));
        }
        s[16] = e.reg_read(ArmRegister::Cpsr);
        s
    }

    /// Drive one engine to `end` (or a stop), returning outcome + register
    /// snapshot + the data region contents.
    fn drive<E: ArmEngine>(mut e: E, end: u32, count: u32) -> (Outcome, [u32; 17], Vec<u8>) {
        let out = loop {
            match e.run(end, count) {
                Ok(EngineRunResult::CountExhausted) => continue,
                other => break outcome(other),
            }
        };
        let regs = snapshot(&e);
        let mut mem = vec![0u8; DATA_SIZE];
        e.mem_read(DATA, DATA_SIZE, &mut mem).unwrap();
        (out, regs, mem)
    }

    /// Run identical setup through both engines and assert bit-for-bit identical
    /// outcome, registers and data memory.
    fn assert_same(code: &[u8], regs: &[u32; 15], end: u32) {
        // Drive each engine to completion sequentially (not both live at once):
        // each holds a 512 KiB inline page table, so keeping only one on the
        // stack at a time avoids overflowing the test thread. The odd budget
        // exercises the run/resume boundary.
        let (fo, fr, fm) = drive(setup(JitEngine::new(), code, regs), end, 37);
        let (so, sr, sm) = drive(setup(Arm32CpuEngine::new(), code, regs), end, 37);
        assert_eq!(so, fo, "outcome differs (interp {so:?} vs block engine {fo:?})");
        if sr != fr {
            for i in 0..17 {
                if sr[i] != fr[i] {
                    panic!("reg[{i}] differs: interp {:#010x} vs block engine {:#010x}", sr[i], fr[i]);
                }
            }
        }
        assert!(sm == fm, "data memory differs between engines");
    }

    #[test]
    fn mixed_fallback_and_branch() {
        // movs r0,#0x12 / push {r0} / adds r0,#1 / pop {r1} / adds r1,r1,r0 /
        // b end / movs r2,#0xff (skipped) / nop. Exercises the interpreter
        // fallback (push/pop), a block-ending branch, and register sync.
        #[rustfmt::skip]
        let code = [0x12,0x20, 0x01,0xb4, 0x40,0x1c, 0x02,0xbc, 0x09,0x18, 0x00,0xe0, 0xff,0x22, 0xc0,0x46];
        let mut regs = [0u32; 15];
        regs[13] = DATA + 0x8000; // sp
        assert_same(&code, &regs, CODE + 0xe);
    }

    /// Tiny deterministic PRNG (xorshift64).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn u16(&mut self) -> u16 {
            self.next() as u16
        }
        fn u32(&mut self) -> u32 {
            self.next() as u32
        }
    }

    #[test]
    #[ignore = "debug helper"]
    fn debug_find_bug() {
        // Mirror `fuzz_straight_line`'s program and register generation so a
        // failure there can be reproduced and bisected here.
        const OPS: usize = 40;
        for seed in 1..=2000u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut code = Vec::with_capacity(OPS * 2);
            while code.len() < OPS * 2 {
                let w = rng.u16();
                let pc = CODE + code.len() as u32;
                if let Some(Decoded::Straight(_)) = decode(w, pc) {
                    code.extend_from_slice(&w.to_le_bytes());
                }
            }
            let mut regs = [0u32; 15];
            for (i, r) in regs.iter_mut().enumerate().take(13) {
                *r = match rng.u32() & 3 {
                    0 => DATA + (rng.u32() & 0x3ff) * 4 + (i as u32) * 4,
                    1 => rng.u32() & 0xff,
                    2 => DATA + 0xfff8 + (rng.u32() & 0xf),
                    _ => rng.u32(),
                };
            }
            regs[13] = DATA + 0x8000;
            // Bisect on program length: find the smallest K where running just
            // the first K instructions already diverges.
            for k in 1..=OPS {
                let end = CODE + (k as u32) * 2;
                let (fo, fr, _) = drive(setup(JitEngine::new(), &code, &regs), end, 1_000_000);
                let (so, sr, _) = drive(setup(Arm32CpuEngine::new(), &code, &regs), end, 1_000_000);
                if fo != so || fr != sr {
                    let w = u16::from_le_bytes([code[(k - 1) * 2], code[(k - 1) * 2 + 1]]);
                    std::eprintln!(
                        "seed {seed}: diverges at op #{} word {w:#06x}; outcome interp {so:?} block engine {fo:?}",
                        k - 1
                    );
                    if fr != sr {
                        for i in 0..17 {
                            if fr[i] != sr[i] {
                                std::eprintln!("   reg[{i}] interp {:#010x} block engine {:#010x}", sr[i], fr[i]);
                            }
                        }
                    }
                    return;
                }
            }
        }
        std::eprintln!("no divergence found");
    }

    #[test]
    fn fuzz_straight_line() {
        // For many seeds, build a straight-line program of random instructions
        // drawn only from the fast set's *straight* ops (no branches, so it runs
        // linearly to `end`), with registers seeded to point into the data
        // region. Any divergence in final registers, memory, or fault behaviour
        // between the fast engine and the interpreter fails the test.
        const OPS: usize = 40;
        for seed in 1..=2000u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut code = Vec::with_capacity(OPS * 2);
            while code.len() < OPS * 2 {
                let w = rng.u16();
                let pc = CODE + code.len() as u32;
                if let Some(Decoded::Straight(_)) = decode(w, pc) {
                    code.extend_from_slice(&w.to_le_bytes());
                }
            }
            let mut regs = [0u32; 15];
            for (i, r) in regs.iter_mut().enumerate().take(13) {
                // Mix of bases: most near the data-region start (accesses stay
                // mapped), some small or near a page boundary to stress the
                // alignment and fault edges. Whatever the outcome — completion or
                // an identical fault — both engines must agree.
                *r = match rng.u32() & 3 {
                    0 => DATA + (rng.u32() & 0x3ff) * 4 + (i as u32) * 4,
                    1 => rng.u32() & 0xff,
                    2 => DATA + 0xfff8 + (rng.u32() & 0xf),
                    _ => rng.u32(),
                };
            }
            regs[13] = DATA + 0x8000; // sp
            let end = CODE + (OPS as u32) * 2;
            assert_same(&code, &regs, end);
        }
    }
}
