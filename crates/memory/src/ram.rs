//! Physical RAM buffer implementation.

use mel_core::{Endianness, MemoryError};
use serde::{Deserialize, Serialize};

/// Represents contiguous physical memory (RAM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ram {
    data: Vec<u8>,
}

impl Ram {
    /// Allocate a new zeroed RAM buffer of given size in bytes.
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }

    /// Returns the capacity of the RAM buffer in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Read a single byte at the given offset.
    #[inline]
    pub fn read_u8(&self, offset: usize) -> Result<u8, MemoryError> {
        self.data
            .get(offset)
            .copied()
            .ok_or(MemoryError::OutOfBounds {
                addr: offset as u64,
                size: self.data.len(),
            })
    }

    /// Write a single byte at the given offset.
    #[inline]
    pub fn write_u8(&mut self, offset: usize, val: u8) -> Result<(), MemoryError> {
        let len = self.data.len();
        let byte = self.data.get_mut(offset).ok_or(MemoryError::OutOfBounds {
            addr: offset as u64,
            size: len,
        })?;
        *byte = val;
        Ok(())
    }

    /// Read a 16-bit value at the given offset.
    pub fn read_u16(&self, offset: usize, endianness: Endianness) -> Result<u16, MemoryError> {
        let slice = self.read_bytes(offset, 2)?;
        endianness.read_u16(slice).ok_or(MemoryError::OutOfBounds {
            addr: offset as u64,
            size: self.data.len(),
        })
    }

    /// Write a 16-bit value at the given offset.
    pub fn write_u16(
        &mut self,
        offset: usize,
        val: u16,
        endianness: Endianness,
    ) -> Result<(), MemoryError> {
        let slice = self.get_slice_mut(offset, 2)?;
        endianness.write_u16(val, slice);
        Ok(())
    }

    /// Read a 32-bit value at the given offset.
    pub fn read_u32(&self, offset: usize, endianness: Endianness) -> Result<u32, MemoryError> {
        let slice = self.read_bytes(offset, 4)?;
        endianness.read_u32(slice).ok_or(MemoryError::OutOfBounds {
            addr: offset as u64,
            size: self.data.len(),
        })
    }

    /// Write a 32-bit value at the given offset.
    pub fn write_u32(
        &mut self,
        offset: usize,
        val: u32,
        endianness: Endianness,
    ) -> Result<(), MemoryError> {
        let slice = self.get_slice_mut(offset, 4)?;
        endianness.write_u32(val, slice);
        Ok(())
    }

    /// Read a 64-bit value at the given offset.
    pub fn read_u64(&self, offset: usize, endianness: Endianness) -> Result<u64, MemoryError> {
        let slice = self.read_bytes(offset, 8)?;
        endianness.read_u64(slice).ok_or(MemoryError::OutOfBounds {
            addr: offset as u64,
            size: self.data.len(),
        })
    }

    /// Write a 64-bit value at the given offset.
    pub fn write_u64(
        &mut self,
        offset: usize,
        val: u64,
        endianness: Endianness,
    ) -> Result<(), MemoryError> {
        let slice = self.get_slice_mut(offset, 8)?;
        endianness.write_u64(val, slice);
        Ok(())
    }

    /// Read a sub-slice of bytes.
    pub fn read_bytes(&self, offset: usize, len: usize) -> Result<&[u8], MemoryError> {
        let total_size = self.data.len();
        let end = offset.checked_add(len).filter(|&e| e <= total_size).ok_or(
            MemoryError::OutOfBounds {
                addr: offset as u64,
                size: total_size,
            },
        )?;
        Ok(&self.data[offset..end])
    }

    /// Write raw bytes into memory.
    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), MemoryError> {
        let slice = self.get_slice_mut(offset, bytes.len())?;
        slice.copy_from_slice(bytes);
        Ok(())
    }

    fn get_slice_mut(&mut self, offset: usize, len: usize) -> Result<&mut [u8], MemoryError> {
        let total_size = self.data.len();
        let end = offset.checked_add(len).filter(|&e| e <= total_size).ok_or(
            MemoryError::OutOfBounds {
                addr: offset as u64,
                size: total_size,
            },
        )?;
        Ok(&mut self.data[offset..end])
    }

    /// Reset all bytes to zero.
    pub fn zero(&mut self) {
        self.data.fill(0);
    }

    /// Fill memory with a specific pattern.
    pub fn fill(&mut self, val: u8) {
        self.data.fill(val);
    }

    /// Direct immutable slice access.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Direct mutable slice access.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl Default for Ram {
    fn default() -> Self {
        Self::new(0)
    }
}

impl AsRef<[u8]> for Ram {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl AsMut<[u8]> for Ram {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ram_read_write_primitives() {
        let mut ram = Ram::new(64);
        assert_eq!(ram.size(), 64);

        ram.write_u8(0, 0x55).unwrap();
        assert_eq!(ram.read_u8(0).unwrap(), 0x55);

        ram.write_u16(2, 0x1234, Endianness::Little).unwrap();
        assert_eq!(ram.read_u16(2, Endianness::Little).unwrap(), 0x1234);

        ram.write_u32(4, 0xDEADBEEF, Endianness::Little).unwrap();
        assert_eq!(ram.read_u32(4, Endianness::Little).unwrap(), 0xDEADBEEF);

        ram.write_u64(8, 0x0102030405060708, Endianness::Big)
            .unwrap();
        assert_eq!(
            ram.read_u64(8, Endianness::Big).unwrap(),
            0x0102030405060708
        );
        assert_eq!(ram.read_u8(8).unwrap(), 0x01);
        assert_eq!(ram.read_u8(15).unwrap(), 0x08);
    }

    #[test]
    fn test_ram_bounds_check() {
        let mut ram = Ram::new(16);
        assert!(ram.read_u8(16).is_err());
        assert!(ram.write_u8(16, 0).is_err());
        assert!(ram.read_u32(13, Endianness::Little).is_err());
        assert!(ram.write_u64(9, 0, Endianness::Little).is_err());
        assert!(ram.read_bytes(10, 10).is_err());
    }

    #[test]
    fn test_ram_zero_and_fill() {
        let mut ram = Ram::new(32);
        ram.fill(0xFF);
        assert_eq!(ram.read_u8(0).unwrap(), 0xFF);
        assert_eq!(ram.read_u8(31).unwrap(), 0xFF);
        ram.zero();
        assert_eq!(ram.read_u8(0).unwrap(), 0x00);
        assert_eq!(ram.read_u8(31).unwrap(), 0x00);
    }
}
