//! VM builder factory constructing `VirtualMachine` instances directly from `MachineProfile` definitions.

use crate::machine::VirtualMachine;
use mel_bus::SystemBus;
use mel_core::profile::MachineProfile;
use mel_core::{Architecture, MelError, MelResult};
use mel_cpu::traits::CpuBackend;
use mel_cpu::{Arm64Cpu, PowerIso64Cpu, Riscv64Cpu, X86_64Cpu};
use mel_devices::{
    BlockDevice, BootRom, Clint, DummyDevice, MmioDevice, Plic, SharedMmioBusAdapter, Uart16550,
};
use mel_memory::Ram;
use std::path::Path;
use std::sync::{Arc, Mutex};
use unicorn_engine::unicorn_const::Prot;

/// Construct a fully initialized [`VirtualMachine`] instance from a [`MachineProfile`].
///
/// This dynamically allocates physical RAM, configures the system bus, maps MMIO peripherals
/// (UART, CLINT, PLIC, Block Device, Boot ROM, GIC/APIC dummies), instantiates the CPU backend
/// for the specified architecture, and sets the entry point.
pub fn build_vm_from_profile(profile: &MachineProfile) -> MelResult<VirtualMachine> {
    // 1. Validate configuration invariants
    profile.validate()?;

    let mut bus = SystemBus::new();
    let ram_base = profile.effective_ram_base();
    let ram_size = profile.ram_size;

    // 2. Physical RAM allocation & bus mapping
    let ram_size_usize = usize::try_from(ram_size).map_err(|_| {
        MelError::Config(format!(
            "RAM size {ram_size} exceeds addressable host pointer width"
        ))
    })?;
    let ram = Arc::new(Mutex::new(Ram::new(ram_size_usize)));
    bus.map_ram("RAM", ram_base, ram.clone())?;

    // 3. Optional Boot ROM peripheral
    let rom = if let Some(rom_cfg) = &profile.boot.rom {
        let rom_data = if let Some(image_path) = &rom_cfg.image_path {
            std::fs::read(image_path).map_err(|e| {
                MelError::Config(format!(
                    "Failed to read boot ROM image from '{}': {e}",
                    image_path
                ))
            })?
        } else {
            match profile.architecture {
                Architecture::Riscv64 => {
                    if rom_cfg.size >= 24 {
                        BootRom::riscv64_jump_to("boot_rom", profile.boot.entry_point)
                            .as_slice()
                            .to_vec()
                    } else {
                        vec![0u8; rom_cfg.size as usize]
                    }
                }
                _ => vec![0u8; rom_cfg.size as usize],
            }
        };

        let boot_rom = Arc::new(Mutex::new(BootRom::new(
            "boot_rom",
            &rom_data,
            rom_cfg.size as usize,
        )));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(boot_rom.clone())));
        bus.map_device("BOOT_ROM", rom_cfg.base, rom_cfg.size, adapter)?;
        Some(boot_rom)
    } else {
        None
    };

    // 4. Peripheral MMIO Devices
    let mut aux_devices: Vec<Arc<Mutex<dyn MmioDevice>>> = Vec::new();

    // UART 16550
    let uart = if let Some(uart_cfg) = &profile.devices.uart {
        let uart_dev = Arc::new(Mutex::new(Uart16550::new("uart0")));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart_dev.clone())));
        bus.map_device("UART", uart_cfg.base, uart_cfg.size, adapter)?;
        Some(uart_dev)
    } else {
        None
    };

    // RISC-V CLINT
    let clint = if let Some(clint_cfg) = &profile.devices.clint {
        let clint_dev = Arc::new(Mutex::new(Clint::new("clint0")));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(clint_dev.clone())));
        bus.map_device("CLINT", clint_cfg.base, clint_cfg.size, adapter)?;
        Some(clint_dev)
    } else {
        None
    };

    // RISC-V PLIC
    let plic = if let Some(plic_cfg) = &profile.devices.plic {
        let plic_dev = Arc::new(Mutex::new(Plic::new("plic0")));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(plic_dev.clone())));
        bus.map_device("PLIC", plic_cfg.base, plic_cfg.size, adapter)?;
        Some(plic_dev)
    } else {
        None
    };

    // Virtual Block Storage Device
    let block = if let Some(block_cfg) = &profile.devices.block {
        let block_dev = if let Some(image_path) = &block_cfg.image_path {
            let disk_data = std::fs::read(image_path).map_err(|e| {
                MelError::Config(format!(
                    "Failed to read block disk image from '{}': {e}",
                    image_path
                ))
            })?;
            BlockDevice::with_data("blk0", disk_data)
        } else {
            BlockDevice::new("blk0", block_cfg.size.max(1024 * 1024) as usize)
        };
        let block_arc = Arc::new(Mutex::new(block_dev));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(block_arc.clone())));
        bus.map_device("BLOCK", block_cfg.base, block_cfg.size, adapter)?;
        Some(block_arc)
    } else {
        None
    };

    // ARM GIC (stub MMIO adapter)
    if let Some(gic_cfg) = &profile.devices.gic {
        let gic_dev = Arc::new(Mutex::new(DummyDevice::new("gic0", gic_cfg.size)));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(gic_dev.clone())));
        bus.map_device("GIC", gic_cfg.base, gic_cfg.size, adapter)?;
        aux_devices.push(gic_dev);
    }

    // x86 Local APIC (stub MMIO adapter)
    if let Some(apic_cfg) = &profile.devices.apic {
        let apic_dev = Arc::new(Mutex::new(DummyDevice::new("apic0", apic_cfg.size)));
        let adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(apic_dev.clone())));
        bus.map_device("APIC", apic_cfg.base, apic_cfg.size, adapter)?;
        aux_devices.push(apic_dev);
    }

    // Custom memory regions
    for region in &profile.custom_memory_regions {
        let region_size_usize = usize::try_from(region.size).map_err(|_| {
            MelError::Config(format!(
                "Custom memory region '{}' size {} exceeds host pointer width",
                region.name, region.size
            ))
        })?;
        let custom_ram = Arc::new(Mutex::new(Ram::new(region_size_usize)));
        bus.map_ram(&region.name, region.base, custom_ram)?;
    }

    let bus_arc = Arc::new(Mutex::new(bus));

    // 5. Dynamic CPU backend allocation & memory mapping
    let cpu: Box<dyn CpuBackend> = match profile.architecture {
        Architecture::Riscv64 => {
            let mut cpu = Riscv64Cpu::with_bus(bus_arc.clone())?;
            cpu.mem_map(ram_base, ram_size, Prot::ALL)?;
            if let Some(rom_cfg) = &profile.boot.rom {
                cpu.mem_map(rom_cfg.base, rom_cfg.size, Prot::ALL)?;
                if let Some(rom_arc) = &rom {
                    if let Ok(r) = rom_arc.lock() {
                        cpu.mem_write(rom_cfg.base, r.as_slice())?;
                    }
                }
            }
            for region in &profile.custom_memory_regions {
                if region.size > 0 {
                    cpu.mem_map(region.base, region.size, Prot::ALL)?;
                }
            }
            cpu.set_pc(profile.boot.entry_point)?;
            Box::new(cpu)
        }
        Architecture::Arm64 => {
            let mut cpu = Arm64Cpu::with_bus(bus_arc.clone())?;
            cpu.mem_map(ram_base, ram_size as usize, Prot::ALL)?;
            if let Some(rom_cfg) = &profile.boot.rom {
                cpu.mem_map(rom_cfg.base, rom_cfg.size as usize, Prot::ALL)?;
                if let Some(rom_arc) = &rom {
                    if let Ok(r) = rom_arc.lock() {
                        cpu.mem_write(rom_cfg.base, r.as_slice())?;
                    }
                }
            }
            for region in &profile.custom_memory_regions {
                if region.size > 0 {
                    cpu.mem_map(region.base, region.size as usize, Prot::ALL)?;
                }
            }
            cpu.set_pc(profile.boot.entry_point)?;
            Box::new(cpu)
        }
        Architecture::X86_64 => {
            let mut cpu = X86_64Cpu::with_bus(bus_arc.clone())?;
            cpu.mem_map(ram_base, ram_size as usize, Prot::ALL)?;
            if let Some(rom_cfg) = &profile.boot.rom {
                cpu.mem_map(rom_cfg.base, rom_cfg.size as usize, Prot::ALL)?;
                if let Some(rom_arc) = &rom {
                    if let Ok(r) = rom_arc.lock() {
                        cpu.mem_write(rom_cfg.base, r.as_slice())?;
                    }
                }
            }
            for region in &profile.custom_memory_regions {
                if region.size > 0 {
                    cpu.mem_map(region.base, region.size as usize, Prot::ALL)?;
                }
            }
            cpu.set_pc(profile.boot.entry_point)?;
            Box::new(cpu)
        }
        Architecture::PowerIso64 => {
            let mut cpu = PowerIso64Cpu::with_bus(bus_arc.clone())?;
            cpu.mem_map(ram_base, ram_size as usize, Prot::ALL)?;
            if let Some(rom_cfg) = &profile.boot.rom {
                cpu.mem_map(rom_cfg.base, rom_cfg.size as usize, Prot::ALL)?;
                if let Some(rom_arc) = &rom {
                    if let Ok(r) = rom_arc.lock() {
                        cpu.mem_write(rom_cfg.base, r.as_slice())?;
                    }
                }
            }
            for region in &profile.custom_memory_regions {
                if region.size > 0 {
                    cpu.mem_map(region.base, region.size as usize, Prot::ALL)?;
                }
            }
            cpu.set_pc(profile.boot.entry_point)?;
            Box::new(cpu)
        }
    };

    // 6. Assemble device handles for tick/reset cycles
    let mut devices: Vec<Arc<Mutex<dyn MmioDevice>>> = Vec::new();
    if let Some(rom_arc) = &rom {
        devices.push(rom_arc.clone());
    }
    if let Some(uart_arc) = &uart {
        devices.push(uart_arc.clone());
    }
    if let Some(clint_arc) = &clint {
        devices.push(clint_arc.clone());
    }
    if let Some(plic_arc) = &plic {
        devices.push(plic_arc.clone());
    }
    if let Some(block_arc) = &block {
        devices.push(block_arc.clone());
    }
    devices.extend(aux_devices);

    let mut vm = VirtualMachine::new(
        profile.architecture,
        cpu,
        bus_arc,
        ram,
        devices,
        uart,
        rom,
        clint,
        plic,
        block,
    );

    // 7. Optional payload loading
    if let Some(payload_path) = &profile.boot.payload_path {
        let payload = std::fs::read(payload_path).map_err(|e| {
            MelError::Config(format!(
                "Failed to read payload from '{}': {e}",
                payload_path
            ))
        })?;
        vm.write_memory(profile.boot.entry_point, &payload)?;
    }

    Ok(vm)
}

/// Construct a [`VirtualMachine`] directly from a YAML configuration string.
pub fn build_vm_from_yaml_str(yaml_str: &str) -> MelResult<VirtualMachine> {
    let profile = MachineProfile::from_yaml_str(yaml_str)?;
    build_vm_from_profile(&profile)
}

/// Construct a [`VirtualMachine`] directly from a YAML configuration file path.
pub fn build_vm_from_file(path: impl AsRef<Path>) -> MelResult<VirtualMachine> {
    let profile = MachineProfile::from_yaml_file(path)?;
    build_vm_from_profile(&profile)
}

/// Construct a [`VirtualMachine`] using the standard default profile for an architecture.
pub fn build_vm_for_arch(arch: Architecture) -> MelResult<VirtualMachine> {
    let profile = MachineProfile::default_for_arch(arch);
    build_vm_from_profile(&profile)
}
