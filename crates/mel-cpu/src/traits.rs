//! Universal CPU backend trait and execution step types.

use mel_core::{Architecture, MelError, RegisterId};
use serde::{Deserialize, Serialize};

/// Result of stepping or executing instructions on a CPU backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuStepResult {
    /// Normal execution completed.
    Ok { cycles: u64 },
    /// Hit a registered breakpoint.
    Breakpoint { pc: u64 },
    /// Execution halted.
    Halted { reason: String },
    /// CPU triggered an architectural trap / exception.
    Trap { cause: u64, epc: u64 },
}

/// Universal trait implemented by all CPU emulation backends.
pub trait CpuBackend: Send + Sync {
    /// Target CPU architecture.
    fn arch(&self) -> Architecture;

    /// Reset CPU to initial architectural reset state.
    fn reset(&mut self) -> Result<(), MelError>;

    /// Execute a single instruction.
    fn step(&mut self) -> Result<CpuStepResult, MelError>;

    /// Execute instructions up to `max_cycles` or until a halt/breakpoint/trap occurs.
    fn run(&mut self, max_cycles: u64) -> Result<CpuStepResult, MelError>;

    /// Read architectural register value.
    fn read_register(&self, reg: RegisterId) -> Result<u64, MelError>;

    /// Write architectural register value.
    fn write_register(&mut self, reg: RegisterId, val: u64) -> Result<(), MelError>;

    /// Current program counter (PC).
    fn pc(&self) -> Result<u64, MelError>;

    /// Set program counter (PC).
    fn set_pc(&mut self, pc: u64) -> Result<(), MelError>;

    /// Serialize internal CPU state for snapshots.
    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        Ok(Vec::new())
    }

    /// Restore internal CPU state from snapshot.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), MelError> {
        Ok(())
    }

    /// Write raw memory to CPU address space if supported by backend.
    fn mem_write(&mut self, _addr: u64, _bytes: &[u8]) -> Result<(), MelError> {
        Ok(())
    }

    /// Read raw memory from CPU address space if supported by backend.
    fn mem_read(&self, _addr: u64, _bytes: &mut [u8]) -> Result<(), MelError> {
        Ok(())
    }
}

