//! Dummy / loopback peripheral for testing bus and MMIO routing.

use crate::device_trait::MmioDevice;
use mel_core::MelError;
use serde::{Deserialize, Serialize};

/// A simple 4-register MMIO device for testing read, write, reset, and state serialization.
/// - 0x00: Status register (Read-only: returns 0x1)
/// - 0x04: Control register (Read/Write: bits)
/// - 0x08: Scratch register (Read/Write: 32-bit storage)
/// - 0x0C: Counter register (Read-only: increments on read)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DummyDevice {
    name: String,
    size: u64,
    control: u32,
    scratch: u32,
    counter: u32,
}

impl DummyDevice {
    pub fn new(name: impl Into<String>, size: u64) -> Self {
        Self {
            name: name.into(),
            size,
            control: 0,
            scratch: 0,
            counter: 0,
        }
    }
}

impl MmioDevice for DummyDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn read(&mut self, offset: u64, _size: usize) -> Result<u64, MelError> {
        match offset {
            0x00 => Ok(0x1),
            0x04 => Ok(u64::from(self.control)),
            0x08 => Ok(u64::from(self.scratch)),
            0x0C => {
                let val = self.counter;
                self.counter = self.counter.wrapping_add(1);
                Ok(u64::from(val))
            }
            _ => Err(MelError::Device {
                device: self.name.clone(),
                message: format!("Invalid read offset {:#x}", offset),
            }),
        }
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), MelError> {
        match offset {
            0x00 => Err(MelError::Device {
                device: self.name.clone(),
                message: "Write to read-only status register".to_string(),
            }),
            0x04 => {
                self.control = value as u32;
                Ok(())
            }
            0x08 => {
                self.scratch = value as u32;
                Ok(())
            }
            0x0C => Err(MelError::Device {
                device: self.name.clone(),
                message: "Write to read-only counter register".to_string(),
            }),
            _ => Err(MelError::Device {
                device: self.name.clone(),
                message: format!("Invalid write offset {:#x}", offset),
            }),
        }
    }

    fn reset(&mut self) -> Result<(), MelError> {
        self.control = 0;
        self.scratch = 0;
        self.counter = 0;
        Ok(())
    }

    fn tick(&mut self, _cycles: u64) -> Result<(), MelError> {
        self.counter = self.counter.wrapping_add(1);
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        serde_json::to_vec(self).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let restored: Self = serde_json::from_slice(data).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })?;
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy_device_operations() {
        let mut dev = DummyDevice::new("test_dummy", 0x100);
        assert_eq!(dev.name(), "test_dummy");
        assert_eq!(dev.size(), 0x100);

        assert_eq!(dev.read(0x00, 4).unwrap(), 0x1);
        assert!(dev.write(0x00, 0x10, 4).is_err());

        dev.write(0x08, 0xCAFEBABE, 4).unwrap();
        assert_eq!(dev.read(0x08, 4).unwrap(), 0xCAFEBABE);

        assert_eq!(dev.read(0x0C, 4).unwrap(), 0);
        assert_eq!(dev.read(0x0C, 4).unwrap(), 1);

        dev.reset().unwrap();
        assert_eq!(dev.read(0x08, 4).unwrap(), 0);
    }

    #[test]
    fn test_dummy_device_save_restore() {
        let mut dev = DummyDevice::new("test_dummy", 0x100);
        dev.write(0x04, 0x1234, 4).unwrap();
        dev.write(0x08, 0x5678, 4).unwrap();

        let state = dev.save_state().unwrap();

        let mut dev2 = DummyDevice::new("test_dummy", 0x100);
        dev2.load_state(&state).unwrap();

        assert_eq!(dev2.read(0x04, 4).unwrap(), 0x1234);
        assert_eq!(dev2.read(0x08, 4).unwrap(), 0x5678);
    }
}
