//! Unicorn-based Power ISA 64-bit (PowerPC 64) CPU backend.

use crate::traits::{CpuBackend, CpuStepResult};
use mel_core::{Architecture, CpuError, MelError, RegisterId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use unicorn_engine::unicorn_const::*;
use unicorn_engine::{RegisterPPC, Unicorn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PowerIso64State {
    nip: u64,
    lr: u64,
    ctr: u64,
    cr: u64,
    msr: u64,
    gprs: [u64; 32],
    cycles: u64,
}

/// Power ISA (PowerPC 64) CPU backend powered by Unicorn Engine.
pub struct PowerIso64Cpu {
    uc: Unicorn<'static, ()>,
    breakpoints: HashSet<u64>,
    cycles_executed: u64,
}

// SAFETY: The underlying Unicorn emulator handle is exclusively owned by `PowerIso64Cpu` and not shared across threads without synchronization.
unsafe impl Send for PowerIso64Cpu {}
// SAFETY: Mutation requires `&mut self` across all CPU interface methods, ensuring thread-safe access when held behind synchronization primitives.
unsafe impl Sync for PowerIso64Cpu {}

impl PowerIso64Cpu {
    /// Create a new PowerPC CPU instance initialized to the default reset vector (0x0000_0100).
    pub fn new() -> Result<Self, MelError> {
        let mut uc = Unicorn::new(Arch::PPC, Mode::PPC32 | Mode::BIG_ENDIAN)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        let reset_pc = Architecture::PowerIso64.default_reset_vector();
        uc.reg_write(RegisterPPC::PC, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        Ok(Self {
            uc,
            breakpoints: HashSet::new(),
            cycles_executed: 0,
        })
    }

    /// Create a new PowerPC 64-bit CPU instance connected to a system bus.
    pub fn with_bus(bus: Arc<Mutex<mel_bus::SystemBus>>) -> Result<Self, MelError> {
        let mut cpu = Self::new()?;
        cpu.attach_bus(bus)?;
        Ok(cpu)
    }

    /// Attach a system bus and register MMIO memory hooks.
    pub fn attach_bus(&mut self, bus: Arc<Mutex<mel_bus::SystemBus>>) -> Result<(), MelError> {
        crate::hook::register_bus_hooks(&mut self.uc, bus)
    }

    /// Map a memory region into Unicorn's address space (automatically page-aligned).
    pub fn mem_map(&mut self, addr: u64, size: usize, perms: Prot) -> Result<(), MelError> {
        let page_base = addr & !0xFFF;
        let page_size = (((size + (addr as usize & 0xFFF) + 0xFFF) & !0xFFF) as u64).max(0x1000);
        self.uc
            .mem_map(page_base, page_size, perms)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Access the underlying Unicorn engine directly.
    pub fn inner_mut(&mut self) -> &mut Unicorn<'static, ()> {
        &mut self.uc
    }

    /// Add an execution breakpoint at `pc`.
    pub fn add_breakpoint(&mut self, pc: u64) {
        self.breakpoints.insert(pc);
    }

    /// Remove a breakpoint at `pc`.
    pub fn remove_breakpoint(&mut self, pc: u64) {
        self.breakpoints.remove(&pc);
    }

    /// Clear all breakpoints.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    /// Total cycles executed by this CPU instance.
    pub fn cycles_executed(&self) -> u64 {
        self.cycles_executed
    }

    /// Helper to map `RegisterId` to Unicorn `RegisterPPC`.
    fn map_register(reg: &RegisterId) -> Result<RegisterPPC, MelError> {
        let name = reg.name.to_lowercase();
        if let Some(idx) = reg.index {
            return Self::gpr_by_index(idx);
        }

        match name.as_str() {
            "pc" | "nip" => Ok(RegisterPPC::PC),
            "lr" => Ok(RegisterPPC::LR),
            "ctr" => Ok(RegisterPPC::CTR),
            "cr" => Ok(RegisterPPC::CR),
            "msr" => Ok(RegisterPPC::MSR),
            "xer" => Ok(RegisterPPC::XER),
            "sp" | "r1" => Ok(RegisterPPC::R1),
            s if s.starts_with('r') => {
                if let Ok(idx) = s[1..].parse::<u32>() {
                    Self::gpr_by_index(idx)
                } else {
                    Err(MelError::Cpu(CpuError::InvalidRegister(s.to_string())))
                }
            }
            s if s.starts_with('g') => {
                if let Ok(idx) = s[1..].parse::<u32>() {
                    Self::gpr_by_index(idx)
                } else {
                    Err(MelError::Cpu(CpuError::InvalidRegister(s.to_string())))
                }
            }
            other => Err(MelError::Cpu(CpuError::InvalidRegister(other.to_string()))),
        }
    }

    /// Map general-purpose register index (0..=31) to Unicorn `RegisterPPC`.
    fn gpr_by_index(idx: u32) -> Result<RegisterPPC, MelError> {
        match idx {
            0 => Ok(RegisterPPC::R0),
            1 => Ok(RegisterPPC::R1),
            2 => Ok(RegisterPPC::R2),
            3 => Ok(RegisterPPC::R3),
            4 => Ok(RegisterPPC::R4),
            5 => Ok(RegisterPPC::R5),
            6 => Ok(RegisterPPC::R6),
            7 => Ok(RegisterPPC::R7),
            8 => Ok(RegisterPPC::R8),
            9 => Ok(RegisterPPC::R9),
            10 => Ok(RegisterPPC::R10),
            11 => Ok(RegisterPPC::R11),
            12 => Ok(RegisterPPC::R12),
            13 => Ok(RegisterPPC::R13),
            14 => Ok(RegisterPPC::R14),
            15 => Ok(RegisterPPC::R15),
            16 => Ok(RegisterPPC::R16),
            17 => Ok(RegisterPPC::R17),
            18 => Ok(RegisterPPC::R18),
            19 => Ok(RegisterPPC::R19),
            20 => Ok(RegisterPPC::R20),
            21 => Ok(RegisterPPC::R21),
            22 => Ok(RegisterPPC::R22),
            23 => Ok(RegisterPPC::R23),
            24 => Ok(RegisterPPC::R24),
            25 => Ok(RegisterPPC::R25),
            26 => Ok(RegisterPPC::R26),
            27 => Ok(RegisterPPC::R27),
            28 => Ok(RegisterPPC::R28),
            29 => Ok(RegisterPPC::R29),
            30 => Ok(RegisterPPC::R30),
            31 => Ok(RegisterPPC::R31),
            other => Err(MelError::Cpu(CpuError::InvalidRegister(format!(
                "r{other}"
            )))),
        }
    }
}

impl CpuBackend for PowerIso64Cpu {
    fn arch(&self) -> Architecture {
        Architecture::PowerIso64
    }

    fn reset(&mut self) -> Result<(), MelError> {
        let reset_pc = Architecture::PowerIso64.default_reset_vector();
        self.uc
            .reg_write(RegisterPPC::PC, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::LR, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::CTR, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::CR, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::MSR, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        for i in 0..=31 {
            let reg = Self::gpr_by_index(i)?;
            self.uc
                .reg_write(reg, 0)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }

        self.cycles_executed = 0;
        Ok(())
    }

    fn step(&mut self) -> Result<CpuStepResult, MelError> {
        let current_pc = self.pc()?;

        if self.breakpoints.contains(&current_pc) {
            return Ok(CpuStepResult::Breakpoint { pc: current_pc });
        }

        match self.uc.emu_start(current_pc, current_pc + 4, 0, 1) {
            Ok(()) => {
                self.cycles_executed += 1;
                let new_pc = self.pc()?;
                if self.breakpoints.contains(&new_pc) {
                    Ok(CpuStepResult::Breakpoint { pc: new_pc })
                } else {
                    Ok(CpuStepResult::Ok { cycles: 1 })
                }
            }
            Err(e) => {
                let pc = self.pc().unwrap_or(current_pc);
                Ok(CpuStepResult::Trap {
                    cause: e as u64,
                    epc: pc,
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
            .reg_read(RegisterPPC::PC)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.uc
            .reg_write(RegisterPPC::PC, pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        let mut gprs = [0u64; 32];
        for i in 0..=31 {
            let reg = Self::gpr_by_index(i)?;
            gprs[i as usize] = self
                .uc
                .reg_read(reg)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }

        let state = PowerIso64State {
            nip: self.pc()?,
            lr: self
                .uc
                .reg_read(RegisterPPC::LR)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            ctr: self
                .uc
                .reg_read(RegisterPPC::CTR)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            cr: self
                .uc
                .reg_read(RegisterPPC::CR)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            msr: self
                .uc
                .reg_read(RegisterPPC::MSR)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            gprs,
            cycles: self.cycles_executed,
        };

        serde_json::to_vec(&state).map_err(|e| MelError::Config(e.to_string()))
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let state: PowerIso64State =
            serde_json::from_slice(data).map_err(|e| MelError::Config(e.to_string()))?;

        self.set_pc(state.nip)?;
        self.uc
            .reg_write(RegisterPPC::LR, state.lr)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::CTR, state.ctr)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::CR, state.cr)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterPPC::MSR, state.msr)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        for i in 0..=31 {
            let reg = Self::gpr_by_index(i)?;
            self.uc
                .reg_write(reg, state.gprs[i as usize])
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }

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
    fn test_powerpc64_initial_state_and_registers() {
        let mut cpu = PowerIso64Cpu::new().expect("create PowerPC 64 CPU");
        assert_eq!(cpu.arch(), Architecture::PowerIso64);
        assert_eq!(cpu.pc().unwrap(), 0x0000_0100);

        // Test writing and reading all 32 GPRs
        for i in 0..=31 {
            let reg_name = format!("r{i}");
            cpu.write_register(RegisterId::named(&reg_name), (i as u64) * 111 + 5)
                .expect("write reg");
            let val = cpu
                .read_register(RegisterId::named(&reg_name))
                .expect("read reg");
            assert_eq!(val, (i as u64) * 111 + 5);
        }

        // Test LR, CTR, CR, MSR
        cpu.write_register(RegisterId::named("lr"), 0x1234_5678)
            .unwrap();
        assert_eq!(
            cpu.read_register(RegisterId::named("lr")).unwrap(),
            0x1234_5678
        );

        cpu.set_pc(0x0000_1000).unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x0000_1000);

        // Test reset
        cpu.reset().unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x0000_0100);
        assert_eq!(cpu.read_register(RegisterId::named("r0")).unwrap(), 0);
        assert_eq!(cpu.read_register(RegisterId::named("lr")).unwrap(), 0);
    }

    #[test]
    fn test_powerpc64_single_step_execution() {
        let mut cpu = PowerIso64Cpu::new().expect("create PowerPC 64 CPU");

        // PowerPC 64 big-endian instructions:
        // 1. li r3, 42         (addi r3, 0, 42) -> 38 60 00 2A
        // 2. addi r4, r3, 10                    -> 38 83 00 0A
        // 3. nop               (ori 0, 0, 0)    -> 60 00 00 00
        let code: [u8; 12] = [
            0x38, 0x60, 0x00, 0x2a, // li r3, 42
            0x38, 0x83, 0x00, 0x0a, // addi r4, r3, 10
            0x60, 0x00, 0x00, 0x00, // nop
        ];

        let start_pc = 0x0000_0100;
        cpu.mem_write(start_pc, &code).expect("write code");
        cpu.set_pc(start_pc).unwrap();

        // Step 1: li r3, 42
        let res1 = cpu.step().expect("step 1");
        assert_eq!(res1, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("r3")).unwrap(), 42);
        assert_eq!(cpu.pc().unwrap(), start_pc + 4);

        // Step 2: addi r4, r3, 10
        let res2 = cpu.step().expect("step 2");
        assert_eq!(res2, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("r4")).unwrap(), 52);
        assert_eq!(cpu.pc().unwrap(), start_pc + 8);

        // Step 3: nop
        let res3 = cpu.step().expect("step 3");
        assert_eq!(res3, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.cycles_executed(), 3);
    }

    #[test]
    fn test_powerpc64_breakpoints_and_run() {
        let mut cpu = PowerIso64Cpu::new().expect("create PowerPC 64 CPU");

        let code: [u8; 12] = [
            0x38, 0x60, 0x00, 0x2a, // li r3, 42
            0x38, 0x83, 0x00, 0x0a, // addi r4, r3, 10
            0x60, 0x00, 0x00, 0x00, // nop
        ];

        let start_pc = 0x0000_0100;
        cpu.mem_write(start_pc, &code).expect("write code");
        cpu.set_pc(start_pc).unwrap();

        // Breakpoint at second instruction (start_pc + 4)
        let bp_pc = start_pc + 4;
        cpu.add_breakpoint(bp_pc);

        let run_res = cpu.run(10).expect("run until breakpoint");
        assert_eq!(run_res, CpuStepResult::Breakpoint { pc: bp_pc });
        assert_eq!(cpu.pc().unwrap(), bp_pc);
        assert_eq!(cpu.read_register(RegisterId::named("r3")).unwrap(), 42);

        // Remove breakpoint and continue remaining 2 instructions
        cpu.remove_breakpoint(bp_pc);
        let cont_res = cpu.run(2).expect("continue run");
        assert_eq!(cont_res, CpuStepResult::Ok { cycles: 2 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.read_register(RegisterId::named("r4")).unwrap(), 52);
    }

    #[test]
    fn test_powerpc64_save_and_load_state() {
        let mut cpu1 = PowerIso64Cpu::new().expect("create CPU 1");
        cpu1.write_register(RegisterId::named("r3"), 0x55AA_33CC)
            .unwrap();
        cpu1.write_register(RegisterId::named("lr"), 0x1122_3344)
            .unwrap();
        cpu1.set_pc(0x0000_0200).unwrap();

        let state_blob = cpu1.save_state().expect("save state");

        let mut cpu2 = PowerIso64Cpu::new().expect("create CPU 2");
        cpu2.load_state(&state_blob).expect("load state");

        assert_eq!(cpu2.pc().unwrap(), 0x0000_0200);
        assert_eq!(
            cpu2.read_register(RegisterId::named("r3")).unwrap(),
            0x55AA_33CC
        );
        assert_eq!(
            cpu2.read_register(RegisterId::named("lr")).unwrap(),
            0x1122_3344
        );
    }

    #[test]
    fn test_powerpc64_bus_bridge_mmio_write() {
        use mel_bus::SystemBus;
        use mel_devices::{SharedMmioBusAdapter, Uart16550};
        use mel_memory::Ram;

        let mut bus = SystemBus::new();
        let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
        bus.map_ram("RAM", 0x0000_0000, ram).unwrap();

        let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
        let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
        bus.map_device("UART", 0x8000_0000, 0x100, uart_adapter)
            .unwrap();

        let bus_arc = Arc::new(Mutex::new(bus));
        let mut cpu = PowerIso64Cpu::with_bus(bus_arc).unwrap();

        let start_pc = 0x0000_0100;
        cpu.mem_map(start_pc, 0x10_000, Prot::ALL).unwrap();

        // PowerPC instruction: stb r4, 0(r5) -> 98 85 00 00
        let code: [u8; 4] = [0x98, 0x85, 0x00, 0x00];
        cpu.mem_write(start_pc, &code).unwrap();

        cpu.write_register(RegisterId::named("r4"), 0x50).unwrap(); // 'P'
        cpu.write_register(RegisterId::named("r5"), 0x8000_0000)
            .unwrap();
        cpu.set_pc(start_pc).unwrap();

        let res = cpu.step().unwrap();
        assert_eq!(res, CpuStepResult::Ok { cycles: 1 });

        let output = uart.lock().unwrap().drain_output();
        assert_eq!(output, b"P");
    }
}
