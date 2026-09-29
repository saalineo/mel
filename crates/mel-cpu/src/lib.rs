//! # MEL CPU
//!
//! Universal CPU backend abstractions, execution step types, and emulator engine bridges.

pub mod mock;
pub mod traits;

pub use mock::MockCpu;
pub use traits::{CpuBackend, CpuStepResult};

pub use mel_core::{
    Address, Architecture, CpuError, MelError, MelResult, RegisterId, RegisterValue,
};
