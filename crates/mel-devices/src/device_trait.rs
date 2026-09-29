//! Core MMIO device trait and lifecycle management.

use std::sync::{Arc, Mutex};
use mel_bus::BusDevice;
use mel_core::MelError;

/// Universal trait implemented by all memory-mapped peripherals in MEL.
pub trait MmioDevice: Send + Sync {
    /// Human-readable device name.
    fn name(&self) -> &str;

    /// Address space size consumed by the device in bytes.
    fn size(&self) -> u64;

    /// Read an integer of `size` bytes at `offset` from device base address.
    fn read(&mut self, offset: u64, size: usize) -> Result<u64, MelError>;

    /// Write an integer of `size` bytes at `offset` from device base address.
    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError>;

    /// Reset device state to architectural power-on state.
    fn reset(&mut self) -> Result<(), MelError>;

    /// Advance device state by `cycles` clock ticks.
    fn tick(&mut self, _cycles: u64) -> Result<(), MelError> {
        Ok(())
    }

    /// Serialize device internal state for save states.
    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        Ok(Vec::new())
    }

    /// Restore device internal state from save state snapshot.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), MelError> {
        Ok(())
    }
}

/// Adapter wrapping any `MmioDevice` into a `mel_bus::BusDevice`.
pub struct MmioBusAdapter<T: MmioDevice>(pub T);

impl<T: MmioDevice> BusDevice for MmioBusAdapter<T> {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, MelError> {
        self.0.read(offset, size)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError> {
        self.0.write(offset, value, size)
    }
}

/// Dynamic shared adapter for `Arc<Mutex<dyn MmioDevice>>`.
pub struct SharedMmioBusAdapter(pub Arc<Mutex<dyn MmioDevice>>);

impl BusDevice for SharedMmioBusAdapter {
    fn name(&self) -> &str {
        "mmio_device"
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, MelError> {
        self.0.lock().expect("MMIO device lock poisoned").read(offset, size)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError> {
        self.0.lock().expect("MMIO device lock poisoned").write(offset, value, size)
    }
}
