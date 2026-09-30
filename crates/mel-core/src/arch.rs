//! Supported CPU architecture definitions and defaults for MEL.

use crate::error::MelError;
use crate::types::{Address, Endianness};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The four officially supported CPU architectures in MEL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    /// RISC-V 64-bit (RV64GC)
    Riscv64,
    /// ARM 64-bit (AArch64 / ARMv8-A)
    Arm64,
    /// x86 64-bit (AMD64 / x86-64)
    X86_64,
    /// Power ISA 64-bit (PowerPC 64)
    PowerIso64,
}

impl Architecture {
    /// Returns pointer width in bits (all target architectures are 64-bit).
    pub const fn pointer_width(&self) -> usize {
        64
    }

    /// Returns standard string identifier.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Riscv64 => "riscv64",
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
            Self::PowerIso64 => "powerpc64",
        }
    }

    /// Returns native default endianness.
    pub const fn default_endianness(&self) -> Endianness {
        match self {
            Self::Riscv64 | Self::Arm64 | Self::X86_64 => Endianness::Little,
            Self::PowerIso64 => Endianness::Big,
        }
    }

    /// Returns default physical RAM base address.
    pub const fn default_ram_base(&self) -> Address {
        match self {
            Self::Riscv64 => 0x8000_0000,
            Self::Arm64 => 0x4000_0000,
            Self::X86_64 => 0x0010_0000,
            Self::PowerIso64 => 0x0000_0000,
        }
    }

    /// Returns default hardware reset vector address.
    pub const fn default_reset_vector(&self) -> Address {
        match self {
            Self::Riscv64 => 0x8000_0000,
            Self::Arm64 => 0x4000_0000,
            Self::X86_64 => 0x000F_FFF0,
            Self::PowerIso64 => 0x0000_0100,
        }
    }

    /// List of all 4 architectures.
    pub const ALL: [Self; 4] = [Self::Riscv64, Self::Arm64, Self::X86_64, Self::PowerIso64];
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl FromStr for Architecture {
    type Err = MelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "riscv64" | "riscv" | "rv64" | "riscv-64" => Ok(Self::Riscv64),
            "arm64" | "aarch64" | "arm" => Ok(Self::Arm64),
            "x86_64" | "x86-64" | "x64" | "amd64" => Ok(Self::X86_64),
            "powerpc64" | "powerpc" | "ppc64" | "powerisa" | "power" => Ok(Self::PowerIso64),
            other => Err(MelError::Config(format!("Unknown architecture: '{other}'"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_from_str() {
        assert_eq!(
            "riscv64".parse::<Architecture>().unwrap(),
            Architecture::Riscv64
        );
        assert_eq!(
            "aarch64".parse::<Architecture>().unwrap(),
            Architecture::Arm64
        );
        assert_eq!(
            "x86_64".parse::<Architecture>().unwrap(),
            Architecture::X86_64
        );
        assert_eq!(
            "ppc64".parse::<Architecture>().unwrap(),
            Architecture::PowerIso64
        );
        assert!("mips".parse::<Architecture>().is_err());
    }

    #[test]
    fn test_architecture_defaults() {
        assert_eq!(
            Architecture::Riscv64.default_endianness(),
            Endianness::Little
        );
        assert_eq!(
            Architecture::PowerIso64.default_endianness(),
            Endianness::Big
        );
        assert_eq!(Architecture::Riscv64.default_ram_base(), 0x8000_0000);
        assert_eq!(Architecture::Arm64.default_ram_base(), 0x4000_0000);
    }
}
