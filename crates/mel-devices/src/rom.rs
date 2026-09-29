//! Read-Only Memory (Boot ROM) MMIO device.

use mel_core::{Address, Endianness, MelError, MemoryError};
use crate::device_trait::MmioDevice;
use serde::{Deserialize, Serialize};

/// Read-Only Memory peripheral storing immutable boot instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootRom {
    name: String,
    data: Vec<u8>,
}

impl BootRom {
    /// Create a Boot ROM populated with the provided binary contents, padded to at least `size` bytes.
    pub fn new(name: impl Into<String>, code: &[u8], size: usize) -> Self {
        let actual_size = size.max(code.len());
        let mut data = vec![0u8; actual_size];
        data[..code.len()].copy_from_slice(code);
        Self {
            name: name.into(),
            data,
        }
    }

    /// Create a standard 4 KB Boot ROM containing raw payload bytes.
    pub fn from_bytes(name: impl Into<String>, code: &[u8]) -> Self {
        Self::new(name, code, 0x1000)
    }

    /// Construct a standard RISC-V 64-bit reset jump payload targeting `target_addr`.
    pub fn riscv64_jump_to(name: impl Into<String>, target_addr: Address) -> Self {
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&0x0000_0297u32.to_le_bytes()); // auipc t0, 0
        payload.extend_from_slice(&0x0102_b283u32.to_le_bytes()); // ld t0, 16(t0)
        payload.extend_from_slice(&0x0002_8067u32.to_le_bytes()); // jr t0
        payload.extend_from_slice(&0x0000_0013u32.to_le_bytes()); // nop
        payload.extend_from_slice(&target_addr.to_le_bytes());

        Self::from_bytes(name, &payload)
    }

    /// Borrow the underlying raw byte buffer.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

impl MmioDevice for BootRom {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> u64 {
        self.data.len() as u64
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, MelError> {
        let offset = offset as usize;
        let slice = self
            .data
            .get(offset..)
            .and_then(|tail| tail.get(..size))
            .ok_or(MemoryError::OutOfBounds {
                addr: offset as u64,
                size: self.data.len(),
            })?;

        match size {
            1 => Ok(u64::from(slice[0])),
            2 => Endianness::Little
                .read_u16(slice)
                .map(u64::from)
                .ok_or_else(|| MemoryError::OutOfBounds {
                    addr: offset as u64,
                    size: self.data.len(),
                }.into()),
            4 => Endianness::Little
                .read_u32(slice)
                .map(u64::from)
                .ok_or_else(|| MemoryError::OutOfBounds {
                    addr: offset as u64,
                    size: self.data.len(),
                }.into()),
            8 => Endianness::Little
                .read_u64(slice)
                .ok_or_else(|| MemoryError::OutOfBounds {
                    addr: offset as u64,
                    size: self.data.len(),
                }.into()),
            _ => Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset as u64,
                size: self.data.len(),
            })),
        }
    }

    fn write(&mut self, offset: u64, _value: u64, _size: usize) -> Result<(), MelError> {
        Err(MelError::Memory(MemoryError::WriteToReadOnly { addr: offset }))
    }

    fn reset(&mut self) -> Result<(), MelError> {
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
    fn test_boot_rom_reading() {
        let code = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
        let mut rom = BootRom::new("test_rom", &code, 0x100);

        assert_eq!(rom.name(), "test_rom");
        assert_eq!(rom.size(), 0x100);

        assert_eq!(rom.read(0, 1).unwrap(), 0x11);
        assert_eq!(rom.read(1, 1).unwrap(), 0x22);
        assert_eq!(rom.read(0, 2).unwrap(), 0x2211);
        assert_eq!(rom.read(0, 4).unwrap(), 0x44332211);
        assert_eq!(rom.read(0, 8).unwrap(), 0x8877665544332211);

        assert!(rom.read(0x100, 1).is_err());
        assert!(rom.read(0xFD, 4).is_err());
    }

    #[test]
    fn test_boot_rom_write_rejection() {
        let mut rom = BootRom::from_bytes("test_rom", &[0x90, 0x90]);
        let err = rom.write(0, 0x42, 1);
        assert!(matches!(err, Err(MelError::Memory(MemoryError::WriteToReadOnly { addr: 0 }))));
    }

    #[test]
    fn test_boot_rom_riscv64_jump() {
        let mut rom = BootRom::riscv64_jump_to("rom", 0x8000_0000);
        assert_eq!(rom.size(), 0x1000);

        assert_eq!(rom.read(0, 4).unwrap(), 0x0000_0297);
        assert_eq!(rom.read(4, 4).unwrap(), 0x0102_b283);
        assert_eq!(rom.read(8, 4).unwrap(), 0x0002_8067);
        assert_eq!(rom.read(12, 4).unwrap(), 0x0000_0013);
        assert_eq!(rom.read(16, 8).unwrap(), 0x8000_0000);
    }

    #[test]
    fn test_boot_rom_save_restore() {
        let code = [1, 2, 3, 4];
        let rom = BootRom::from_bytes("rom", &code);
        let snapshot = rom.save_state().unwrap();

        let mut restored = BootRom::from_bytes("rom", &[]);
        restored.load_state(&snapshot).unwrap();
        assert_eq!(restored.read(0, 4).unwrap(), 0x04030201);
    }
}
