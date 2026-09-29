//! Mock CPU implementation for testing backend abstraction and compliance.

use std::collections::{HashMap, HashSet};
use mel_core::{Architecture, CpuError, MelError, RegisterId};
use serde::{Deserialize, Serialize};
use crate::traits::{CpuBackend, CpuStepResult};

/// In-memory mock CPU backend for unit testing and validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockCpu {
    arch: Architecture,
    pc: u64,
    registers: HashMap<String, u64>,
    breakpoints: HashSet<u64>,
    cycles_executed: u64,
    halted: bool,
}

impl MockCpu {
    /// Create a new MockCpu initialized to architecture reset defaults.
    pub fn new(arch: Architecture) -> Self {
        Self {
            arch,
            pc: arch.default_reset_vector(),
            registers: HashMap::new(),
            breakpoints: HashSet::new(),
            cycles_executed: 0,
            halted: false,
        }
    }

    /// Register a breakpoint address.
    pub fn add_breakpoint(&mut self, pc: u64) {
        self.breakpoints.insert(pc);
    }

    /// Remove a breakpoint address.
    pub fn remove_breakpoint(&mut self, pc: u64) {
        self.breakpoints.remove(&pc);
    }

    /// Total cycles executed since creation or last reset.
    pub fn cycles_executed(&self) -> u64 {
        self.cycles_executed
    }
}

impl CpuBackend for MockCpu {
    fn arch(&self) -> Architecture {
        self.arch
    }

    fn reset(&mut self) -> Result<(), MelError> {
        self.pc = self.arch.default_reset_vector();
        self.registers.clear();
        self.cycles_executed = 0;
        self.halted = false;
        Ok(())
    }

    fn step(&mut self) -> Result<CpuStepResult, MelError> {
        if self.halted {
            return Ok(CpuStepResult::Halted {
                reason: "CPU halted".to_string(),
            });
        }

        if self.breakpoints.contains(&self.pc) {
            return Ok(CpuStepResult::Breakpoint { pc: self.pc });
        }

        self.pc = self.pc.wrapping_add(4);
        self.cycles_executed = self.cycles_executed.wrapping_add(1);

        Ok(CpuStepResult::Ok { cycles: 1 })
    }

    fn run(&mut self, max_cycles: u64) -> Result<CpuStepResult, MelError> {
        let mut executed = 0;
        while executed < max_cycles {
            if self.breakpoints.contains(&self.pc) {
                return Ok(CpuStepResult::Breakpoint { pc: self.pc });
            }

            if self.halted {
                return Ok(CpuStepResult::Halted {
                    reason: "CPU halted".to_string(),
                });
            }

            self.pc = self.pc.wrapping_add(4);
            self.cycles_executed = self.cycles_executed.wrapping_add(1);
            executed += 1;
        }

        Ok(CpuStepResult::Ok { cycles: executed })
    }

    fn read_register(&self, reg: RegisterId) -> Result<u64, MelError> {
        self.registers
            .get(&reg.name)
            .copied()
            .ok_or(MelError::Cpu(CpuError::InvalidRegister(reg.name)))
    }

    fn write_register(&mut self, reg: RegisterId, val: u64) -> Result<(), MelError> {
        self.registers.insert(reg.name, val);
        Ok(())
    }

    fn pc(&self) -> Result<u64, MelError> {
        Ok(self.pc)
    }

    fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.pc = pc;
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        serde_json::to_vec(self).map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let restored: Self = serde_json::from_slice(data)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_cpu_lifecycle() {
        let mut cpu = MockCpu::new(Architecture::Riscv64);
        assert_eq!(cpu.arch(), Architecture::Riscv64);
        assert_eq!(cpu.pc().unwrap(), 0x8000_0000);

        let step_res = cpu.step().unwrap();
        assert_eq!(step_res, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.pc().unwrap(), 0x8000_0004);
        assert_eq!(cpu.cycles_executed(), 1);

        cpu.reset().unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x8000_0000);
        assert_eq!(cpu.cycles_executed(), 0);
    }

    #[test]
    fn test_mock_cpu_registers() {
        let mut cpu = MockCpu::new(Architecture::Arm64);
        let x0 = RegisterId::named("x0");

        assert!(cpu.read_register(x0.clone()).is_err());
        cpu.write_register(x0.clone(), 0xCAFEBABE).unwrap();
        assert_eq!(cpu.read_register(x0).unwrap(), 0xCAFEBABE);
    }

    #[test]
    fn test_mock_cpu_breakpoints() {
        let mut cpu = MockCpu::new(Architecture::X86_64);
        let bp_addr = cpu.pc().unwrap() + 8;
        cpu.add_breakpoint(bp_addr);

        let run_res = cpu.run(10).unwrap();
        assert_eq!(run_res, CpuStepResult::Breakpoint { pc: bp_addr });
        assert_eq!(cpu.pc().unwrap(), bp_addr);
    }

    #[test]
    fn test_mock_cpu_save_restore() {
        let mut cpu = MockCpu::new(Architecture::Riscv64);
        cpu.step().unwrap();
        cpu.write_register(RegisterId::named("t0"), 0x1234).unwrap();

        let snapshot = cpu.save_state().unwrap();

        let mut restored = MockCpu::new(Architecture::Riscv64);
        restored.load_state(&snapshot).unwrap();

        assert_eq!(restored.pc().unwrap(), 0x8000_0004);
        assert_eq!(restored.read_register(RegisterId::named("t0")).unwrap(), 0x1234);
    }
}
