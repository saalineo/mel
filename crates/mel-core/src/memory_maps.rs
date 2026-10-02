//! Standard architecture memory maps, device layouts, and reset vectors.

use crate::arch::Architecture;
use crate::types::{Address, Permissions, Size};
use serde::{Deserialize, Serialize};

/// Standard memory map entry representing a physical memory or MMIO region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMapEntry {
    pub name: String,
    pub base: Address,
    pub size: Size,
    pub permissions: Permissions,
    pub description: String,
}

impl MemoryMapEntry {
    pub fn new(
        name: impl Into<String>,
        base: Address,
        size: Size,
        permissions: Permissions,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base,
            size,
            permissions,
            description: description.into(),
        }
    }

    /// Check if the given address falls within this memory entry.
    #[inline]
    pub fn contains(&self, addr: Address) -> bool {
        addr >= self.base && addr < self.base.saturating_add(self.size)
    }

    /// End address (exclusive).
    #[inline]
    pub fn end(&self) -> Address {
        self.base.saturating_add(self.size)
    }
}

/// Standardized memory map template and default hardware configuration for a CPU architecture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureMemoryMap {
    pub arch: Architecture,
    pub ram_base: Address,
    pub default_ram_size: Size,
    pub default_reset_vector: Address,
    pub alt_reset_vector: Option<Address>,
    pub uart_mmio_base: Option<Address>,
    pub uart_port_io_base: Option<u16>,
    pub block_device_base: Option<Address>,
    pub interrupt_controller_base: Option<Address>,
    pub boot_rom_base: Option<Address>,
    pub entries: Vec<MemoryMapEntry>,
}

impl ArchitectureMemoryMap {
    /// Return the standard memory map for the given architecture.
    pub fn for_architecture(arch: Architecture) -> Self {
        match arch {
            Architecture::Riscv64 => Self::riscv64(),
            Architecture::Arm64 => Self::arm64(),
            Architecture::X86_64 => Self::x86_64(),
            Architecture::PowerIso64 => Self::power_iso64(),
        }
    }

    /// Standard RISC-V RV64GC memory map layout.
    pub fn riscv64() -> Self {
        let entries = vec![
            MemoryMapEntry::new(
                "Boot ROM",
                0x0000_1000,
                0x1000, // 4 KB
                Permissions::READ_EXECUTE,
                "Hardware reset vector & initial jump ROM",
            ),
            MemoryMapEntry::new(
                "CLINT",
                0x0200_0000,
                0x0001_0000, // 64 KB
                Permissions::READ_WRITE,
                "Core Local Interruptor (mtime, mtimecmp, msip)",
            ),
            MemoryMapEntry::new(
                "PLIC",
                0x0C00_0000,
                0x0400_0000, // 64 MB
                Permissions::READ_WRITE,
                "Platform-Level Interrupt Controller",
            ),
            MemoryMapEntry::new(
                "UART 16550",
                0x1000_0000,
                0x0100, // 256 B
                Permissions::READ_WRITE,
                "NS16550 serial console MMIO",
            ),
            MemoryMapEntry::new(
                "Block Storage",
                0x2000_0000,
                0x1000, // 4 KB
                Permissions::READ_WRITE,
                "Virtual block storage disk controller",
            ),
            MemoryMapEntry::new(
                "Main RAM",
                0x8000_0000,
                0x0400_0000, // 64 MB
                Permissions::ALL,
                "System Physical RAM",
            ),
        ];

        Self {
            arch: Architecture::Riscv64,
            ram_base: 0x8000_0000,
            default_ram_size: 0x0400_0000,
            default_reset_vector: 0x8000_0000,
            alt_reset_vector: Some(0x0000_1000),
            uart_mmio_base: Some(0x1000_0000),
            uart_port_io_base: None,
            block_device_base: Some(0x2000_0000),
            interrupt_controller_base: Some(0x0C00_0000), // PLIC
            boot_rom_base: Some(0x0000_1000),
            entries,
        }
    }

    /// Standard ARM64 (AArch64) memory map layout.
    pub fn arm64() -> Self {
        let entries = vec![
            MemoryMapEntry::new(
                "Boot ROM",
                0x0000_0000,
                0x0001_0000, // 64 KB
                Permissions::READ_EXECUTE,
                "Boot ROM reset vector",
            ),
            MemoryMapEntry::new(
                "GIC",
                0x0800_0000,
                0x0001_0000, // 64 KB
                Permissions::READ_WRITE,
                "Generic Interrupt Controller (Distributor & CPU interface)",
            ),
            MemoryMapEntry::new(
                "UART 16550",
                0x0900_0000,
                0x0100, // 256 B
                Permissions::READ_WRITE,
                "Serial console MMIO",
            ),
            MemoryMapEntry::new(
                "Block Storage",
                0x2000_0000,
                0x1000, // 4 KB
                Permissions::READ_WRITE,
                "Virtual block storage disk controller",
            ),
            MemoryMapEntry::new(
                "Main RAM",
                0x4000_0000,
                0x0400_0000, // 64 MB
                Permissions::ALL,
                "System Physical RAM",
            ),
        ];

        Self {
            arch: Architecture::Arm64,
            ram_base: 0x4000_0000,
            default_ram_size: 0x0400_0000,
            default_reset_vector: 0x4000_0000,
            alt_reset_vector: Some(0x0000_0000),
            uart_mmio_base: Some(0x0900_0000),
            uart_port_io_base: None,
            block_device_base: Some(0x2000_0000),
            interrupt_controller_base: Some(0x0800_0000), // GIC
            boot_rom_base: Some(0x0000_0000),
            entries,
        }
    }

    /// Standard x86-64 memory map layout.
    pub fn x86_64() -> Self {
        let entries = vec![
            MemoryMapEntry::new(
                "BIOS / Boot ROM",
                0x000F_0000,
                0x0001_0000, // 64 KB
                Permissions::READ_EXECUTE,
                "System BIOS ROM (Reset Vector 0x000F_FFF0)",
            ),
            MemoryMapEntry::new(
                "Main RAM",
                0x0010_0000,
                0x0400_0000, // 64 MB (1MB base to 65MB)
                Permissions::ALL,
                "System Physical RAM (Extended Memory)",
            ),
            MemoryMapEntry::new(
                "UART MMIO / COM1 Mirror",
                0x1000_0000,
                0x0100, // 256 B
                Permissions::READ_WRITE,
                "UART 16550 MMIO mirror (also Port 0x3F8)",
            ),
            MemoryMapEntry::new(
                "Block Storage",
                0x2000_0000,
                0x1000, // 4 KB
                Permissions::READ_WRITE,
                "Virtual block storage disk controller",
            ),
            MemoryMapEntry::new(
                "Local APIC",
                0xFEE0_0000,
                0x1000, // 4 KB
                Permissions::READ_WRITE,
                "Advanced Programmable Interrupt Controller",
            ),
        ];

        Self {
            arch: Architecture::X86_64,
            ram_base: 0x0010_0000,
            default_ram_size: 0x0400_0000,
            default_reset_vector: 0x0010_0000,
            alt_reset_vector: Some(0x000F_FFF0),
            uart_mmio_base: Some(0x1000_0000),
            uart_port_io_base: Some(0x03F8),
            block_device_base: Some(0x2000_0000),
            interrupt_controller_base: Some(0xFEE0_0000), // Local APIC
            boot_rom_base: Some(0x000F_0000),
            entries,
        }
    }

    /// Standard Power ISA (PPC64) memory map layout.
    pub fn power_iso64() -> Self {
        let entries = vec![
            MemoryMapEntry::new(
                "Main RAM",
                0x0000_0000,
                0x0400_0000, // 64 MB
                Permissions::ALL,
                "System Physical RAM (Interrupt vectors at 0x0000_0100)",
            ),
            MemoryMapEntry::new(
                "UART 16550",
                0x1000_0000,
                0x0100, // 256 B
                Permissions::READ_WRITE,
                "Serial console MMIO",
            ),
            MemoryMapEntry::new(
                "Block Storage",
                0x2000_0000,
                0x1000, // 4 KB
                Permissions::READ_WRITE,
                "Virtual block storage disk controller",
            ),
            MemoryMapEntry::new(
                "OpenPIC / XIVE",
                0x3000_0000,
                0x0001_0000, // 64 KB
                Permissions::READ_WRITE,
                "Power ISA Interrupt Controller",
            ),
        ];

        Self {
            arch: Architecture::PowerIso64,
            ram_base: 0x0000_0000,
            default_ram_size: 0x0400_0000,
            default_reset_vector: 0x0000_0100,
            alt_reset_vector: Some(0x8000_0000),
            uart_mmio_base: Some(0x1000_0000),
            uart_port_io_base: None,
            block_device_base: Some(0x2000_0000),
            interrupt_controller_base: Some(0x3000_0000), // OpenPIC / XIVE
            boot_rom_base: None,
            entries,
        }
    }

    /// Find an entry whose address range contains `addr`.
    pub fn find_entry_at(&self, addr: Address) -> Option<&MemoryMapEntry> {
        self.entries.iter().find(|entry| entry.contains(addr))
    }

    /// Find an entry by case-insensitive name prefix or match.
    pub fn find_entry_by_name(&self, name: &str) -> Option<&MemoryMapEntry> {
        let name_lower = name.to_lowercase();
        self.entries
            .iter()
            .find(|entry| entry.name.to_lowercase().contains(&name_lower))
    }

    /// Check if the specified address range falls entirely within physical RAM.
    pub fn is_valid_ram_address(&self, addr: Address, size: usize) -> bool {
        let ram_end = self.ram_base.saturating_add(self.default_ram_size);
        let access_end = addr.saturating_add(size as u64);
        addr >= self.ram_base && access_end <= ram_end
    }

    /// Return templates for all 4 supported architectures.
    pub fn all_templates() -> Vec<Self> {
        Architecture::ALL
            .iter()
            .map(|&arch| Self::for_architecture(arch))
            .collect()
    }
}

impl Architecture {
    /// Return the standard memory map layout for this architecture.
    pub fn memory_map(&self) -> ArchitectureMemoryMap {
        ArchitectureMemoryMap::for_architecture(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_riscv64_memory_map() {
        let map = ArchitectureMemoryMap::riscv64();
        assert_eq!(map.arch, Architecture::Riscv64);
        assert_eq!(map.ram_base, 0x8000_0000);
        assert_eq!(map.default_reset_vector, 0x8000_0000);
        assert_eq!(map.alt_reset_vector, Some(0x0000_1000));
        assert_eq!(map.uart_mmio_base, Some(0x1000_0000));
        assert_eq!(map.block_device_base, Some(0x2000_0000));
        assert_eq!(map.interrupt_controller_base, Some(0x0C00_0000));

        let ram_entry = map.find_entry_by_name("RAM").unwrap();
        assert_eq!(ram_entry.base, 0x8000_0000);
        assert_eq!(ram_entry.size, 0x0400_0000);

        let uart_entry = map.find_entry_at(0x1000_0050).unwrap();
        assert_eq!(uart_entry.name, "UART 16550");

        assert!(map.is_valid_ram_address(0x8000_0000, 4));
        assert!(map.is_valid_ram_address(0x83FF_FFFC, 4));
        assert!(!map.is_valid_ram_address(0x8400_0000, 4));
        assert!(!map.is_valid_ram_address(0x1000_0000, 4));
    }

    #[test]
    fn test_arm64_memory_map() {
        let map = ArchitectureMemoryMap::arm64();
        assert_eq!(map.arch, Architecture::Arm64);
        assert_eq!(map.ram_base, 0x4000_0000);
        assert_eq!(map.default_reset_vector, 0x4000_0000);
        assert_eq!(map.alt_reset_vector, Some(0x0000_0000));
        assert_eq!(map.uart_mmio_base, Some(0x0900_0000));
        assert_eq!(map.block_device_base, Some(0x2000_0000));
        assert_eq!(map.interrupt_controller_base, Some(0x0800_0000));

        let uart_entry = map.find_entry_at(0x0900_0000).unwrap();
        assert_eq!(uart_entry.name, "UART 16550");

        assert!(map.is_valid_ram_address(0x4000_0000, 8));
        assert!(!map.is_valid_ram_address(0x8000_0000, 8));
    }

    #[test]
    fn test_x86_64_memory_map() {
        let map = ArchitectureMemoryMap::x86_64();
        assert_eq!(map.arch, Architecture::X86_64);
        assert_eq!(map.ram_base, 0x0010_0000);
        assert_eq!(map.default_reset_vector, 0x0010_0000);
        assert_eq!(map.alt_reset_vector, Some(0x000F_FFF0));
        assert_eq!(map.uart_mmio_base, Some(0x1000_0000));
        assert_eq!(map.uart_port_io_base, Some(0x03F8));
        assert_eq!(map.block_device_base, Some(0x2000_0000));
        assert_eq!(map.interrupt_controller_base, Some(0xFEE0_0000));

        let bios_entry = map.find_entry_at(0x000F_FFF0).unwrap();
        assert!(bios_entry.name.contains("BIOS"));

        assert!(map.is_valid_ram_address(0x0010_0000, 16));
        assert!(!map.is_valid_ram_address(0x0000_0000, 16));
    }

    #[test]
    fn test_power_iso64_memory_map() {
        let map = ArchitectureMemoryMap::power_iso64();
        assert_eq!(map.arch, Architecture::PowerIso64);
        assert_eq!(map.ram_base, 0x0000_0000);
        assert_eq!(map.default_reset_vector, 0x0000_0100);
        assert_eq!(map.alt_reset_vector, Some(0x8000_0000));
        assert_eq!(map.uart_mmio_base, Some(0x1000_0000));
        assert_eq!(map.block_device_base, Some(0x2000_0000));
        assert_eq!(map.interrupt_controller_base, Some(0x3000_0000));

        let intc_entry = map.find_entry_at(0x3000_0000).unwrap();
        assert!(intc_entry.name.contains("OpenPIC"));

        assert!(map.is_valid_ram_address(0x0000_0100, 4));
        assert!(map.is_valid_ram_address(0x03FF_FFF0, 4));
        assert!(!map.is_valid_ram_address(0x0400_0000, 4));
    }

    #[test]
    fn test_architecture_memory_map_lookup() {
        for &arch in &Architecture::ALL {
            let map = arch.memory_map();
            assert_eq!(map.arch, arch);
            assert!(!map.entries.is_empty());
        }

        assert_eq!(ArchitectureMemoryMap::all_templates().len(), 4);
    }
}
