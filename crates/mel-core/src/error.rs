//! Error types for the MEL simulation ecosystem.

use crate::types::Address;
use thiserror::Error;

/// Main error type for MEL operations.
#[derive(Debug, Error)]
pub enum MelError {
    #[error("CPU error: {0}")]
    Cpu(#[from] CpuError),

    #[error("Bus error: {0}")]
    Bus(#[from] BusError),

    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Device error [{device}]: {message}")]
    Device { device: String, message: String },

    #[error("Loader error: {0}")]
    Loader(String),

    #[error("Debugger error: {0}")]
    Debugger(String),

    #[error("Scenario error: {0}")]
    Scenario(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors related to CPU simulation.
#[derive(Debug, Error)]
pub enum CpuError {
    #[error("Backend initialization failed: {0}")]
    InitFailed(String),

    #[error("Execution trapped: cause {cause:#x}, epc {epc:#x}")]
    Trap { cause: u64, epc: u64 },

    #[error("Invalid instruction at {pc:#x}: opcode {opcode:#x}")]
    IllegalInstruction { pc: Address, opcode: u64 },

    #[error("Invalid register identifier: {0}")]
    InvalidRegister(String),

    #[error("CPU execution halted: {0}")]
    Halted(String),

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),

    #[error("Internal emulation engine failure: {0}")]
    Internal(String),
}

/// Errors related to the system bus.
#[derive(Debug, Error)]
pub enum BusError {
    #[error("Access to unmapped address {addr:#x} (size {size} bytes)")]
    UnmappedAddress { addr: Address, size: usize },

    #[error("Overlapping address range: {start:#x}..{end:#x}")]
    OverlappingRegion { start: Address, end: Address },

    #[error("Misaligned bus access at {addr:#x} for alignment {align}")]
    MisalignedAccess { addr: Address, align: usize },
}

/// Errors related to physical memory.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Memory access out of bounds at {addr:#x} (region size {size:#x})")]
    OutOfBounds { addr: Address, size: usize },

    #[error("Attempted write to read-only memory region at {addr:#x}")]
    WriteToReadOnly { addr: Address },

    #[error("Invalid memory allocation size: {0} bytes")]
    InvalidAllocation(usize),
}

/// Result alias for MEL operations.
pub type MelResult<T> = Result<T, MelError>;
