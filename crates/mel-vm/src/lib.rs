//! # MEL Virtual Machine
//!
//! Virtual motherboard composing CPU, system bus, memory, and MMIO peripherals into a single machine.

pub use mel_core::{Architecture, MelError, MelResult};

pub struct VirtualMachine {
    pub arch: Architecture,
}

impl VirtualMachine {
    pub fn new(arch: Architecture) -> Self {
        Self { arch }
    }
}
