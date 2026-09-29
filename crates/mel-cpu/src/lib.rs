//! # MEL CPU
//!
//! Universal CPU backend abstractions, execution step types, and emulator engine bridges.

pub mod mock;
pub mod riscv64;
pub mod traits;

pub use mock::MockCpu;
pub use riscv64::Riscv64Cpu;
pub use traits::{CpuBackend, CpuStepResult};

pub use mel_core::{
    Address, Architecture, CpuError, MelError, MelResult, RegisterId, RegisterValue,
};
