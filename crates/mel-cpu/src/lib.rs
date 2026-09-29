//! # MEL CPU
//!
//! CPU backend abstractions and emulation engine bridges.

pub use mel_core::{Architecture, CpuError, MelError, MelResult};

pub trait CpuBackend: Send + Sync {
    fn arch(&self) -> Architecture;
    fn reset(&mut self) -> Result<(), MelError>;
    fn pc(&self) -> Result<u64, MelError>;
}
