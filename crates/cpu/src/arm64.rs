//! Unicorn-based ARM64 (AArch64) CPU backend.

use crate::traits::{CpuBackend, CpuStepResult};
use mel_core::{Architecture, CpuError, MelError, RegisterId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use unicorn_engine::unicorn_const::*;
use unicorn_engine::{RegisterARM64, Unicorn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Arm64State {
    pc: u64,
    sp: u64,
    pstate: u64,
    gprs: [u64; 31],
    cycles: u64,
}

/// ARM64 (AArch64) CPU backend powered by Unicorn Engine.
pub struct Arm64Cpu {
    uc: Unicorn<'static, ()>,
    breakpoints: HashSet<u64>,
    cycles_executed: u64,
}

// SAFETY: The underlying Unicorn emulator handle is exclusively owned by `Arm64Cpu` and not shared across threads without synchronization.
unsafe impl Send for Arm64Cpu {}
// SAFETY: Mutation requires `&mut self` across all CPU interface methods, ensuring thread-safe access when held behind synchronization primitives.
unsafe impl Sync for Arm64Cpu {}

impl Arm64Cpu {
    /// Create a new ARM64 CPU instance initialized to the default reset vector (0x4000_0000).
    pub fn new() -> Result<Self, MelError> {
        let mut uc = Unicorn::new(Arch::ARM64, Mode::ARM)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        let reset_pc = Architecture::Arm64.default_reset_vector();
        uc.reg_write(RegisterARM64::PC, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        Ok(Self {
            uc,
            breakpoints: HashSet::new(),
            cycles_executed: 0,
        })
    }

    /// Create a new ARM64 CPU instance connected to a system bus.
    pub fn with_bus(
        bus: std::sync::Arc<std::sync::Mutex<mel_bus::SystemBus>>,
    ) -> Result<Self, MelError> {
        let mut cpu = Self::new()?;
        cpu.attach_bus(bus)?;
        Ok(cpu)
    }

    /// Attach a system bus and register MMIO memory hooks.
    pub fn attach_bus(
        &mut self,
        bus: std::sync::Arc<std::sync::Mutex<mel_bus::SystemBus>>,
    ) -> Result<(), MelError> {
        crate::hook::register_bus_hooks(&mut self.uc, bus)
    }

    /// Map a memory region into Unicorn's address space.
    pub fn mem_map(&mut self, addr: u64, size: usize, perms: Prot) -> Result<(), MelError> {
        self.uc
            .mem_map(addr, size as u64, perms)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Access the underlying Unicorn engine directly (e.g. for hook registration).
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

    /// Helper to map `RegisterId` to Unicorn `RegisterARM64`.
    fn map_register(reg: &RegisterId) -> Result<RegisterARM64, MelError> {
        let name = reg.name.to_lowercase();
        if let Some(idx) = reg.index {
            return Self::gpr_by_index(idx);
        }

        match name.as_str() {
            "pc" => Ok(RegisterARM64::PC),
            "sp" => Ok(RegisterARM64::SP),
            "lr" | "x30" => Ok(RegisterARM64::X30),
            "fp" | "x29" => Ok(RegisterARM64::X29),
            "nzcv" | "pstate" | "cpsr" => Ok(RegisterARM64::NZCV),
            s if s.starts_with('x') => {
                if let Ok(idx) = s[1..].parse::<u32>() {
                    Self::gpr_by_index(idx)
                } else {
                    Err(MelError::Cpu(CpuError::InvalidRegister(s.to_string())))
                }
            }
            s if s.starts_with('w') => {
                if let Ok(idx) = s[1..].parse::<u32>() {
                    Self::gpr_by_index(idx)
                } else {
                    Err(MelError::Cpu(CpuError::InvalidRegister(s.to_string())))
                }
            }
            other => Err(MelError::Cpu(CpuError::InvalidRegister(other.to_string()))),
        }
    }

    /// Map general-purpose register index (0..=30) to Unicorn `RegisterARM64`.
    fn gpr_by_index(idx: u32) -> Result<RegisterARM64, MelError> {
        match idx {
            0 => Ok(RegisterARM64::X0),
            1 => Ok(RegisterARM64::X1),
            2 => Ok(RegisterARM64::X2),
            3 => Ok(RegisterARM64::X3),
            4 => Ok(RegisterARM64::X4),
            5 => Ok(RegisterARM64::X5),
            6 => Ok(RegisterARM64::X6),
            7 => Ok(RegisterARM64::X7),
            8 => Ok(RegisterARM64::X8),
            9 => Ok(RegisterARM64::X9),
            10 => Ok(RegisterARM64::X10),
            11 => Ok(RegisterARM64::X11),
            12 => Ok(RegisterARM64::X12),
            13 => Ok(RegisterARM64::X13),
            14 => Ok(RegisterARM64::X14),
            15 => Ok(RegisterARM64::X15),
            16 => Ok(RegisterARM64::X16),
            17 => Ok(RegisterARM64::X17),
            18 => Ok(RegisterARM64::X18),
            19 => Ok(RegisterARM64::X19),
            20 => Ok(RegisterARM64::X20),
            21 => Ok(RegisterARM64::X21),
            22 => Ok(RegisterARM64::X22),
            23 => Ok(RegisterARM64::X23),
            24 => Ok(RegisterARM64::X24),
            25 => Ok(RegisterARM64::X25),
            26 => Ok(RegisterARM64::X26),
            27 => Ok(RegisterARM64::X27),
            28 => Ok(RegisterARM64::X28),
            29 => Ok(RegisterARM64::X29),
            30 => Ok(RegisterARM64::X30),
            other => Err(MelError::Cpu(CpuError::InvalidRegister(format!(
                "x{other}"
            )))),
        }
    }
}

impl CpuBackend for Arm64Cpu {
    fn arch(&self) -> Architecture {
        Architecture::Arm64
    }

    fn reset(&mut self) -> Result<(), MelError> {
        let reset_pc = Architecture::Arm64.default_reset_vector();
        self.uc
            .reg_write(RegisterARM64::PC, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterARM64::SP, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterARM64::NZCV, 0)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        for i in 0..=30 {
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
            .reg_read(RegisterARM64::PC)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.uc
            .reg_write(RegisterARM64::PC, pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        let mut gprs = [0u64; 31];
        for i in 0..=30 {
            let reg = Self::gpr_by_index(i)?;
            gprs[i as usize] = self
                .uc
                .reg_read(reg)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }

        let state = Arm64State {
            pc: self.pc()?,
            sp: self
                .uc
                .reg_read(RegisterARM64::SP)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            pstate: self
                .uc
                .reg_read(RegisterARM64::NZCV)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?,
            gprs,
            cycles: self.cycles_executed,
        };

        serde_json::to_vec(&state).map_err(|e| MelError::Config(e.to_string()))
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let state: Arm64State =
            serde_json::from_slice(data).map_err(|e| MelError::Config(e.to_string()))?;

        self.set_pc(state.pc)?;
        self.uc
            .reg_write(RegisterARM64::SP, state.sp)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.uc
            .reg_write(RegisterARM64::NZCV, state.pstate)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;

        for i in 0..=30 {
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
    fn test_arm64_initial_state_and_registers() {
        let mut cpu = Arm64Cpu::new().expect("create ARM64 CPU");
        assert_eq!(cpu.arch(), Architecture::Arm64);
        assert_eq!(cpu.pc().unwrap(), 0x4000_0000);

        // Test writing and reading all 31 GPRs
        for i in 0..=30 {
            let reg_name = format!("x{i}");
            cpu.write_register(RegisterId::named(&reg_name), (i as u64) * 100 + 7)
                .expect("write reg");
            let val = cpu
                .read_register(RegisterId::named(&reg_name))
                .expect("read reg");
            assert_eq!(val, (i as u64) * 100 + 7);
        }

        // Test SP and PC
        cpu.write_register(RegisterId::named("sp"), 0x4008_0000)
            .unwrap();
        assert_eq!(
            cpu.read_register(RegisterId::named("sp")).unwrap(),
            0x4008_0000
        );

        cpu.set_pc(0x4000_1000).unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x4000_1000);

        // Test reset
        cpu.reset().unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x4000_0000);
        assert_eq!(cpu.read_register(RegisterId::named("x0")).unwrap(), 0);
        assert_eq!(cpu.read_register(RegisterId::named("sp")).unwrap(), 0);
    }

    #[test]
    fn test_arm64_single_step_execution() {
        let mut cpu = Arm64Cpu::new().expect("create ARM64 CPU");

        // ARM64 instructions:
        // 1. mov x0, #42     -> 0xd2800540
        // 2. add x1, x0, #10 -> 0x11002801
        // 3. nop             -> 0xd503201f
        let code: [u8; 12] = [
            0x40, 0x05, 0x80, 0xd2, // mov x0, #42
            0x01, 0x28, 0x00, 0x11, // add x1, x0, #10
            0x1f, 0x20, 0x03, 0xd5, // nop
        ];

        let start_pc = 0x4000_0000;
        cpu.mem_write(start_pc, &code).expect("write instructions");

        // Step 1: mov x0, #42
        let res1 = cpu.step().expect("step 1");
        assert_eq!(res1, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("x0")).unwrap(), 42);
        assert_eq!(cpu.pc().unwrap(), start_pc + 4);

        // Step 2: add x1, x0, #10
        let res2 = cpu.step().expect("step 2");
        assert_eq!(res2, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("x1")).unwrap(), 52);
        assert_eq!(cpu.pc().unwrap(), start_pc + 8);

        // Step 3: nop
        let res3 = cpu.step().expect("step 3");
        assert_eq!(res3, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.cycles_executed(), 3);
    }

    #[test]
    fn test_arm64_breakpoints_and_run() {
        let mut cpu = Arm64Cpu::new().expect("create ARM64 CPU");

        let code: [u8; 12] = [
            0x40, 0x05, 0x80, 0xd2, // mov x0, #42
            0x01, 0x28, 0x00, 0x11, // add x1, x0, #10
            0x1f, 0x20, 0x03, 0xd5, // nop
        ];

        let start_pc = 0x4000_0000;
        cpu.mem_write(start_pc, &code).expect("write code");

        // Set breakpoint on second instruction
        let bp_pc = start_pc + 4;
        cpu.add_breakpoint(bp_pc);

        let run_res = cpu.run(10).expect("run until breakpoint");
        assert_eq!(run_res, CpuStepResult::Breakpoint { pc: bp_pc });
        assert_eq!(cpu.pc().unwrap(), bp_pc);
        assert_eq!(cpu.read_register(RegisterId::named("x0")).unwrap(), 42);

        // Remove breakpoint and continue
        cpu.remove_breakpoint(bp_pc);
        let cont_res = cpu.run(2).expect("continue run");
        assert_eq!(cont_res, CpuStepResult::Ok { cycles: 2 });
        assert_eq!(cpu.pc().unwrap(), start_pc + 12);
        assert_eq!(cpu.read_register(RegisterId::named("x1")).unwrap(), 52);
    }

    #[test]
    fn test_arm64_save_and_load_state() {
        let mut cpu1 = Arm64Cpu::new().expect("create CPU 1");
        cpu1.write_register(RegisterId::named("x0"), 0x1234_5678)
            .unwrap();
        cpu1.write_register(RegisterId::named("x30"), 0xDEAD_BEEF)
            .unwrap();
        cpu1.write_register(RegisterId::named("sp"), 0x4001_0000)
            .unwrap();
        cpu1.set_pc(0x4000_0080).unwrap();

        let state_blob = cpu1.save_state().expect("save state");

        let mut cpu2 = Arm64Cpu::new().expect("create CPU 2");
        cpu2.load_state(&state_blob).expect("load state");

        assert_eq!(cpu2.pc().unwrap(), 0x4000_0080);
        assert_eq!(
            cpu2.read_register(RegisterId::named("x0")).unwrap(),
            0x1234_5678
        );
        assert_eq!(
            cpu2.read_register(RegisterId::named("x30")).unwrap(),
            0xDEAD_BEEF
        );
        assert_eq!(
            cpu2.read_register(RegisterId::named("sp")).unwrap(),
            0x4001_0000
        );
    }

    #[test]
    fn test_arm64_bus_bridge_mmio_write() {
        use mel_bus::SystemBus;
        use mel_devices::{SharedMmioBusAdapter, Uart16550};
        use mel_memory::Ram;
        use std::sync::{Arc, Mutex};

        let mut bus = SystemBus::new();
        let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
        bus.map_ram("RAM", 0x4000_0000, ram).unwrap();

        let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
        let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
        bus.map_device("UART", 0x0900_0000, 0x100, uart_adapter)
            .unwrap();

        let bus_arc = Arc::new(Mutex::new(bus));
        let mut cpu = Arm64Cpu::with_bus(bus_arc).unwrap();

        let start_pc = 0x4000_0000;
        cpu.mem_map(start_pc, 0x10_000, Prot::ALL).unwrap();

        // ARM64 instructions:
        // Set x0 = 0x0900_0000 (UART THR MMIO address)
        // Set w1 = 0x41 ('A')
        // Instruction: strb w1, [x0] -> 0x39000001
        let code: [u8; 4] = [0x01, 0x00, 0x00, 0x39];
        cpu.mem_write(start_pc, &code).unwrap();

        cpu.write_register(RegisterId::named("x0"), 0x0900_0000)
            .unwrap();
        cpu.write_register(RegisterId::named("x1"), 0x41).unwrap();
        cpu.set_pc(start_pc).unwrap();

        let res = cpu.step().unwrap();
        assert_eq!(res, CpuStepResult::Ok { cycles: 1 });

        let output = uart.lock().unwrap().drain_output();
        assert_eq!(output, b"A");
    }
}
