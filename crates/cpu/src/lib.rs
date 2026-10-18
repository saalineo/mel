//! # MEL CPU
//!
//! Universal CPU backend abstractions, execution step types, and emulator engine bridges.

pub mod arm64;
pub mod hook;
pub mod mock;
pub mod powerpc64;
pub mod riscv64;
pub mod traits;
pub mod x86_64;

pub use arm64::Arm64Cpu;
pub use hook::{register_bus_hooks, CpuBusBridge};
pub use mock::MockCpu;
pub use powerpc64::PowerIso64Cpu;
pub use riscv64::{
    Riscv64Cpu, MCAUSE_INTERRUPT_BIT, MCAUSE_MEI, MCAUSE_MSI, MCAUSE_MTI, MIE_MEIE, MIE_MSIE,
    MIE_MTIE, MIP_MEIP, MIP_MSIP, MIP_MTIP, MSTATUS_MIE, MSTATUS_MPIE, MSTATUS_MPP_M,
    MSTATUS_MPP_MASK,
};
pub use traits::{CpuBackend, CpuStepResult};
pub use x86_64::X86_64Cpu;

pub use mel_core::{
    Address, Architecture, CpuError, MelError, MelResult, RegisterId, RegisterValue,
};
