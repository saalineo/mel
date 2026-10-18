//! Device trait and handles for components attached to the system bus.

use mel_core::{Address, MelError};

/// Trait for any peripheral or memory block addressable on the system bus.
pub trait BusDevice: Send + Sync {
    /// Human-readable device name.
    fn name(&self) -> &str;

    /// Read an integer of `size` bytes (1, 2, 4, or 8) at `offset` from device base.
    fn read(&mut self, offset: Address, size: usize) -> Result<u64, MelError>;

    /// Write an integer of `size` bytes (1, 2, 4, or 8) at `offset` from device base.
    fn write(&mut self, offset: Address, value: u64, size: usize) -> Result<(), MelError>;
}
