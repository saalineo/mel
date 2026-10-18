//! # MEL Core
//!
//! Core types, errors, architecture enums, and shared definitions for the MEL simulator.

pub mod arch;
pub mod error;
pub mod memory_maps;
pub mod profile;
pub mod types;

pub use arch::Architecture;
pub use error::{BusError, CpuError, MelError, MelResult, MemoryError};
pub use memory_maps::{ArchitectureMemoryMap, MemoryMapEntry};
pub use profile::{
    ApicDeviceConfig, BlockDeviceConfig, BootConfig, BootRomConfig, ClintDeviceConfig,
    DevicesConfig, GicDeviceConfig, MachineProfile, PlicDeviceConfig, UartDeviceConfig,
};
pub use types::{
    AccessType, Address, ByteCount, Endianness, Permissions, PhysicalAddress, RegisterId,
    RegisterValue, Size, VirtualAddress,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_names() {
        assert_eq!(Architecture::Riscv64.name(), "riscv64");
        assert_eq!(Architecture::Arm64.name(), "arm64");
        assert_eq!(Architecture::X86_64.name(), "x86_64");
        assert_eq!(Architecture::PowerIso64.name(), "powerpc64");
        assert_eq!(Architecture::Riscv64.pointer_width(), 64);
    }

    #[test]
    fn test_error_conversions() {
        let mem_err = MemoryError::OutOfBounds {
            addr: 0x8000_0000,
            size: 64,
        };
        let mel_err: MelError = mem_err.into();
        assert!(mel_err.to_string().contains("0x80000000"));

        let bus_err = BusError::UnmappedAddress {
            addr: 0x1000,
            size: 4,
        };
        let mel_err_bus: MelError = bus_err.into();
        assert!(mel_err_bus.to_string().contains("0x1000"));
    }
}
