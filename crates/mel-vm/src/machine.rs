use mel_bus::SystemBus;
use mel_core::{Architecture, MelError, RegisterId};
use mel_cpu::traits::{CpuBackend, CpuStepResult};
use mel_cpu::{MockCpu, Riscv64Cpu};
use mel_devices::{BootRom, MmioDevice, SharedMmioBusAdapter, Uart16550};
use mel_memory::Ram;
use std::sync::{Arc, Mutex};
use unicorn_engine::unicorn_const::Prot;

/// Unified virtual machine representing a complete computer motherboard.
pub struct VirtualMachine {
    arch: Architecture,
    cpu: Box<dyn CpuBackend>,
    bus: Arc<Mutex<SystemBus>>,
    ram: Arc<Mutex<Ram>>,
    devices: Vec<Arc<Mutex<dyn MmioDevice>>>,
    uart: Option<Arc<Mutex<Uart16550>>>,
    rom: Option<Arc<Mutex<BootRom>>>,
}

impl VirtualMachine {
    /// Construct a new VirtualMachine from components.
    pub fn new(
        arch: Architecture,
        cpu: Box<dyn CpuBackend>,
        bus: Arc<Mutex<SystemBus>>,
        ram: Arc<Mutex<Ram>>,
        devices: Vec<Arc<Mutex<dyn MmioDevice>>>,
        uart: Option<Arc<Mutex<Uart16550>>>,
        rom: Option<Arc<Mutex<BootRom>>>,
    ) -> Self {
        Self {
            arch,
            cpu,
            bus,
            ram,
            devices,
            uart,
            rom,
        }
    }

    /// Machine target CPU architecture.
    pub fn arch(&self) -> Architecture {
        self.arch
    }

    /// Advance the virtual machine by a single instruction step.
    pub fn step(&mut self) -> Result<CpuStepResult, MelError> {
        let step_result = self.cpu.step()?;
        let cycles = match step_result {
            CpuStepResult::Ok { cycles } => cycles,
            _ => 1,
        };
        for dev in &self.devices {
            if let Ok(mut d) = dev.lock() {
                let _ = d.tick(cycles);
            }
        }
        Ok(step_result)
    }

    /// Run the virtual machine for up to `max_cycles` or until a halt/breakpoint occurs.
    pub fn run(&mut self, max_cycles: u64) -> Result<CpuStepResult, MelError> {
        let run_result = self.cpu.run(max_cycles)?;
        let cycles = match run_result {
            CpuStepResult::Ok { cycles } => cycles,
            _ => 1,
        };
        for dev in &self.devices {
            if let Ok(mut d) = dev.lock() {
                let _ = d.tick(cycles);
            }
        }
        Ok(run_result)
    }

    /// Reset the CPU and all peripherals to power-on state.
    pub fn reset(&mut self) -> Result<(), MelError> {
        self.cpu.reset()?;
        for dev in &self.devices {
            if let Ok(mut d) = dev.lock() {
                let _ = d.reset();
            }
        }
        Ok(())
    }

    /// Read program counter.
    pub fn pc(&self) -> Result<u64, MelError> {
        self.cpu.pc()
    }

    /// Set program counter.
    pub fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.cpu.set_pc(pc)
    }

    /// Read architectural register value.
    pub fn read_register(&self, reg: RegisterId) -> Result<u64, MelError> {
        self.cpu.read_register(reg)
    }

    /// Write architectural register value.
    pub fn write_register(&mut self, reg: RegisterId, val: u64) -> Result<(), MelError> {
        self.cpu.write_register(reg, val)
    }

    /// Access the system bus.
    pub fn bus(&self) -> &Arc<Mutex<SystemBus>> {
        &self.bus
    }

    /// Access the main RAM buffer.
    pub fn ram(&self) -> &Arc<Mutex<Ram>> {
        &self.ram
    }

    /// Access primary UART if mapped.
    pub fn uart(&self) -> Option<Arc<Mutex<Uart16550>>> {
        self.uart.clone()
    }

    /// Access Boot ROM if mapped.
    pub fn boot_rom(&self) -> Option<Arc<Mutex<BootRom>>> {
        self.rom.clone()
    }

    /// Write data into VM system memory and synchronize with CPU backend.
    pub fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), MelError> {
        self.bus
            .lock()
            .map_err(|e| MelError::Cpu(mel_core::CpuError::Internal(e.to_string())))?
            .write_bytes(addr, data)?;
        self.cpu.mem_write(addr, data)?;
        Ok(())
    }

    /// Read data from VM system memory.
    pub fn read_memory(&self, addr: u64, data: &mut [u8]) -> Result<(), MelError> {
        self.bus
            .lock()
            .map_err(|e| MelError::Cpu(mel_core::CpuError::Internal(e.to_string())))?
            .read_bytes(addr, data)?;
        Ok(())
    }

    /// Immutable CPU backend handle.
    pub fn cpu(&self) -> &dyn CpuBackend {
        &*self.cpu
    }

    /// Mutable CPU backend handle.
    pub fn cpu_mut(&mut self) -> &mut dyn CpuBackend {
        &mut *self.cpu
    }
}

/// Factory function building a standard RISC-V 64-bit virtual machine.
///
/// Memory Layout:
/// - `0x0000_1000` - `0x0000_1FFF` (4 KB): Boot ROM (Reset vector jump to RAM)
/// - `0x1000_0000` - `0x1000_00FF` (256 B): NS16550 Serial UART
/// - `0x8000_0000` - `0x80FF_FFFF` (16 MB): System RAM
pub fn create_basic_riscv64_vm() -> Result<VirtualMachine, MelError> {
    let mut bus = SystemBus::new();

    // 1. RAM Allocation (16 MB at 0x8000_0000)
    let ram_size = 16 * 1024 * 1024;
    let ram = Arc::new(Mutex::new(Ram::new(ram_size)));
    bus.map_ram("RAM", 0x8000_0000, ram.clone())?;

    // 2. Boot ROM (4 KB at 0x0000_1000 jumping to 0x8000_0000)
    let rom = Arc::new(Mutex::new(BootRom::riscv64_jump_to(
        "boot_rom",
        0x8000_0000,
    )));
    let rom_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(rom.clone())));
    bus.map_device("BOOT_ROM", 0x0000_1000, 0x1000, rom_adapter)?;

    // 3. UART 16550 (256 B at 0x1000_0000)
    let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
    let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
    bus.map_device("UART", 0x1000_0000, 0x100, uart_adapter)?;

    let bus_arc = Arc::new(Mutex::new(bus));

    // 4. RISC-V 64 CPU backend with bus bridge
    let mut cpu = Riscv64Cpu::with_bus(bus_arc.clone())?;
    cpu.mem_map(0x8000_0000, ram_size as u64, Prot::ALL)?;
    cpu.mem_map(0x0000_1000, 0x1000, Prot::ALL)?;
    cpu.set_pc(0x8000_0000)?;

    let devices: Vec<Arc<Mutex<dyn MmioDevice>>> = vec![rom.clone(), uart.clone()];

    Ok(VirtualMachine::new(
        Architecture::Riscv64,
        Box::new(cpu),
        bus_arc,
        ram,
        devices,
        Some(uart),
        Some(rom),
    ))
}

/// Factory function building a lightweight mock virtual machine for unit testing.
pub fn create_mock_vm(arch: Architecture) -> Result<VirtualMachine, MelError> {
    let mut bus = SystemBus::new();
    let ram_base = arch.default_ram_base();
    let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
    bus.map_ram("RAM", ram_base, ram.clone())?;

    let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
    let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
    bus.map_device("UART", 0x1000_0000, 0x100, uart_adapter)?;

    let bus_arc = Arc::new(Mutex::new(bus));
    let cpu = Box::new(MockCpu::new(arch));
    let devices: Vec<Arc<Mutex<dyn MmioDevice>>> = vec![uart.clone()];

    Ok(VirtualMachine::new(
        arch,
        cpu,
        bus_arc,
        ram,
        devices,
        Some(uart),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_core::Endianness;

    #[test]
    fn test_basic_riscv64_vm_composition() {
        let vm = create_basic_riscv64_vm().unwrap();
        assert_eq!(vm.arch(), Architecture::Riscv64);
        assert_eq!(vm.pc().unwrap(), 0x8000_0000);

        // Verify RAM is accessible via VM and bus
        vm.bus()
            .lock()
            .unwrap()
            .write(0x8000_0000, 0x12345678, 4, Endianness::Little)
            .unwrap();

        assert_eq!(
            vm.bus()
                .lock()
                .unwrap()
                .read(0x8000_0000, 4, Endianness::Little)
                .unwrap(),
            0x12345678
        );

        // Verify UART output access
        assert!(vm.uart().is_some());
        assert!(vm.boot_rom().is_some());
    }

    #[test]
    fn test_riscv64_vm_code_execution_and_uart() {
        let mut vm = create_basic_riscv64_vm().unwrap();

        // Write assembly:
        // lui x1, 0x10000    (UART base)
        // addi x2, x0, 88    ('X')
        // sb x2, 0(x1)       (write to UART)
        let code: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x80, 0x05, 0x23, 0x80, 0x20, 0x00,
        ];

        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        let s1 = vm.step().unwrap();
        assert_eq!(s1, CpuStepResult::Ok { cycles: 1 });
        let s2 = vm.step().unwrap();
        assert_eq!(s2, CpuStepResult::Ok { cycles: 1 });
        let s3 = vm.step().unwrap();
        assert_eq!(s3, CpuStepResult::Ok { cycles: 1 });

        let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
        assert_eq!(uart_out, b"X");
    }

    #[test]
    fn test_mock_vm_reset_and_registers() {
        let mut vm = create_mock_vm(Architecture::Arm64).unwrap();
        assert_eq!(vm.arch(), Architecture::Arm64);

        vm.write_register(RegisterId::named("x0"), 0xFEEDFACE)
            .unwrap();
        assert_eq!(
            vm.read_register(RegisterId::named("x0")).unwrap(),
            0xFEEDFACE
        );

        vm.step().unwrap();
        assert_eq!(vm.pc().unwrap(), 0x4000_0004);

        vm.reset().unwrap();
        assert_eq!(vm.pc().unwrap(), 0x4000_0000);
    }
}
