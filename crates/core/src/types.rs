//! Core types and byte/endianness helpers for the MEL simulation ecosystem.

use serde::{Deserialize, Serialize};

/// 64-bit physical address.
pub type Address = u64;

/// 64-bit physical address alias.
pub type PhysicalAddress = u64;

/// 64-bit virtual address.
pub type VirtualAddress = u64;

/// Size in bytes (64-bit).
pub type Size = u64;

/// Byte count / size in bytes (usize).
pub type ByteCount = usize;

/// Register value (64-bit width).
pub type RegisterValue = u64;

/// Memory access type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessType {
    Read,
    Write,
    Execute,
}

/// Memory access permissions bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    pub const READ: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const READ_EXECUTE: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const ALL: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
}

/// Endianness of memory and operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endianness {
    Little,
    Big,
}

impl Endianness {
    /// Read a 16-bit unsigned integer from a byte slice with this endianness.
    #[inline]
    pub fn read_u16(&self, bytes: &[u8]) -> Option<u16> {
        let arr: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(arr),
            Self::Big => u16::from_be_bytes(arr),
        })
    }

    /// Read a 32-bit unsigned integer from a byte slice with this endianness.
    #[inline]
    pub fn read_u32(&self, bytes: &[u8]) -> Option<u32> {
        let arr: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        Some(match self {
            Self::Little => u32::from_le_bytes(arr),
            Self::Big => u32::from_be_bytes(arr),
        })
    }

    /// Read a 64-bit unsigned integer from a byte slice with this endianness.
    #[inline]
    pub fn read_u64(&self, bytes: &[u8]) -> Option<u64> {
        let arr: [u8; 8] = bytes.get(..8)?.try_into().ok()?;
        Some(match self {
            Self::Little => u64::from_le_bytes(arr),
            Self::Big => u64::from_be_bytes(arr),
        })
    }

    /// Write a 16-bit unsigned integer into a byte slice with this endianness.
    #[inline]
    pub fn write_u16(&self, val: u16, bytes: &mut [u8]) -> bool {
        if let Some(target) = bytes.get_mut(..2) {
            let arr = match self {
                Self::Little => val.to_le_bytes(),
                Self::Big => val.to_be_bytes(),
            };
            target.copy_from_slice(&arr);
            true
        } else {
            false
        }
    }

    /// Write a 32-bit unsigned integer into a byte slice with this endianness.
    #[inline]
    pub fn write_u32(&self, val: u32, bytes: &mut [u8]) -> bool {
        if let Some(target) = bytes.get_mut(..4) {
            let arr = match self {
                Self::Little => val.to_le_bytes(),
                Self::Big => val.to_be_bytes(),
            };
            target.copy_from_slice(&arr);
            true
        } else {
            false
        }
    }

    /// Write a 64-bit unsigned integer into a byte slice with this endianness.
    #[inline]
    pub fn write_u64(&self, val: u64, bytes: &mut [u8]) -> bool {
        if let Some(target) = bytes.get_mut(..8) {
            let arr = match self {
                Self::Little => val.to_le_bytes(),
                Self::Big => val.to_be_bytes(),
            };
            target.copy_from_slice(&arr);
            true
        } else {
            false
        }
    }
}

/// Generic register identifier across architectures.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegisterId {
    pub name: String,
    pub index: Option<u32>,
}

impl RegisterId {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            index: None,
        }
    }

    pub fn indexed(name: impl Into<String>, index: u32) -> Self {
        Self {
            name: name.into(),
            index: Some(index),
        }
    }
}

impl std::fmt::Display for RegisterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(idx) = self.index {
            write!(f, "{}[{idx}]", self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl From<&str> for RegisterId {
    fn from(s: &str) -> Self {
        Self::named(s)
    }
}

impl From<String> for RegisterId {
    fn from(s: String) -> Self {
        Self::named(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endianness_helpers() {
        let mut buf = [0u8; 8];
        Endianness::Little.write_u32(0x12345678, &mut buf);
        assert_eq!(&buf[..4], &[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(Endianness::Little.read_u32(&buf).unwrap(), 0x12345678);

        Endianness::Big.write_u32(0x12345678, &mut buf);
        assert_eq!(&buf[..4], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(Endianness::Big.read_u32(&buf).unwrap(), 0x12345678);

        Endianness::Little.write_u64(0x0123456789ABCDEF, &mut buf);
        assert_eq!(
            Endianness::Little.read_u64(&buf).unwrap(),
            0x0123456789ABCDEF
        );
    }
}
