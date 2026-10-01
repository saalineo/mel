//! # MEL CPU
//!
//! Universal CPU backend abstractions, execution step types, and emulator engine bridges.

pub mod hook;
pub mod mock;
pub mod riscv64;
pub mod traits;

pub use hook::{register_bus_hooks, CpuBusBridge};
pub use mock::MockCpu;
pub use riscv64::{
    Riscv64Cpu, MCAUSE_INTERRUPT_BIT, MCAUSE_MEI, MCAUSE_MSI, MCAUSE_MTI, MIE_MEIE, MIE_MSIE,
    MIE_MTIE, MIP_MEIP, MIP_MSIP, MIP_MTIP, MSTATUS_MIE, MSTATUS_MPIE, MSTATUS_MPP_M,
    MSTATUS_MPP_MASK,
};
pub use traits::{CpuBackend, CpuStepResult};

pub use mel_core::{
    Address, Architecture, CpuError, MelError, MelResult, RegisterId, RegisterValue,
};

