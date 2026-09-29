//! # MEL Virtual Machine
//!
//! Virtual motherboard composing CPU, system bus, memory, and MMIO peripherals into a single machine.

pub mod machine;

pub use machine::{create_basic_riscv64_vm, create_mock_vm, VirtualMachine};
pub use mel_core::{Architecture, MelError, MelResult};
