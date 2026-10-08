mod arm32_cpu;
#[cfg(test)]
mod bench;
#[cfg(not(target_arch = "wasm32"))]
mod debugged_arm32_cpu;
mod fast;
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64")))]
mod jit;

use wie_util::{AsAny, Result};

pub use arm32_cpu::Arm32CpuEngine;
#[cfg(not(target_arch = "wasm32"))]
pub use debugged_arm32_cpu::DebuggedArm32CpuEngine;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use debugged_arm32_cpu::{DebugBreakpointKind, DebugInner, DebugSignal, DebugStopReason};
pub use fast::FastCpuEngine;
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64")))]
pub use jit::JitEngine;

/// A platform call an engine may answer on its own, without leaving `run`.
///
/// Each is a C library routine over guest memory whose whole effect is that
/// memory: [`ArmCore::make_intrinsic_svc_stub`](crate::ArmCore::make_intrinsic_svc_stub)
/// says which stub stands for which.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MemoryIntrinsic {
    /// `memcpy(dst, src, len)`.
    Copy,
    /// `memmove(dst, src, len)`.
    Move,
    /// `memset(dst, value, len)`.
    Set,
}

pub enum EngineRunResult {
    End,
    CountExhausted,
    Svc { category: u32, lr: u32, spsr: u32 },
}

pub trait ArmEngine: Send + AsAny {
    fn run(&mut self, end: u32, count: u32) -> Result<EngineRunResult>;
    fn reg_write(&mut self, reg: ArmRegister, value: u32);
    fn reg_read(&self, reg: ArmRegister) -> u32;
    fn mem_map(&mut self, address: u32, size: usize, permission: MemoryPermission);
    fn mem_write(&mut self, address: u32, data: &[u8]) -> Result<()>;
    fn mem_read(&mut self, address: u32, size: usize, result: &mut [u8]) -> Result<usize>;
    fn is_mapped(&self, address: u32, size: usize) -> bool;

    /// Answer the `svc` at `svc_address` as `kind` without returning from
    /// `run`, where the engine can. An engine that cannot leaves the call to
    /// the platform's handler, which answers it the same way.
    fn set_svc_intrinsic(&mut self, _svc_address: u32, _kind: MemoryIntrinsic) {}
}

#[allow(clippy::enum_variant_names)]
pub enum MemoryPermission {
    ReadExecute = 5,
    ReadWrite = 6,
    ReadWriteExecute = 7,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ArmRegister {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    SB,
    SL,
    FP,
    IP,
    SP,
    LR,
    PC,
    Cpsr,
}
