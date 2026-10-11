//! The block engine's backend for a host it does not generate machine code
//! for.
//!
//! Where [`super::x64`] and [`super::aarch64`] turn a decoded trace into a
//! native function, this keeps the trace as the decoded ops themselves and
//! runs them. Everything around a block is the same engine: the same decoders,
//! the same block cache and its invalidation, the same answered `svc`s, the
//! same interpreter fallback for whatever a trace does not cover. So a host
//! without the JIT - a 32-bit ARM handset, or one that refuses executable
//! memory to an app - still decodes each instruction once rather than every
//! time it runs. It used to step `arm32_cpu` one instruction at a time, which
//! re-fetches, re-decodes and re-dispatches every instruction through the
//! banked register file.
//!
//! The protocol is the native backends' exactly, and so is every op: a block
//! charges its budget per basic block, a fault or a store into compiled code
//! leaves `regs[15]` at the next instruction and exits with
//! [`exit::FAULT`]/[`exit::SMC`], and a load writes its destination before
//! the fault is noticed. The emitters in [`super::x64`] are the reference each
//! arm here follows. The engine's differential tests run against this backend
//! whenever the JIT is not built.

use alloc::{boxed::Box, vec::Vec};

use arm32_cpu::util::{
    arm::{arg_shift, arg_shift0, build_flags},
    bit::BitUtilExt,
};

use crate::engine::fast::{FastOp, ends_trace};

use super::arm_frontend::{ArmOp, Off, Op2, arm_ends_trace};
use super::{JitCtx, exit, jit_alu_shift, jit_arm_multiply, jit_arm_shift, jit_load8, jit_load16, jit_load32, jit_store8, jit_store16, jit_store32};

/// Whether each condition holds, by condition and then by the NZCV nibble -
/// the interpreter's own `cond_met`, worked out once.
static CONDITIONS: [[bool; 16]; 16] = {
    let mut table = [[false; 16]; 16];
    let mut cond = 0;
    while cond < 16 {
        let mut flags = 0;
        while flags < 16 {
            let (n, z, c, v) = (flags & 8 != 0, flags & 4 != 0, flags & 2 != 0, flags & 1 != 0);
            table[cond][flags] = match cond {
                0x0 => z,
                0x1 => !z,
                0x2 => c,
                0x3 => !c,
                0x4 => n,
                0x5 => !n,
                0x6 => v,
                0x7 => !v,
                0x8 => c && !z,
                0x9 => !c || z,
                0xA => n == v,
                0xB => n != v,
                0xC => !z && n == v,
                0xD => z || n != v,
                _ => true,
            };
            flags += 1;
        }
        cond += 1;
    }
    table
};

#[inline(always)]
fn cond_met(cond: u8, cpsr: u32) -> bool {
    CONDITIONS[(cond & 15) as usize][(cpsr >> 28) as usize]
}

/// A Thumb step's in-trace branch target, when its target is outside the trace.
const NO_TARGET: u16 = u16::MAX;

/// One Thumb op of a trace, with what running it needs worked out in advance.
#[derive(Clone, Copy)]
struct ThumbStep {
    op: FastOp,
    /// The instructions this step's basic block charges to the budget, when a
    /// basic block starts here; zero otherwise.
    charge: i32,
    /// Index of the branch target inside the trace, for an in-trace branch.
    target: u16,
}

/// A decoded trace, ready to run.
pub(crate) struct Code(Trace);

enum Trace {
    Thumb {
        steps: Box<[ThumbStep]>,
        start_pc: u32,
        end_pc: u32,
    },
    /// `body` runs straight through; `terminator`, when the trace has one, is
    /// its last op.
    Arm {
        body: Box<[ArmOp]>,
        terminator: Option<ArmOp>,
        start_pc: u32,
        end_pc: u32,
    },
}

pub(crate) fn run_block(code: &Code, ctx: &mut JitCtx) -> u32 {
    match &code.0 {
        Trace::Thumb { steps, start_pc, end_pc } => run_thumb(steps, *start_pc, *end_pc, ctx),
        Trace::Arm {
            body,
            terminator,
            start_pc,
            end_pc,
        } => run_arm(body, terminator.as_ref(), *start_pc, *end_pc, ctx),
    }
}

/// Takes a decoded Thumb trace up to and including its first terminator, as
/// the native backends compile it, and works out its basic blocks.
pub(crate) fn compile_block(ops: &[FastOp], start_pc: u32) -> Option<(Code, usize)> {
    let limit = ops.iter().position(ends_trace).map_or(ops.len(), |i| i + 1);
    if limit == 0 {
        return None;
    }
    let end_pc = start_pc.wrapping_add(2 * limit as u32);
    let in_range = |t: u32| t >= start_pc && t < end_pc && (t.wrapping_sub(start_pc)) & 1 == 0;
    let index_of = |t: u32| (t.wrapping_sub(start_pc) / 2) as usize;
    let ops = &ops[..limit];

    // Basic-block boundaries, as the native backends draw them: the trace
    // start, every in-trace branch target, and the op after every branch.
    let mut boundary = alloc::vec![false; limit];
    boundary[0] = true;
    for (i, op) in ops.iter().enumerate() {
        if let FastOp::CondBranch { target, .. } | FastOp::Branch { target } = *op {
            if in_range(target) {
                boundary[index_of(target)] = true;
            }
            if i + 1 < limit {
                boundary[i + 1] = true;
            }
        }
    }

    let steps: Vec<ThumbStep> = ops
        .iter()
        .enumerate()
        .map(|(i, &op)| {
            let charge = if boundary[i] {
                let next = (i + 1..limit).find(|&j| boundary[j]).unwrap_or(limit);
                (next - i) as i32
            } else {
                0
            };
            let target = match op {
                FastOp::CondBranch { target, .. } | FastOp::Branch { target } if in_range(target) => index_of(target) as u16,
                _ => NO_TARGET,
            };
            ThumbStep { op, charge, target }
        })
        .collect();

    Some((
        Code(Trace::Thumb {
            steps: steps.into_boxed_slice(),
            start_pc,
            end_pc,
        }),
        limit,
    ))
}

/// Takes a decoded ARM trace up to and including its first terminator. An ARM
/// trace is one basic block, as the native backends compile it.
pub(crate) fn compile_arm_block(ops: &[ArmOp], start_pc: u32) -> Option<(Code, usize)> {
    let limit = ops.iter().position(arm_ends_trace).map_or(ops.len(), |i| i + 1);
    if limit == 0 {
        return None;
    }

    let (body, terminator) = match ops[..limit].split_last() {
        Some((last, body)) if arm_ends_trace(last) => (body, Some(*last)),
        _ => (&ops[..limit], None),
    };

    Some((
        Code(Trace::Arm {
            body: body.into(),
            terminator,
            start_pc,
            end_pc: start_pc.wrapping_add(4 * limit as u32),
        }),
        limit,
    ))
}

fn run_thumb(steps: &[ThumbStep], start_pc: u32, end_pc: u32, ctx: &mut JitCtx) -> u32 {
    let mut i = 0;
    loop {
        let Some(&step) = steps.get(i) else {
            ctx.regs[15] = end_pc;
            return exit::CONTINUE;
        };
        let pc = start_pc.wrapping_add(2 * i as u32);

        // Yield to the dispatcher when the budget is spent; otherwise charge
        // the basic block starting here.
        if step.charge != 0 {
            if ctx.budget <= 0 {
                ctx.regs[15] = pc;
                return exit::CONTINUE;
            }
            ctx.budget -= step.charge;
        }

        match step.op {
            FastOp::CondBranch { cond, target, .. } => {
                if cond_met(cond, ctx.cpsr) {
                    if step.target != NO_TARGET {
                        i = step.target as usize;
                        continue;
                    }
                    ctx.regs[15] = target;
                    return exit::CONTINUE;
                }
            }
            FastOp::Branch { target } => {
                if step.target != NO_TARGET {
                    i = step.target as usize;
                    continue;
                }
                ctx.regs[15] = target;
                return exit::CONTINUE;
            }
            op => {
                if let Some(reason) = exec_thumb(ctx, op, pc) {
                    return reason;
                }
            }
        }
        i += 1;
    }
}

fn run_arm(body: &[ArmOp], terminator: Option<&ArmOp>, start_pc: u32, end_pc: u32, ctx: &mut JitCtx) -> u32 {
    // The whole trace is one basic block, charged once.
    if ctx.budget <= 0 {
        ctx.regs[15] = start_pc;
        return exit::CONTINUE;
    }
    ctx.budget -= (body.len() + terminator.is_some() as usize) as i32;

    let mut pc = start_pc;
    for op in body {
        if let Some(reason) = exec_arm(ctx, op, pc) {
            return reason;
        }
        pc = pc.wrapping_add(4);
    }

    match terminator {
        Some(op) => arm_terminator(ctx, op, pc, end_pc),
        None => {
            ctx.regs[15] = end_pc;
            exit::CONTINUE
        }
    }
}

// --- guest state -------------------------------------------------------------

#[inline(always)]
fn carry(ctx: &JitCtx) -> u32 {
    (ctx.cpsr >> 29) & 1
}

#[inline(always)]
fn overflow(ctx: &JitCtx) -> u32 {
    (ctx.cpsr >> 28) & 1
}

/// N and Z from `res`, with the given V and C.
#[inline(always)]
fn set_flags(ctx: &mut JitCtx, res: u32, v: u32, c: u32) {
    let flags = build_flags(v, c, (res == 0) as u32, res.is_neg() as u32);
    ctx.cpsr = (ctx.cpsr & !0xf000_0000) | (flags << 28);
}

/// Switches to Thumb or ARM by `thumb`.
#[inline(always)]
fn set_thumb(ctx: &mut JitCtx, thumb: u32) {
    ctx.cpsr = (ctx.cpsr & !(1 << 5)) | (thumb << 5);
}

// The memory helpers are the native backends' own, so a fault and a store into
// compiled code are noticed exactly as they are there.

#[inline(always)]
fn load(ctx: &mut JitCtx, size: u8, addr: u32) -> u32 {
    let ctx = ctx as *mut JitCtx;
    // SAFETY: `run` points `ctx.mem` and `ctx.code_pages` at the engine's own
    // for the whole of a run, and no other reference to the context is live.
    unsafe {
        match size {
            8 => jit_load8(ctx, addr),
            16 => jit_load16(ctx, addr),
            _ => jit_load32(ctx, addr),
        }
    }
}

#[inline(always)]
fn store(ctx: &mut JitCtx, size: u8, addr: u32, value: u32) {
    let ctx = ctx as *mut JitCtx;
    // SAFETY: as for `load`.
    unsafe {
        match size {
            8 => jit_store8(ctx, addr, value),
            16 => jit_store16(ctx, addr, value),
            _ => jit_store32(ctx, addr, value),
        }
    }
}

/// The exit for a fault in the instruction before `next`.
#[inline(always)]
fn after_load(ctx: &mut JitCtx, next: u32) -> Option<u32> {
    if ctx.faulted != 0 {
        ctx.regs[15] = next;
        return Some(exit::FAULT);
    }
    None
}

/// The exit for a fault in, or a store into compiled code by, the
/// instruction before `next`.
#[inline(always)]
fn after_store(ctx: &mut JitCtx, next: u32) -> Option<u32> {
    if let Some(reason) = after_load(ctx, next) {
        return Some(reason);
    }
    if ctx.smc != 0 {
        ctx.regs[15] = next;
        return Some(exit::SMC);
    }
    None
}

// --- Thumb ---------------------------------------------------------------------

/// Runs one Thumb op at `pc` other than a short branch, and returns the
/// block's exit if it ends here.
#[inline(always)]
fn exec_thumb(ctx: &mut JitCtx, op: FastOp, pc: u32) -> Option<u32> {
    let next = pc.wrapping_add(2);
    match op {
        FastOp::Shift { op, rd, rs, shift } => {
            let val = ctx.regs[rs as usize];
            let (res, c) = if shift == 0 {
                arg_shift0(val, op as u32, carry(ctx))
            } else {
                arg_shift(val, shift, op as u32)
            };
            ctx.regs[rd as usize] = res;
            set_flags(ctx, res, overflow(ctx), c);
        }
        FastOp::AddSubReg { sub, rd, rs, rn } => {
            let (a, b) = (ctx.regs[rs as usize], ctx.regs[rn as usize]);
            let (res, v, c) = if sub { a.sub_flags(b, 0) } else { a.add_flags(b, 0) };
            ctx.regs[rd as usize] = res;
            set_flags(ctx, res, v, c);
        }
        FastOp::AddSubImm { sub, rd, rs, imm } => {
            let a = ctx.regs[rs as usize];
            let (res, v, c) = if sub { a.sub_flags(imm, 0) } else { a.add_flags(imm, 0) };
            ctx.regs[rd as usize] = res;
            set_flags(ctx, res, v, c);
        }
        FastOp::ImmOp { op, rd, imm } => {
            let (res, v, c) = match op {
                0 => (imm, overflow(ctx), carry(ctx)),
                1 | 3 => ctx.regs[rd as usize].sub_flags(imm, 0),
                _ => ctx.regs[rd as usize].add_flags(imm, 0),
            };
            if op != 1 {
                ctx.regs[rd as usize] = res;
            }
            set_flags(ctx, res, v, c);
        }
        FastOp::AluOp { op, rd, rs } => {
            let (c, v) = (carry(ctx), overflow(ctx));
            let (vald, vals) = (ctx.regs[rd as usize], ctx.regs[rs as usize]);
            let (res, new_v, new_c) = match op {
                0x0 | 0x8 => (vald & vals, v, c),
                0x1 => (vald ^ vals, v, c),
                0x2 | 0x3 | 0x4 | 0x7 => {
                    let shift_type = ((op >> 1) & 2) | (op & 1);
                    let shifted = jit_alu_shift(vald, vals, shift_type as u32, c);
                    (shifted as u32, v, (shifted >> 32) as u32)
                }
                0x5 => vald.add_flags(vals, c),
                0x6 => vald.sub_flags(vals, 1 - c),
                0x9 => 0u32.sub_flags(vals, 0),
                0xA => vald.sub_flags(vals, 0),
                0xB => vald.add_flags(vals, 0),
                0xC => (vald | vals, v, c),
                0xD => (vald.wrapping_mul(vals), v, 0),
                0xE => (vald & !vals, v, c),
                _ => (!vals, v, c),
            };
            if !matches!(op, 0x8 | 0xA | 0xB) {
                ctx.regs[rd as usize] = res;
            }
            set_flags(ctx, res, new_v, new_c);
        }
        FastOp::HiReg { op, crd, crs } => {
            // The decode never makes the destination the PC; a PC source reads
            // the pipeline's `pc + 4`.
            let vals = if crs == 15 { pc.wrapping_add(4) } else { ctx.regs[crs as usize] };
            let vald = ctx.regs[crd as usize];
            match op {
                0 => ctx.regs[crd as usize] = vald.wrapping_add(vals),
                1 => {
                    let (res, v, c) = vald.sub_flags(vals, 0);
                    set_flags(ctx, res, v, c);
                }
                _ => ctx.regs[crd as usize] = vals,
            }
        }
        FastOp::PcLoad { rd, offset } => {
            let addr = pc.wrapping_add(4).wrapping_add(offset * 4) & !3;
            ctx.regs[rd as usize] = load(ctx, 32, addr);
            return after_load(ctx, next);
        }
        FastOp::LoadAddr { sp, rd, imm } => {
            let base = if sp { ctx.regs[13] } else { pc.wrapping_add(4) & !2 };
            ctx.regs[rd as usize] = base.wrapping_add(imm * 4);
        }
        FastOp::SpAdd { sub, imm } => {
            let sp = ctx.regs[13];
            ctx.regs[13] = if sub { sp.wrapping_sub(imm) } else { sp.wrapping_add(imm) };
        }
        FastOp::HwXferI {
            load: is_load,
            rb,
            rd,
            offset,
        } => {
            let addr = ctx.regs[rb as usize].wrapping_add(offset * 2) & !1;
            return thumb_xfer(ctx, is_load, 16, false, rd, addr, next);
        }
        FastOp::SingleXferI {
            load: is_load,
            byte,
            rb,
            rd,
            offset,
        } => {
            let scaled = if byte { offset } else { offset * 4 };
            let addr = ctx.regs[rb as usize].wrapping_add(scaled);
            return thumb_xfer(ctx, is_load, if byte { 8 } else { 32 }, false, rd, addr, next);
        }
        FastOp::SingleXferR {
            load: is_load,
            byte,
            ro,
            rb,
            rd,
        } => {
            let addr = ctx.regs[rb as usize].wrapping_add(ctx.regs[ro as usize]);
            return thumb_xfer(ctx, is_load, if byte { 8 } else { 32 }, false, rd, addr, next);
        }
        FastOp::HwSgnXfer { s, h, ro, rb, rd } => {
            let addr = ctx.regs[rb as usize].wrapping_add(ctx.regs[ro as usize]);
            // Halfword accesses align the address; the signed-byte load does not.
            return match (s, h) {
                (false, false) => thumb_xfer(ctx, false, 16, false, rd, addr & !1, next),
                (false, true) => thumb_xfer(ctx, true, 16, false, rd, addr & !1, next),
                (true, false) => thumb_xfer(ctx, true, 8, true, rd, addr, next),
                (true, true) => thumb_xfer(ctx, true, 16, true, rd, addr & !1, next),
            };
        }
        FastOp::SpXfer { load: is_load, rd, offset } => {
            let addr = ctx.regs[13].wrapping_add(offset);
            return thumb_xfer(ctx, is_load, 32, false, rd, addr, next);
        }
        FastOp::PushPop { load: is_load, extra, rlist } => return push_pop(ctx, is_load, extra, rlist, next),
        FastOp::BlockXfer { load: is_load, rb, rlist } => return block_xfer(ctx, is_load, rb, rlist, next),
        FastOp::BranchExchange { link, rm } => {
            let val = if rm == 15 { pc.wrapping_add(4) } else { ctx.regs[rm as usize] };
            branch_exchange(ctx, val);
            if link {
                ctx.regs[14] = next | 1;
            }
            return Some(exit::CONTINUE);
        }
        FastOp::MovPc { rm } => {
            ctx.regs[15] = ctx.regs[rm as usize] & !1;
            return Some(exit::CONTINUE);
        }
        FastOp::BranchLink { exchange, target, ret } => {
            ctx.regs[14] = ret;
            ctx.regs[15] = target;
            if exchange {
                set_thumb(ctx, 0);
            }
            return Some(exit::CONTINUE);
        }
        FastOp::CondBranch { .. } | FastOp::Branch { .. } => unreachable!("handled by run_thumb"),
    }
    None
}

/// A single Thumb load or store of `size` bits at `addr`.
#[inline(always)]
fn thumb_xfer(ctx: &mut JitCtx, is_load: bool, size: u8, signed: bool, rd: u8, addr: u32, next: u32) -> Option<u32> {
    if is_load {
        let mut value = load(ctx, size, addr);
        if signed {
            value = if size == 8 {
                value as u8 as i8 as u32
            } else {
                value as u16 as i16 as u32
            };
        }
        // Written before the fault is noticed, as the interpreter does.
        ctx.regs[rd as usize] = value;
        after_load(ctx, next)
    } else {
        store(ctx, size, addr, ctx.regs[rd as usize]);
        after_store(ctx, next)
    }
}

/// `push`/`pop`. Every access runs and SP moves whatever faults, with the
/// fault noticed once at the end; a `pop` into the PC ends the trace.
fn push_pop(ctx: &mut JitCtx, is_load: bool, extra: bool, rlist: u8, next: u32) -> Option<u32> {
    let count = rlist.count_ones() + extra as u32;
    let sp = ctx.regs[13];
    let pop_pc = is_load && extra;
    // Ascending order; the extra register (LR for push, PC for pop) is last.
    let registers = (0..8u8)
        .filter(|&r| rlist & (1 << r) != 0)
        .chain(extra.then_some(if is_load { 15 } else { 14 }));

    if is_load {
        for (i, r) in registers.enumerate() {
            let value = load(ctx, 32, sp.wrapping_add(4 * i as u32));
            if r == 15 {
                set_thumb(ctx, value & 1);
                ctx.regs[15] = value & !1;
            } else {
                ctx.regs[r as usize] = value;
            }
        }
        ctx.regs[13] = sp.wrapping_add(4 * count);
    } else {
        let base = sp.wrapping_sub(4 * count);
        for (i, r) in registers.enumerate() {
            store(ctx, 32, base.wrapping_add(4 * i as u32), ctx.regs[r as usize]);
        }
        ctx.regs[13] = base;
    }

    if ctx.faulted != 0 {
        // A pop into the PC has already written the PC it popped.
        if !pop_pc {
            ctx.regs[15] = next;
        }
        return Some(exit::FAULT);
    }
    if !is_load && ctx.smc != 0 {
        ctx.regs[15] = next;
        return Some(exit::SMC);
    }
    pop_pc.then_some(exit::CONTINUE)
}

/// `ldmia`/`stmia rb!`. Writeback comes first, as in the interpreter, and the
/// lowest slot of a store of `rb` stores the original base.
fn block_xfer(ctx: &mut JitCtx, is_load: bool, rb: u8, rlist: u8, next: u32) -> Option<u32> {
    let base = ctx.regs[rb as usize];
    ctx.regs[rb as usize] = base.wrapping_add(4 * rlist.count_ones());

    for (i, r) in (0..8u8).filter(|&r| rlist & (1 << r) != 0).enumerate() {
        let addr = base.wrapping_add(4 * i as u32);
        if is_load {
            ctx.regs[r as usize] = load(ctx, 32, addr);
        } else {
            let value = if r == rb && i == 0 { base } else { ctx.regs[r as usize] };
            store(ctx, 32, addr, value);
        }
    }

    if is_load { after_load(ctx, next) } else { after_store(ctx, next) }
}

/// `bx`: the target's bit 0 picks the state, and the PC drops the bits that
/// state does not have.
#[inline(always)]
fn branch_exchange(ctx: &mut JitCtx, val: u32) {
    let thumb = val & 1;
    ctx.regs[15] = val & (0xffff_fffc | (thumb << 1));
    set_thumb(ctx, thumb);
}

// --- ARM -----------------------------------------------------------------------

/// Operand 2's value and the shifter's carry-out.
#[inline(always)]
fn operand2(ctx: &JitCtx, op2: Op2) -> (u32, u32) {
    let shifted = match op2 {
        Op2::Imm { val, carry } => return (val, carry),
        Op2::ShiftImm { rm, ty, amount } => jit_arm_shift(ctx.regs[rm as usize], ty as u32, amount as u32, 0, carry(ctx)),
        Op2::ShiftReg { rm, ty, rs } => jit_arm_shift(ctx.regs[rm as usize], ty as u32, ctx.regs[rs as usize] & 0xff, 1, carry(ctx)),
    };
    (shifted as u32, (shifted >> 32) as u32)
}

/// A load/store offset's value.
#[inline(always)]
fn offset_value(ctx: &JitCtx, offset: Off) -> u32 {
    match offset {
        Off::Imm(v) => v,
        Off::ShiftImm { rm, ty, amount } => jit_arm_shift(ctx.regs[rm as usize], ty as u32, amount as u32, 0, carry(ctx)) as u32,
    }
}

/// The data-processing ALU, setting the flags when `s`. Returns the result,
/// which the caller writes back unless the op only compares.
#[inline(always)]
fn alu(ctx: &mut JitCtx, opcode: u8, s: bool, rn: u8, val: u32, shifter_carry: u32) -> u32 {
    let a = ctx.regs[rn as usize];
    let (c, v) = (carry(ctx), overflow(ctx));
    let (res, new_v, new_c) = match opcode {
        0x0 | 0x8 => (a & val, v, shifter_carry),
        0x1 | 0x9 => (a ^ val, v, shifter_carry),
        0x2 | 0xA => a.sub_flags(val, 0),
        0x3 => val.sub_flags(a, 0),
        0x4 | 0xB => a.add_flags(val, 0),
        0x5 => a.add_flags(val, c),
        0x6 => a.sub_flags(val, 1 - c),
        0x7 => val.sub_flags(a, 1 - c),
        0xC => (a | val, v, shifter_carry),
        0xD => (val, v, shifter_carry),
        0xE => (a & !val, v, shifter_carry),
        _ => (!val, v, shifter_carry),
    };
    if s {
        set_flags(ctx, res, new_v, new_c);
    }
    res
}

/// Runs one straight-line ARM op at `pc`, and returns the block's exit if it
/// ends here.
#[inline(always)]
fn exec_arm(ctx: &mut JitCtx, op: &ArmOp, pc: u32) -> Option<u32> {
    let next = pc.wrapping_add(4);
    match *op {
        ArmOp::DataProc {
            cond,
            opcode,
            s,
            rd,
            rn,
            op2,
        } => {
            if cond_met(cond, ctx.cpsr) {
                let (val, shifter_carry) = operand2(ctx, op2);
                let res = alu(ctx, opcode, s, rn, val, shifter_carry);
                if !matches!(opcode, 0x8..=0xB) {
                    ctx.regs[rd as usize] = res;
                }
            }
            None
        }
        ArmOp::Multiply { cond, inst } => {
            if cond_met(cond, ctx.cpsr) {
                // SAFETY: `ctx` is the live context of this run.
                unsafe { jit_arm_multiply(ctx as *mut JitCtx, inst) };
            }
            None
        }
        ArmOp::LoadStore {
            cond,
            load: is_load,
            byte,
            rd,
            rn,
            pre,
            up,
            wb,
            base_pc,
            offset,
        } => {
            if !cond_met(cond, ctx.cpsr) {
                return None;
            }
            let size = if byte { 8 } else { 32 };
            arm_xfer(ctx, is_load, size, false, rd, rn, pre, up, wb, base_pc, offset, next)
        }
        ArmOp::HalfXfer {
            cond,
            load: is_load,
            signed,
            halfword,
            rd,
            rn,
            pre,
            up,
            wb,
            base_pc,
            offset,
        } => {
            if !cond_met(cond, ctx.cpsr) {
                return None;
            }
            // A store is always a halfword (STRH).
            let size = if halfword || !is_load { 16 } else { 8 };
            arm_xfer(ctx, is_load, size, signed, rd, rn, pre, up, wb, base_pc, offset, next)
        }
        ArmOp::Block { cond, .. } => {
            if !cond_met(cond, ctx.cpsr) {
                return None;
            }
            arm_block(ctx, op, pc)
        }
        _ => unreachable!("terminator in a straight-line slot"),
    }
}

/// A single ARM load or store. Writeback follows the interpreter: a
/// post-index or the W bit updates `rn`, except for a load into `rn`, and a
/// store takes its value before writeback, so `str rX, [rX], #n` stores the
/// original.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn arm_xfer(
    ctx: &mut JitCtx,
    is_load: bool,
    size: u8,
    signed: bool,
    rd: u8,
    rn: u8,
    pre: bool,
    up: bool,
    wb: bool,
    base_pc: Option<u32>,
    offset: Off,
    next: u32,
) -> Option<u32> {
    let off = offset_value(ctx, offset);
    let base = base_pc.unwrap_or(ctx.regs[rn as usize]);
    let post = if up { base.wrapping_add(off) } else { base.wrapping_sub(off) };
    let mut addr = if pre { post } else { base };
    // A halfword access ignores bit 0 of the address; writeback keeps it.
    if size == 16 {
        addr &= !1;
    }
    let writeback = (!pre || wb) && base_pc.is_none() && (!is_load || rd != rn);

    if is_load {
        if writeback {
            ctx.regs[rn as usize] = post;
        }
        let mut value = load(ctx, size, addr);
        if signed {
            value = if size == 8 {
                value as u8 as i8 as u32
            } else {
                value as u16 as i16 as u32
            };
        }
        ctx.regs[rd as usize] = value;
        after_load(ctx, next)
    } else {
        let value = ctx.regs[rd as usize];
        if writeback {
            ctx.regs[rn as usize] = post;
        }
        store(ctx, size, addr, value);
        after_store(ctx, next)
    }
}

/// LDM/STM's register-list transfer. Writeback comes first; the lowest slot of
/// a store of `rn` with writeback stores the original base, and a stored PC
/// reads `pc + 12`. A loaded PC is left in `regs[15]` for the terminator.
fn arm_block(ctx: &mut JitCtx, op: &ArmOp, pc: u32) -> Option<u32> {
    let ArmOp::Block {
        load: is_load,
        rn,
        pre,
        up,
        wb,
        reglist,
        ..
    } = *op
    else {
        unreachable!()
    };
    let total = reglist.count_ones();
    let base = ctx.regs[rn as usize];
    if wb {
        ctx.regs[rn as usize] = if up {
            base.wrapping_add(4 * total)
        } else {
            base.wrapping_sub(4 * total)
        };
    }
    let first = if up { base } else { base.wrapping_sub(4 * total) };
    let pre_increment = (pre == up) as u32;

    for (i, r) in (0..16u8).filter(|&r| reglist & (1 << r) != 0).enumerate() {
        let addr = first.wrapping_add(4 * (i as u32 + pre_increment));
        if is_load {
            ctx.regs[r as usize] = load(ctx, 32, addr);
        } else {
            let value = if r == 15 {
                pc.wrapping_add(12)
            } else if r == rn && wb && i == 0 {
                base
            } else {
                ctx.regs[r as usize]
            };
            store(ctx, 32, addr, value);
        }
    }

    let next = pc.wrapping_add(4);
    if is_load { after_load(ctx, next) } else { after_store(ctx, next) }
}

/// A trace's last op: a branch, `bx`, a data-processing op into the PC, or an
/// LDM that loads it. One whose condition fails carries on at `end_pc`.
fn arm_terminator(ctx: &mut JitCtx, op: &ArmOp, pc: u32, end_pc: u32) -> u32 {
    let cond = match *op {
        ArmOp::Branch { cond, .. } | ArmOp::BranchEx { cond, .. } | ArmOp::DataProc { cond, .. } | ArmOp::Block { cond, .. } => cond,
        _ => 0xe,
    };
    if !cond_met(cond, ctx.cpsr) {
        ctx.regs[15] = end_pc;
        return exit::CONTINUE;
    }

    match *op {
        ArmOp::Branch {
            target, link, ret, to_thumb, ..
        } => {
            if link {
                ctx.regs[14] = ret;
            }
            if to_thumb {
                set_thumb(ctx, 1);
            }
            ctx.regs[15] = target;
        }
        ArmOp::BranchEx { rm, .. } => {
            let val = if rm == 15 { pc.wrapping_add(8) } else { ctx.regs[rm as usize] };
            branch_exchange(ctx, val);
        }
        ArmOp::DataProc { opcode, rn, op2, .. } => {
            // Into the PC: the result is the branch target, and the flag-setting
            // form never reaches here (the decode leaves it to the interpreter).
            let (val, shifter_carry) = operand2(ctx, op2);
            ctx.regs[15] = alu(ctx, opcode, false, rn, val, shifter_carry);
        }
        ArmOp::Block { .. } => {
            if let Some(reason) = arm_block(ctx, op, pc) {
                return reason;
            }
        }
        _ => unreachable!(),
    }
    exit::CONTINUE
}

#[cfg(test)]
mod tests {
    /// The condition table is the interpreter's predicate for every condition
    /// and every combination of flags.
    #[test]
    fn the_condition_table_is_the_interpreters_predicate() {
        for cond in 0..16u8 {
            for flags in 0..16u32 {
                let cpsr = (flags << 28) | 0x10;
                assert_eq!(
                    super::cond_met(cond, cpsr),
                    arm32_cpu::util::arm::cond_met(cond as u32, cpsr),
                    "cond {cond:#x}, NZCV {flags:04b}"
                );
            }
        }
    }
}
