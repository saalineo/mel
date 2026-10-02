//! Data-driven machine configuration profile and YAML schema.

use crate::arch::Architecture;
use crate::error::{MelError, MelResult};
use crate::memory_maps::{ArchitectureMemoryMap, MemoryMapEntry};
use crate::types::{Address, Permissions, Size};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

fn default_ram_size() -> Size {
    64 * 1024 * 1024 // 64 MB
}

fn default_uart_size() -> Size {
    0x0100 // 256 B
}

fn default_clint_size() -> Size {
    0x0001_0000 // 64 KB
}

fn default_plic_size() -> Size {
    0x0400_0000 // 64 MB
}

fn default_block_size() -> Size {
    0x1000 // 4 KB
}

fn default_gic_size() -> Size {
    0x0001_0000 // 64 KB
}

fn default_apic_size() -> Size {
    0x1000 // 4 KB
}

fn default_hart_count() -> usize {
    1
}

fn default_context_count() -> usize {
    1
}

fn default_source_count() -> usize {
    32
}

/// Declarative machine specification profile for instantiating custom virtual machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineProfile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub architecture: Architecture,
    #[serde(default = "default_ram_size")]
    pub ram_size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram_base: Option<Address>,
    pub boot: BootConfig,
    #[serde(default)]
    pub devices: DevicesConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_memory_regions: Vec<MemoryMapEntry>,
}

/// Boot configuration including reset vector, entry point, and initial payload/ROM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootConfig {
    pub entry_point: Address,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_vector: Option<Address>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rom: Option<BootRomConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_path: Option<String>,
}

/// Boot ROM image configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootRomConfig {
    pub base: Address,
    pub size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

/// Declarative peripheral devices configuration attached to the system bus.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DevicesConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uart: Option<UartDeviceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clint: Option<ClintDeviceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plic: Option<PlicDeviceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<BlockDeviceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gic: Option<GicDeviceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apic: Option<ApicDeviceConfig>,
}

/// Serial UART peripheral configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UartDeviceConfig {
    pub base: Address,
    #[serde(default = "default_uart_size")]
    pub size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub irq: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_io: Option<u16>,
}

/// RISC-V CLINT timer and software interrupt device configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClintDeviceConfig {
    pub base: Address,
    #[serde(default = "default_clint_size")]
    pub size: Size,
    #[serde(default = "default_hart_count")]
    pub harts: usize,
}

/// RISC-V PLIC platform interrupt controller configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlicDeviceConfig {
    pub base: Address,
    #[serde(default = "default_plic_size")]
    pub size: Size,
    #[serde(default = "default_context_count")]
    pub contexts: usize,
    #[serde(default = "default_source_count")]
    pub max_sources: usize,
}

/// Virtual block storage disk device configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDeviceConfig {
    pub base: Address,
    #[serde(default = "default_block_size")]
    pub size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub irq: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

/// ARM Generic Interrupt Controller configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GicDeviceConfig {
    pub base: Address,
    #[serde(default = "default_gic_size")]
    pub size: Size,
}

/// x86 Local APIC configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApicDeviceConfig {
    pub base: Address,
    #[serde(default = "default_apic_size")]
    pub size: Size,
}

impl MachineProfile {
    /// Return the default profile for a given CPU architecture.
    pub fn default_for_arch(arch: Architecture) -> Self {
        match arch {
            Architecture::Riscv64 => Self::riscv64_default(),
            Architecture::Arm64 => Self::arm64_default(),
            Architecture::X86_64 => Self::x86_64_default(),
            Architecture::PowerIso64 => Self::power_iso64_default(),
        }
    }

    /// Standard default profile for RISC-V 64 virt machine.
    pub fn riscv64_default() -> Self {
        Self {
            name: "riscv64-virt".to_string(),
            description: Some("Standard RISC-V RV64GC virtual machine".to_string()),
            architecture: Architecture::Riscv64,
            ram_size: 64 * 1024 * 1024,
            ram_base: Some(0x8000_0000),
            boot: BootConfig {
                entry_point: 0x8000_0000,
                reset_vector: Some(0x8000_0000),
                rom: Some(BootRomConfig {
                    base: 0x0000_1000,
                    size: 0x1000,
                    image_path: None,
                }),
                payload_path: None,
            },
            devices: DevicesConfig {
                uart: Some(UartDeviceConfig {
                    base: 0x1000_0000,
                    size: 0x0100,
                    irq: Some(10),
                    port_io: None,
                }),
                clint: Some(ClintDeviceConfig {
                    base: 0x0200_0000,
                    size: 0x0001_0000,
                    harts: 1,
                }),
                plic: Some(PlicDeviceConfig {
                    base: 0x0C00_0000,
                    size: 0x0400_0000,
                    contexts: 1,
                    max_sources: 32,
                }),
                block: Some(BlockDeviceConfig {
                    base: 0x2000_0000,
                    size: 0x1000,
                    irq: Some(1),
                    image_path: None,
                }),
                gic: None,
                apic: None,
            },
            custom_memory_regions: Vec::new(),
        }
    }

    /// Standard default profile for ARM64 virt machine.
    pub fn arm64_default() -> Self {
        Self {
            name: "arm64-virt".to_string(),
            description: Some("Standard ARM64 AArch64 virtual machine".to_string()),
            architecture: Architecture::Arm64,
            ram_size: 64 * 1024 * 1024,
            ram_base: Some(0x4000_0000),
            boot: BootConfig {
                entry_point: 0x4000_0000,
                reset_vector: Some(0x4000_0000),
                rom: Some(BootRomConfig {
                    base: 0x0000_0000,
                    size: 0x0001_0000,
                    image_path: None,
                }),
                payload_path: None,
            },
            devices: DevicesConfig {
                uart: Some(UartDeviceConfig {
                    base: 0x0900_0000,
                    size: 0x0100,
                    irq: Some(1),
                    port_io: None,
                }),
                clint: None,
                plic: None,
                block: Some(BlockDeviceConfig {
                    base: 0x2000_0000,
                    size: 0x1000,
                    irq: Some(2),
                    image_path: None,
                }),
                gic: Some(GicDeviceConfig {
                    base: 0x0800_0000,
                    size: 0x0001_0000,
                }),
                apic: None,
            },
            custom_memory_regions: Vec::new(),
        }
    }

    /// Standard default profile for x86-64 virt machine.
    pub fn x86_64_default() -> Self {
        Self {
            name: "x86_64-virt".to_string(),
            description: Some("Standard x86-64 PC-compatible virtual machine".to_string()),
            architecture: Architecture::X86_64,
            ram_size: 64 * 1024 * 1024,
            ram_base: Some(0x0010_0000),
            boot: BootConfig {
                entry_point: 0x0010_0000,
                reset_vector: Some(0x0010_0000),
                rom: Some(BootRomConfig {
                    base: 0x000F_0000,
                    size: 0x0001_0000,
                    image_path: None,
                }),
                payload_path: None,
            },
            devices: DevicesConfig {
                uart: Some(UartDeviceConfig {
                    base: 0x1000_0000,
                    size: 0x0100,
                    irq: Some(4),
                    port_io: Some(0x03F8),
                }),
                clint: None,
                plic: None,
                block: Some(BlockDeviceConfig {
                    base: 0x2000_0000,
                    size: 0x1000,
                    irq: Some(14),
                    image_path: None,
                }),
                gic: None,
                apic: Some(ApicDeviceConfig {
                    base: 0xFEE0_0000,
                    size: 0x1000,
                }),
            },
            custom_memory_regions: Vec::new(),
        }
    }

    /// Standard default profile for Power ISA (PPC64) virt machine.
    pub fn power_iso64_default() -> Self {
        Self {
            name: "ppc64-virt".to_string(),
            description: Some("Standard Power ISA 64-bit virtual machine".to_string()),
            architecture: Architecture::PowerIso64,
            ram_size: 64 * 1024 * 1024,
            ram_base: Some(0x0000_0000),
            boot: BootConfig {
                entry_point: 0x0000_0100,
                reset_vector: Some(0x0000_0100),
                rom: None,
                payload_path: None,
            },
            devices: DevicesConfig {
                uart: Some(UartDeviceConfig {
                    base: 0x1000_0000,
                    size: 0x0100,
                    irq: Some(1),
                    port_io: None,
                }),
                clint: None,
                plic: None,
                block: Some(BlockDeviceConfig {
                    base: 0x2000_0000,
                    size: 0x1000,
                    irq: Some(2),
                    image_path: None,
                }),
                gic: None,
                apic: None,
            },
            custom_memory_regions: Vec::new(),
        }
    }

    /// Return the effective physical RAM base address.
    pub fn effective_ram_base(&self) -> Address {
        self.ram_base
            .unwrap_or_else(|| self.architecture.default_ram_base())
    }

    /// Return the effective hardware reset vector address.
    pub fn effective_reset_vector(&self) -> Address {
        self.boot
            .reset_vector
            .unwrap_or(self.boot.entry_point)
    }

    /// Deserialize a MachineProfile from YAML string and validate.
    pub fn from_yaml_str(yaml_str: &str) -> MelResult<Self> {
        let profile: MachineProfile = serde_yaml::from_str(yaml_str)
            .map_err(|e| MelError::Config(format!("Failed to parse machine YAML profile: {e}")))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Serialize this MachineProfile to YAML string.
    pub fn to_yaml_str(&self) -> MelResult<String> {
        serde_yaml::to_string(self)
            .map_err(|e| MelError::Config(format!("Failed to serialize machine profile: {e}")))
    }

    /// Load and validate a MachineProfile from a YAML file.
    pub fn from_yaml_file(path: impl AsRef<Path>) -> MelResult<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|e| {
            MelError::Config(format!(
                "Failed to read machine profile at '{}': {e}",
                path.display()
            ))
        })?;
        Self::from_yaml_str(&content)
    }

    /// Save this MachineProfile to a YAML file.
    pub fn to_yaml_file(&self, path: impl AsRef<Path>) -> MelResult<()> {
        let path = path.as_ref();
        let yaml_str = self.to_yaml_str()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                MelError::Config(format!(
                    "Failed to create directory for profile '{}': {e}",
                    parent.display()
                ))
            })?;
        }
        fs::write(path, yaml_str).map_err(|e| {
            MelError::Config(format!(
                "Failed to write machine profile to '{}': {e}",
                path.display()
            ))
        })?;
        Ok(())
    }

    /// Validate the machine profile configuration for invariants.
    pub fn validate(&self) -> MelResult<()> {
        if self.ram_size == 0 {
            return Err(MelError::Config(
                "Machine profile RAM size must be greater than zero".to_string(),
            ));
        }

        // Limit RAM to 16 GB max to prevent overflow or unreasonable allocations
        if self.ram_size > 16 * 1024 * 1024 * 1024 {
            return Err(MelError::Config(format!(
                "Machine profile RAM size ({} bytes) exceeds maximum limit (16 GB)",
                self.ram_size
            )));
        }

        let ram_base = self.effective_ram_base();
        if ram_base.checked_add(self.ram_size).is_none() {
            return Err(MelError::Config(
                "Physical RAM range overflows 64-bit address space".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate an ArchitectureMemoryMap representing all configured memory regions and devices.
    pub fn to_memory_map(&self) -> ArchitectureMemoryMap {
        let mut entries = Vec::new();

        if let Some(rom) = &self.boot.rom {
            entries.push(MemoryMapEntry::new(
                "Boot ROM",
                rom.base,
                rom.size,
                Permissions::READ_EXECUTE,
                "Hardware Boot ROM",
            ));
        }

        if let Some(clint) = &self.devices.clint {
            entries.push(MemoryMapEntry::new(
                "CLINT",
                clint.base,
                clint.size,
                Permissions::READ_WRITE,
                "Core Local Interruptor",
            ));
        }

        if let Some(plic) = &self.devices.plic {
            entries.push(MemoryMapEntry::new(
                "PLIC",
                plic.base,
                plic.size,
                Permissions::READ_WRITE,
                "Platform-Level Interrupt Controller",
            ));
        }

        if let Some(gic) = &self.devices.gic {
            entries.push(MemoryMapEntry::new(
                "GIC",
                gic.base,
                gic.size,
                Permissions::READ_WRITE,
                "Generic Interrupt Controller",
            ));
        }

        if let Some(apic) = &self.devices.apic {
            entries.push(MemoryMapEntry::new(
                "Local APIC",
                apic.base,
                apic.size,
                Permissions::READ_WRITE,
                "Advanced Programmable Interrupt Controller",
            ));
        }

        if let Some(uart) = &self.devices.uart {
            entries.push(MemoryMapEntry::new(
                "UART 16550",
                uart.base,
                uart.size,
                Permissions::READ_WRITE,
                "Serial Console MMIO",
            ));
        }

        if let Some(block) = &self.devices.block {
            entries.push(MemoryMapEntry::new(
                "Block Storage",
                block.base,
                block.size,
                Permissions::READ_WRITE,
                "Virtual Block Storage Device",
            ));
        }

        entries.push(MemoryMapEntry::new(
            "Main RAM",
            self.effective_ram_base(),
            self.ram_size,
            Permissions::ALL,
            "System Physical RAM",
        ));

        for custom in &self.custom_memory_regions {
            entries.push(custom.clone());
        }

        ArchitectureMemoryMap {
            arch: self.architecture,
            ram_base: self.effective_ram_base(),
            default_ram_size: self.ram_size,
            default_reset_vector: self.effective_reset_vector(),
            alt_reset_vector: self.boot.reset_vector,
            uart_mmio_base: self.devices.uart.as_ref().map(|u| u.base),
            uart_port_io_base: self.devices.uart.as_ref().and_then(|u| u.port_io),
            block_device_base: self.devices.block.as_ref().map(|b| b.base),
            interrupt_controller_base: self
                .devices
                .plic
                .as_ref()
                .map(|p| p.base)
                .or_else(|| self.devices.gic.as_ref().map(|g| g.base))
                .or_else(|| self.devices.apic.as_ref().map(|a| a.base)),
            boot_rom_base: self.boot.rom.as_ref().map(|r| r.base),
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_defaults_all_architectures() {
        for &arch in &Architecture::ALL {
            let profile = MachineProfile::default_for_arch(arch);
            assert_eq!(profile.architecture, arch);
            assert_eq!(profile.ram_size, 64 * 1024 * 1024);
            assert!(profile.validate().is_ok());

            let mmap = profile.to_memory_map();
            assert_eq!(mmap.arch, arch);
            assert!(mmap.ram_base == profile.effective_ram_base());
        }
    }

    #[test]
    fn test_profile_yaml_roundtrip() {
        let profile = MachineProfile::riscv64_default();
        let yaml = profile.to_yaml_str().unwrap();
        assert!(yaml.contains("riscv64-virt"));
        assert!(yaml.contains("architecture: riscv64"));

        let restored = MachineProfile::from_yaml_str(&yaml).unwrap();
        assert_eq!(profile, restored);
    }

    #[test]
    fn test_profile_yaml_parsing_custom() {
        let yaml = r#"
name: custom-arm64
description: Custom ARM64 test board
architecture: arm64
ram_size: 134217728
ram_base: 0x40000000
boot:
  entry_point: 0x40000000
  reset_vector: 0x40000000
devices:
  uart:
    base: 0x09000000
    size: 256
    irq: 1
"#;
        let profile = MachineProfile::from_yaml_str(yaml).unwrap();
        assert_eq!(profile.name, "custom-arm64");
        assert_eq!(profile.architecture, Architecture::Arm64);
        assert_eq!(profile.ram_size, 128 * 1024 * 1024);
        assert_eq!(profile.boot.entry_point, 0x4000_0000);
        assert!(profile.devices.uart.is_some());
    }

    #[test]
    fn test_profile_validation_errors() {
        let mut profile = MachineProfile::riscv64_default();
        profile.ram_size = 0;
        assert!(profile.validate().is_err());

        profile.ram_size = 32 * 1024 * 1024 * 1024; // 32 GB exceeds max 16GB
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_load_machine_yaml_files() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        let machine_files = [
            ("machines/riscv64_virt.yaml", Architecture::Riscv64),
            ("machines/arm64_virt.yaml", Architecture::Arm64),
            ("machines/x86_64_virt.yaml", Architecture::X86_64),
            ("machines/ppc64_virt.yaml", Architecture::PowerIso64),
        ];

        for (rel_path, expected_arch) in machine_files {
            let path = workspace_root.join(rel_path);
            let profile = MachineProfile::from_yaml_file(&path)
                .unwrap_or_else(|e| panic!("Failed to load {}: {e}", path.display()));
            assert_eq!(profile.architecture, expected_arch);
            assert!(profile.ram_size > 0);
            assert!(profile.validate().is_ok());

            let mmap = profile.to_memory_map();
            assert_eq!(mmap.arch, expected_arch);
        }
    }
}
