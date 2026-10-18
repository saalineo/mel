//! Unicorn-based x86-64 (AMD64) CPU backend.

use crate::traits::{CpuBackend, CpuStepResult};
use mel_core::{Architecture, CpuError, Endianness, MelError, RegisterId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use unicorn_engine::unicorn_const::*;
use unicorn_engine::{RegisterX86, Unicorn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct X86_64State {
    rip: u64,
    rsp: u64,
    rbp: u64,
    rflags: u64,
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    cycles: u64,
}

/// x86-64 (AMD64) CPU backend powered by Unicorn Engine.
pub struct X86_64Cpu {
    uc: Unicorn<'static, ()>,
    breakpoints: HashSet<u64>,
    cycles_executed: u64,
}

// SAFETY: The underlying Unicorn emulator handle is exclusively owned by `X86_64Cpu` and not shared across threads without synchronization.
unsafe impl Send for X86_64Cpu {}
// SAFETY: Mutation requires `&mut self` across all CPU interface methods, ensuring thread-safe access when held behind synchronization primitives.
unsafe impl Sync for X86_64Cpu {}

impl X86_64Cpu {
    /// Create a new x86-64 CPU instance initialized to the default reset vector (0x000F_FFF0).
    pub fn new() -> Result<Self, MelError> {
        let mut uc = Unicorn::new(Arch::X86, Mode::MODE_64)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        let reset_pc = Architecture::X86_64.default_reset_vector();
        uc.reg_write(RegisterX86::RIP, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        Ok(Self {
            uc,
            breakpoints: HashSet::new(),
            cycles_executed: 0,
        })
    }

    /// Create a new x86-64 CPU instance connected to a system bus.
    pub fn with_bus(bus: Arc<Mutex<mel_bus::SystemBus>>) -> Result<Self, MelError> {
        let mut cpu = Self::new()?;
        cpu.attach_bus(bus)?;
        Ok(cpu)
    }

    /// Attach a system bus and register MMIO and Port I/O hooks.
    pub fn attach_bus(&mut self, bus: Arc<Mutex<mel_bus::SystemBus>>) -> Result<(), MelError> {
        crate::hook::register_bus_hooks(&mut self.uc, bus.clone())?;
        Self::register_port_io_hooks(&mut self.uc, bus)?;
        Ok(())
    }

    /// Register x86 Port I/O hooks (IN and OUT instructions) mapped to SystemBus.
    pub fn register_port_io_hooks(
        uc: &mut Unicorn<'static, ()>,
        bus: Arc<Mutex<mel_bus::SystemBus>>,
    ) -> Result<(), MelError> {
        // Port OUT hook (CPU -> Port)
        let bus_out = bus.clone();
        uc.add_insn_out_hook(move |_uc, port, size, val| {
            if let Ok(mut b) = bus_out.lock() {
                let _ = b.write(port as u64, val as u64, size, Endianness::Little);
            }
        })
        .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        // Port IN hook (Port -> CPU)
        let bus_in = bus;
        uc.add_insn_in_hook(move |_uc, port, size| -> u32 {
            if let Ok(b) = bus_in.lock() {
                if let Ok(val) = b.read(port as u64, size, Endianness::Little) {
                    return val as u32;
                }
            }
            0xFFFFFFFF
        })
        .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        Ok(())
    }

    /// Map a memory region into Unicorn's address space.
    pub fn mem_map(&mut self, addr: u64, size: usize, perms: Prot) -> Result<(), MelError> {
        self.uc
            .mem_map(addr, size as u64, perms)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Access the underlying Unicorn engine directly.
    pub fn inner_mut(&mut self) -> &mut Unicorn<'static, ()> {
        &mut self.uc
    }

    /// Add an execution breakpoint at `rip`.
    pub fn add_breakpoint(&mut self, rip: u64) {
        self.breakpoints.insert(rip);
    }

    /// Remove a breakpoint at `rip`.
    pub fn remove_breakpoint(&mut self, rip: u64) {
        self.breakpoints.remove(&rip);
    }

    /// Clear all breakpoints.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    /// Total cycles executed by this CPU instance.
    pub fn cycles_executed(&self) -> u64 {
        self.cycles_executed
    }

    /// Helper to map `RegisterId` to Unicorn `RegisterX86`.
    fn map_register(reg: &RegisterId) -> Result<RegisterX86, MelError> {
        let name = reg.name.to_lowercase();
        match name.as_str() {
            "rip" | "pc" => Ok(RegisterX86::RIP),
            "rsp" | "sp" => Ok(RegisterX86::RSP),
            "rbp" | "fp" => Ok(RegisterX86::RBP),
            "rflags" | "eflags" | "flags" => Ok(RegisterX86::EFLAGS),
            "rax" => Ok(RegisterX86::RAX),
            "rbx" => Ok(RegisterX86::RBX),
            "rcx" => Ok(RegisterX86::RCX),
            "rdx" => Ok(RegisterX86::RDX),
            "rsi" => Ok(RegisterX86::RSI),
            "rdi" => Ok(RegisterX86::RDI),
            "r8" => Ok(RegisterX86::R8),
            "r9" => Ok(RegisterX86::R9),
            "r10" => Ok(RegisterX86::R10),
            "r11" => Ok(RegisterX86::R11),
            "r12" => Ok(RegisterX86::R12),
            "r13" => Ok(RegisterX86::R13),
            "r14" => Ok(RegisterX86::R14),
            "r15" => Ok(RegisterX86::R15),

            // 32-bit sub-registers
            "eax" => Ok(RegisterX86::EAX),
            "ebx" => Ok(RegisterX86::EBX),
            "ecx" => Ok(RegisterX86::ECX),
            "edx" => Ok(RegisterX86::EDX),
            "esi" => Ok(RegisterX86::ESI),
            "edi" => Ok(RegisterX86::EDI),
            "esp" => Ok(RegisterX86::ESP),
            "ebp" => Ok(RegisterX86::EBP),

            other => Err(MelError::Cpu(CpuError::InvalidRegister(other.to_string()))),
        }
    }
}

impl CpuBackend for X86_64Cpu {
    fn arch(&self) -> Architecture {
        Architecture::X86_64
    }

    fn reset(&mut self) -> Result<(), MelError> {
        let reset_pc = Architecture::X86_64.default_reset_vector();
        self.uc
            .reg_write(RegisterX86::RIP, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RSP, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RBP, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::EFLAGS, 0x0002) // standard initial EFLAGS (bit 1 is always 1)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        let gprs = [
            RegisterX86::RAX,
            RegisterX86::RBX,
            RegisterX86::RCX,
            RegisterX86::RDX,
            RegisterX86::RSI,
            RegisterX86::RDI,
            RegisterX86::R8,
            RegisterX86::R9,
            RegisterX86::R10,
            RegisterX86::R11,
            RegisterX86::R12,
            RegisterX86::R13,
            RegisterX86::R14,
            RegisterX86::R15,
        ];

        for reg in gprs {
            self.uc
                .reg_write(reg, 0)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }

        self.cycles_executed = 0;
        Ok(())
    }

    fn step(&mut self) -> Result<CpuStepResult, MelError> {
        let current_rip = self.pc()?;

        if self.breakpoints.contains(&current_rip) {
            return Ok(CpuStepResult::Breakpoint { pc: current_rip });
        }

        // In x86_64 instructions are variable length (1-15 bytes). emu_start with count=1 steps 1 instruction.
        match self.uc.emu_start(current_rip, current_rip + 15, 0, 1) {
            Ok(()) => {
                self.cycles_executed += 1;
                let new_rip = self.pc()?;
                if self.breakpoints.contains(&new_rip) {
                    Ok(CpuStepResult::Breakpoint { pc: new_rip })
                } else {
                    Ok(CpuStepResult::Ok { cycles: 1 })
                }
            }
            Err(e) => {
                let rip = self.pc().unwrap_or(current_rip);
                Ok(CpuStepResult::Trap {
                    cause: e as u64,
                    epc: rip,
                })
            }
        }
    }

    fn run(&mut self, max_cycles: u64) -> Result<CpuStepResult, MelError> {
        let mut executed = 0;
        while executed < max_cycles {
            match self.step()? {
                CpuStepResult::Ok { cycles } => {
                    executed += cycles;
                }
                other => return Ok(other),
            }
        }
        Ok(CpuStepResult::Ok { cycles: executed })
    }

    fn read_register(&self, reg: RegisterId) -> Result<u64, MelError> {
        let uc_reg = Self::map_register(&reg)?;
        self.uc
            .reg_read(uc_reg)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn write_register(&mut self, reg: RegisterId, val: u64) -> Result<(), MelError> {
        let uc_reg = Self::map_register(&reg)?;
        self.uc
            .reg_write(uc_reg, val)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn pc(&self) -> Result<u64, MelError> {
        self.uc
            .reg_read(RegisterX86::RIP)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.uc
            .reg_write(RegisterX86::RIP, pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        let state = X86_64State {
            rip: self.pc()?,
            rsp: self
                .uc
                .reg_read(RegisterX86::RSP)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rbp: self
                .uc
                .reg_read(RegisterX86::RBP)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rflags: self
                .uc
                .reg_read(RegisterX86::EFLAGS)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rax: self
                .uc
                .reg_read(RegisterX86::RAX)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rbx: self
                .uc
                .reg_read(RegisterX86::RBX)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rcx: self
                .uc
                .reg_read(RegisterX86::RCX)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rdx: self
                .uc
                .reg_read(RegisterX86::RDX)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rsi: self
                .uc
                .reg_read(RegisterX86::RSI)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            rdi: self
                .uc
                .reg_read(RegisterX86::RDI)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r8: self
                .uc
                .reg_read(RegisterX86::R8)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r9: self
                .uc
                .reg_read(RegisterX86::R9)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r10: self
                .uc
                .reg_read(RegisterX86::R10)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r11: self
                .uc
                .reg_read(RegisterX86::R11)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r12: self
                .uc
                .reg_read(RegisterX86::R12)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r13: self
                .uc
                .reg_read(RegisterX86::R13)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r14: self
                .uc
                .reg_read(RegisterX86::R14)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            r15: self
                .uc
                .reg_read(RegisterX86::R15)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            cycles: self.cycles_executed,
        };

        serde_json::to_vec(&state).map_err(|e| MelError::Config(e.to_string()))
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let state: X86_64State =
            serde_json::from_slice(data).map_err(|e| MelError::Config(e.to_string()))?;

        self.set_pc(state.rip)?;
        self.uc
            .reg_write(RegisterX86::RSP, state.rsp)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RBP, state.rbp)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::EFLAGS, state.rflags)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RAX, state.rax)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RBX, state.rbx)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RCX, state.rcx)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RDX, state.rdx)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RSI, state.rsi)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::RDI, state.rdi)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R8, state.r8)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R9, state.r9)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R10, state.r10)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R11, state.r11)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R12, state.r12)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R13, state.r13)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R14, state.r14)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterX86::R15, state.r15)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        self.cycles_executed = state.cycles;
        Ok(())
    }

    fn mem_write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MelError> {
        let page_base = addr & !0xFFF;
        let page_size = (((bytes.len() + 0xFFF) & !0xFFF) as u64).max(0x1000);
        let _ = self.uc.mem_map(page_base, page_size, Prot::ALL);

        self.uc
            .mem_write(addr, bytes)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn mem_read(&self, addr: u64, bytes: &mut [u8]) -> Result<(), MelError> {
        self.uc
            .mem_read(addr, bytes)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x86_64_initial_state_and_registers() {
        let mut cpu = X86_64Cpu::new().expect("create x86-64 CPU");
        assert_eq!(cpu.arch(), Architecture::X86_64);
        assert_eq!(cpu.pc().unwrap(), 0x000F_FFF0);

        // Test writing and reading GPRs
        let gprs = [
            "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rsp", "rbp", "r8", "r9", "r10", "r11",
            "r12", "r13", "r14", "r15",
        ];
        for (i, name) in gprs.iter().enumerate() {
            cpu.write_register(RegisterId::named(*name), (i as u64) * 50 + 10)
                .expect("write reg");
            let val = cpu
                .read_register(RegisterId::named(*name))
                .expect("read reg");
            assert_eq!(val, (i as u64) * 50 + 10);
        }

        // Test RIP
        cpu.set_pc(0x10_0000).unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x10_0000);

        // Test reset
        cpu.reset().unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x000F_FFF0);
        assert_eq!(cpu.read_register(RegisterId::named("rax")).unwrap(), 0);
        assert_eq!(cpu.read_register(RegisterId::named("rsp")).unwrap(), 0);
    }

    #[test]
    fn test_x86_64_single_step_execution() {
        let mut cpu = X86_64Cpu::new().expect("create x86-64 CPU");

        // x86-64 instructions:
        // 1. mov rax, 42         -> 48 C7 C0 2A 00 00 00 (7 bytes)
        // 2. add rax, 10         -> 48 83 C0 0A          (4 bytes)
        // 3. nop                 -> 90                   (1 byte)
        let code: [u8; 12] = [
            0x48, 0xc7, 0xc0, 0x2a, 0x00, 0x00, 0x00, // mov rax, 42
            0x48, 0x83, 0xc0, 0x0a, // add rax, 10
            0x90, // nop
        ];

        let start_pc = 0x10_0000;
        cpu.mem_write(start_pc, &code).expect("write code");
        cpu.set_pc(start_pc).unwrap();

        // Step 1: mov rax, 42
        let res1 = cpu.step().expect("step 1");
        assert_eq!(res1, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("rax")).unwrap(), 42);
        assert_eq!(cpu.pc().unwrap(), start_pc + 7);

        // Step 2: add rax, 10
        let res2 = cpu.step().expect("step 2");
        assert_eq!(res2, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("rax")).unwrap(), 52);
        assert_eq!(cpu.pc().unwrap(), start_pc + 11);

        // Step 3: nop
        let res3 = cpu.step().expect("step 3");
        assert_eq!(res3, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.cycles_executed(), 3);
    }

    #[test]
    fn test_x86_64_breakpoints_and_run() {
        let mut cpu = X86_64Cpu::new().expect("create x86-64 CPU");

        let code: [u8; 12] = [
            0x48, 0xc7, 0xc0, 0x2a, 0x00, 0x00, 0x00, // mov rax, 42 (7 bytes)
            0x48, 0x83, 0xc0, 0x0a, // add rax, 10 (4 bytes)
            0x90, // nop         (1 byte)
        ];

        let start_pc = 0x10_0000;
        cpu.mem_write(start_pc, &code).expect("write code");
        cpu.set_pc(start_pc).unwrap();

        // Set breakpoint on second instruction (start_pc + 7)
        let bp_pc = start_pc + 7;
        cpu.add_breakpoint(bp_pc);

        let run_res = cpu.run(10).expect("run until breakpoint");
        assert_eq!(run_res, CpuStepResult::Breakpoint { pc: bp_pc });
        assert_eq!(cpu.pc().unwrap(), bp_pc);
        assert_eq!(cpu.read_register(RegisterId::named("rax")).unwrap(), 42);

        // Remove breakpoint and continue
        cpu.remove_breakpoint(bp_pc);
        let cont_res = cpu.run(2).expect("continue run");
        assert_eq!(cont_res, CpuStepResult::Ok { cycles: 2 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.read_register(RegisterId::named("rax")).unwrap(), 52);
    }

    #[test]
    fn test_x86_64_save_and_load_state() {
        let mut cpu1 = X86_64Cpu::new().expect("create CPU 1");
        cpu1.write_register(RegisterId::named("rax"), 0xDEAD_BEEF_CAFE_BABE)
            .unwrap();
        cpu1.write_register(RegisterId::named("rsp"), 0x7FFF_0000)
            .unwrap();
        cpu1.set_pc(0x10_0050).unwrap();

        let state_blob = cpu1.save_state().expect("save state");

        let mut cpu2 = X86_64Cpu::new().expect("create CPU 2");
        cpu2.load_state(&state_blob).expect("load state");

        assert_eq!(cpu2.pc().unwrap(), 0x10_0050);
        assert_eq!(
            cpu2.read_register(RegisterId::named("rax")).unwrap(),
            0xDEAD_BEEF_CAFE_BABE
        );
        assert_eq!(
            cpu2.read_register(RegisterId::named("rsp")).unwrap(),
            0x7FFF_0000
        );
    }

    #[test]
    fn test_x86_64_port_io_and_bus_integration() {
        use mel_bus::SystemBus;
        use mel_devices::{SharedMmioBusAdapter, Uart16550};
        use mel_memory::Ram;

        let mut bus = SystemBus::new();
        let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
        bus.map_ram("RAM", 0x10_0000, ram).unwrap();

        // Map UART at standard x86 COM1 I/O port 0x3F8
        let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
        let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
        bus.map_device("UART_COM1", 0x3F8, 8, uart_adapter).unwrap();

        let bus_arc = Arc::new(Mutex::new(bus));
        let mut cpu = X86_64Cpu::with_bus(bus_arc).unwrap();

        let start_pc = 0x10_0000;
        cpu.mem_map(start_pc, 0x10_000, Prot::ALL).unwrap();

        // x86-64 instructions:
        // mov dx, 0x3F8  -> 66 BA F8 03 (4 bytes)
        // mov al, 'X'    -> B0 58       (2 bytes) (0x58 = 88)
        // out dx, al     -> EE          (1 byte)
        let code: [u8; 7] = [
            0x66, 0xba, 0xf8, 0x03, // mov dx, 0x3F8
            0xb0, 0x58, // mov al, 'X'
            0xee, // out dx, al
        ];

        cpu.mem_write(start_pc, &code).unwrap();
        cpu.set_pc(start_pc).unwrap();

        let res = cpu.run(3).unwrap();
        assert_eq!(res, CpuStepResult::Ok { cycles: 3 });

        let output = uart.lock().unwrap().drain_output();
        assert_eq!(output, b"X");
    }
}
