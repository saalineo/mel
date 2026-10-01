//! Unicorn-based RISC-V 64-bit CPU backend with full interrupt and trap handling.

use crate::traits::{CpuBackend, CpuStepResult};
use mel_core::{Architecture, CpuError, MelError, RegisterId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use unicorn_engine::unicorn_const::*;
use unicorn_engine::{RegisterRISCV, Unicorn};

/// Machine Status Register: Machine Interrupt Enable bit (bit 3).
pub const MSTATUS_MIE: u64 = 1 << 3;

/// Machine Status Register: Machine Previous Interrupt Enable bit (bit 7).
pub const MSTATUS_MPIE: u64 = 1 << 7;

/// Machine Status Register: Machine Previous Privilege Mode mask (bits 11..12).
pub const MSTATUS_MPP_MASK: u64 = 3 << 11;

/// Machine Status Register: Machine Previous Privilege Mode = Machine Mode (3 << 11).
pub const MSTATUS_MPP_M: u64 = 3 << 11;

/// Machine Interrupt Enable / Pending: Software Interrupt (bit 3).
pub const MIE_MSIE: u64 = 1 << 3;
pub const MIP_MSIP: u64 = 1 << 3;

/// Machine Interrupt Enable / Pending: Timer Interrupt (bit 7).
pub const MIE_MTIE: u64 = 1 << 7;
pub const MIP_MTIP: u64 = 1 << 7;

/// Machine Interrupt Enable / Pending: External Interrupt (bit 11).
pub const MIE_MEIE: u64 = 1 << 11;
pub const MIP_MEIP: u64 = 1 << 11;

/// Machine Cause: Interrupt Bit (MSB, bit 63).
pub const MCAUSE_INTERRUPT_BIT: u64 = 1 << 63;

/// Machine Cause: Machine Software Interrupt exception code (3).
pub const MCAUSE_MSI: u64 = 3;

/// Machine Cause: Machine Timer Interrupt exception code (7).
pub const MCAUSE_MTI: u64 = 7;

/// Machine Cause: Machine External Interrupt exception code (11).
pub const MCAUSE_MEI: u64 = 11;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Riscv64State {
    pc: u64,
    gprs: [u64; 32],
    cycles: u64,
    mstatus: u64,
    mie: u64,
    mip: u64,
    mtvec: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
}

/// RISC-V 64-bit CPU backend powered by the Unicorn Engine with CSR interrupt delivery.
pub struct Riscv64Cpu {
    uc: Unicorn<'static, ()>,
    breakpoints: HashSet<u64>,
    cycles_executed: u64,
    mstatus: u64,
    mie: u64,
    mip: u64,
    mtvec: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
}

unsafe impl Send for Riscv64Cpu {}
unsafe impl Sync for Riscv64Cpu {}

impl Riscv64Cpu {
    /// Create a new RISC-V 64-bit CPU instance initialized to the default reset vector.
    pub fn new() -> Result<Self, MelError> {
        let mut uc = Unicorn::new(Arch::RISCV, Mode::RISCV64)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        let reset_pc = Architecture::Riscv64.default_reset_vector();
        uc.reg_write(RegisterRISCV::PC, reset_pc)
            .map_err(|e| MelError::Cpu(CpuError::InitFailed(e.to_string())))?;

        Ok(Self {
            uc,
            breakpoints: HashSet::new(),
            cycles_executed: 0,
            mstatus: MSTATUS_MPP_M,
            mie: 0,
            mip: 0,
            mtvec: reset_pc,
            mepc: 0,
            mcause: 0,
            mtval: 0,
        })
    }

    /// Create a new RISC-V 64-bit CPU instance connected to a system bus.
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

    /// Map a memory region directly into the Unicorn emulator.
    pub fn mem_map(&mut self, addr: u64, size: u64, perms: Prot) -> Result<(), MelError> {
        self.uc
            .mem_map(addr, size, perms)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Write raw bytes into Unicorn memory.
    pub fn mem_write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MelError> {
        self.uc
            .mem_write(addr, bytes)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Read raw bytes from Unicorn memory.
    pub fn mem_read(&self, addr: u64, bytes: &mut [u8]) -> Result<(), MelError> {
        self.uc
            .mem_read(addr, bytes)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    /// Add a breakpoint address.
    pub fn add_breakpoint(&mut self, pc: u64) {
        self.breakpoints.insert(pc);
    }

    /// Remove a breakpoint address.
    pub fn remove_breakpoint(&mut self, pc: u64) {
        self.breakpoints.remove(&pc);
    }

    /// Total execution cycles counted on this core.
    pub fn cycles_executed(&self) -> u64 {
        self.cycles_executed
    }

    /// Access the underlying Unicorn engine reference.
    pub fn unicorn(&self) -> &Unicorn<'static, ()> {
        &self.uc
    }

    /// Access the underlying Unicorn engine mutable reference.
    pub fn unicorn_mut(&mut self) -> &mut Unicorn<'static, ()> {
        &mut self.uc
    }

    /// Read `mstatus` CSR.
    pub fn mstatus(&self) -> u64 {
        self.mstatus
    }

    /// Write `mstatus` CSR.
    pub fn set_mstatus(&mut self, val: u64) {
        self.mstatus = val;
    }

    /// Read `mie` CSR.
    pub fn mie(&self) -> u64 {
        self.mie
    }

    /// Write `mie` CSR.
    pub fn set_mie(&mut self, val: u64) {
        self.mie = val;
    }

    /// Read `mip` CSR.
    pub fn mip(&self) -> u64 {
        self.mip
    }

    /// Write `mip` CSR.
    pub fn set_mip(&mut self, val: u64) {
        self.mip = val;
    }

    /// Read `mtvec` CSR.
    pub fn mtvec(&self) -> u64 {
        self.mtvec
    }

    /// Write `mtvec` CSR.
    pub fn set_mtvec(&mut self, val: u64) {
        self.mtvec = val;
    }

    /// Read `mepc` CSR.
    pub fn mepc(&self) -> u64 {
        self.mepc
    }

    /// Write `mepc` CSR.
    pub fn set_mepc(&mut self, val: u64) {
        self.mepc = val;
    }

    /// Read `mcause` CSR.
    pub fn mcause(&self) -> u64 {
        self.mcause
    }

    /// Write `mcause` CSR.
    pub fn set_mcause(&mut self, val: u64) {
        self.mcause = val;
    }

    /// Read `mtval` CSR.
    pub fn mtval(&self) -> u64 {
        self.mtval
    }

    /// Write `mtval` CSR.
    pub fn set_mtval(&mut self, val: u64) {
        self.mtval = val;
    }

    /// Set or clear timer interrupt pending (MTIP).
    pub fn set_timer_interrupt(&mut self, pending: bool) {
        if pending {
            self.mip |= MIP_MTIP;
        } else {
            self.mip &= !MIP_MTIP;
        }
    }

    /// Set or clear software interrupt pending (MSIP).
    pub fn set_software_interrupt(&mut self, pending: bool) {
        if pending {
            self.mip |= MIP_MSIP;
        } else {
            self.mip &= !MIP_MSIP;
        }
    }

    /// Set or clear external interrupt pending (MEIP).
    pub fn set_external_interrupt(&mut self, pending: bool) {
        if pending {
            self.mip |= MIP_MEIP;
        } else {
            self.mip &= !MIP_MEIP;
        }
    }

    /// Check if any enabled interrupt is pending delivery.
    /// RISC-V priority: MEI (11) > MSI (3) > MTI (7).
    pub fn check_pending_interrupt(&self) -> Option<u64> {
        if (self.mstatus & MSTATUS_MIE) == 0 {
            return None;
        }
        let active = self.mip & self.mie;
        if (active & MIP_MEIP) != 0 {
            Some(MCAUSE_MEI)
        } else if (active & MIP_MSIP) != 0 {
            Some(MCAUSE_MSI)
        } else if (active & MIP_MTIP) != 0 {
            Some(MCAUSE_MTI)
        } else {
            None
        }
    }

    /// Deliver an asynchronous trap for the given cause code.
    pub fn deliver_trap(&mut self, cause_code: u64) -> Result<CpuStepResult, MelError> {
        let current_pc = self.pc()?;
        self.mepc = current_pc;
        self.mcause = MCAUSE_INTERRUPT_BIT | cause_code;
        self.mtval = 0;

        // Transition mstatus: MPIE <= MIE, MIE <= 0, MPP <= 3 (Machine mode)
        let mstatus_mie = (self.mstatus & MSTATUS_MIE) != 0;
        let mut new_mstatus = self.mstatus & !MSTATUS_MIE;
        if mstatus_mie {
            new_mstatus |= MSTATUS_MPIE;
        } else {
            new_mstatus &= !MSTATUS_MPIE;
        }
        new_mstatus = (new_mstatus & !MSTATUS_MPP_MASK) | MSTATUS_MPP_M;
        self.mstatus = new_mstatus;

        // Determine target vector from mtvec (Direct mode 0 vs Vectored mode 1)
        let mode = self.mtvec & 0x3;
        let base = self.mtvec & !0x3;
        let target_pc = if mode == 1 {
            base.wrapping_add(cause_code.wrapping_mul(4))
        } else {
            base
        };

        self.set_pc(target_pc)?;
        self.cycles_executed = self.cycles_executed.wrapping_add(1);

        Ok(CpuStepResult::Trap {
            cause: self.mcause,
            epc: current_pc,
        })
    }

    /// Return from Machine Mode trap (`mret`).
    pub fn mret(&mut self) -> Result<(), MelError> {
        let mpie = (self.mstatus & MSTATUS_MPIE) != 0;
        if mpie {
            self.mstatus |= MSTATUS_MIE;
        } else {
            self.mstatus &= !MSTATUS_MIE;
        }
        self.mstatus |= MSTATUS_MPIE;
        self.set_pc(self.mepc)?;
        Ok(())
    }
}

fn resolve_register(reg: &RegisterId) -> Result<RegisterRISCV, MelError> {
    if let Some(idx) = reg.index {
        return register_from_index(idx);
    }

    let name = reg.name.to_lowercase();
    match name.as_str() {
        "pc" => Ok(RegisterRISCV::PC),
        "x0" | "zero" => Ok(RegisterRISCV::X0),
        "x1" | "ra" => Ok(RegisterRISCV::X1),
        "x2" | "sp" => Ok(RegisterRISCV::X2),
        "x3" | "gp" => Ok(RegisterRISCV::X3),
        "x4" | "tp" => Ok(RegisterRISCV::X4),
        "x5" | "t0" => Ok(RegisterRISCV::X5),
        "x6" | "t1" => Ok(RegisterRISCV::X6),
        "x7" | "t2" => Ok(RegisterRISCV::X7),
        "x8" | "s0" | "fp" => Ok(RegisterRISCV::X8),
        "x9" | "s1" => Ok(RegisterRISCV::X9),
        "x10" | "a0" => Ok(RegisterRISCV::X10),
        "x11" | "a1" => Ok(RegisterRISCV::X11),
        "x12" | "a2" => Ok(RegisterRISCV::X12),
        "x13" | "a3" => Ok(RegisterRISCV::X13),
        "x14" | "a4" => Ok(RegisterRISCV::X14),
        "x15" | "a5" => Ok(RegisterRISCV::X15),
        "x16" | "a6" => Ok(RegisterRISCV::X16),
        "x17" | "a7" => Ok(RegisterRISCV::X17),
        "x18" | "s2" => Ok(RegisterRISCV::X18),
        "x19" | "s3" => Ok(RegisterRISCV::X19),
        "x20" | "s4" => Ok(RegisterRISCV::X20),
        "x21" | "s5" => Ok(RegisterRISCV::X21),
        "x22" | "s6" => Ok(RegisterRISCV::X22),
        "x23" | "s7" => Ok(RegisterRISCV::X23),
        "x24" | "s8" => Ok(RegisterRISCV::X24),
        "x25" | "s9" => Ok(RegisterRISCV::X25),
        "x26" | "s10" => Ok(RegisterRISCV::X26),
        "x27" | "s11" => Ok(RegisterRISCV::X27),
        "x28" | "t3" => Ok(RegisterRISCV::X28),
        "x29" | "t4" => Ok(RegisterRISCV::X29),
        "x30" | "t5" => Ok(RegisterRISCV::X30),
        "x31" | "t6" => Ok(RegisterRISCV::X31),
        _ => Err(MelError::Cpu(CpuError::InvalidRegister(reg.name.clone()))),
    }
}

fn register_from_index(idx: u32) -> Result<RegisterRISCV, MelError> {
    match idx {
        0 => Ok(RegisterRISCV::X0),
        1 => Ok(RegisterRISCV::X1),
        2 => Ok(RegisterRISCV::X2),
        3 => Ok(RegisterRISCV::X3),
        4 => Ok(RegisterRISCV::X4),
        5 => Ok(RegisterRISCV::X5),
        6 => Ok(RegisterRISCV::X6),
        7 => Ok(RegisterRISCV::X7),
        8 => Ok(RegisterRISCV::X8),
        9 => Ok(RegisterRISCV::X9),
        10 => Ok(RegisterRISCV::X10),
        11 => Ok(RegisterRISCV::X11),
        12 => Ok(RegisterRISCV::X12),
        13 => Ok(RegisterRISCV::X13),
        14 => Ok(RegisterRISCV::X14),
        15 => Ok(RegisterRISCV::X15),
        16 => Ok(RegisterRISCV::X16),
        17 => Ok(RegisterRISCV::X17),
        18 => Ok(RegisterRISCV::X18),
        19 => Ok(RegisterRISCV::X19),
        20 => Ok(RegisterRISCV::X20),
        21 => Ok(RegisterRISCV::X21),
        22 => Ok(RegisterRISCV::X22),
        23 => Ok(RegisterRISCV::X23),
        24 => Ok(RegisterRISCV::X24),
        25 => Ok(RegisterRISCV::X25),
        26 => Ok(RegisterRISCV::X26),
        27 => Ok(RegisterRISCV::X27),
        28 => Ok(RegisterRISCV::X28),
        29 => Ok(RegisterRISCV::X29),
        30 => Ok(RegisterRISCV::X30),
        31 => Ok(RegisterRISCV::X31),
        _ => Err(MelError::Cpu(CpuError::InvalidRegister(format!(
            "index {idx}"
        )))),
    }
}

const GPR_REGISTERS: [RegisterRISCV; 32] = [
    RegisterRISCV::X0,
    RegisterRISCV::X1,
    RegisterRISCV::X2,
    RegisterRISCV::X3,
    RegisterRISCV::X4,
    RegisterRISCV::X5,
    RegisterRISCV::X6,
    RegisterRISCV::X7,
    RegisterRISCV::X8,
    RegisterRISCV::X9,
    RegisterRISCV::X10,
    RegisterRISCV::X11,
    RegisterRISCV::X12,
    RegisterRISCV::X13,
    RegisterRISCV::X14,
    RegisterRISCV::X15,
    RegisterRISCV::X16,
    RegisterRISCV::X17,
    RegisterRISCV::X18,
    RegisterRISCV::X19,
    RegisterRISCV::X20,
    RegisterRISCV::X21,
    RegisterRISCV::X22,
    RegisterRISCV::X23,
    RegisterRISCV::X24,
    RegisterRISCV::X25,
    RegisterRISCV::X26,
    RegisterRISCV::X27,
    RegisterRISCV::X28,
    RegisterRISCV::X29,
    RegisterRISCV::X30,
    RegisterRISCV::X31,
];

impl CpuBackend for Riscv64Cpu {
    fn arch(&self) -> Architecture {
        Architecture::Riscv64
    }

    fn reset(&mut self) -> Result<(), MelError> {
        let reset_pc = Architecture::Riscv64.default_reset_vector();
        self.set_pc(reset_pc)?;
        for reg in GPR_REGISTERS {
            self.uc
                .reg_write(reg, 0)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }
        self.cycles_executed = 0;
        self.mstatus = MSTATUS_MPP_M;
        self.mie = 0;
        self.mip = 0;
        self.mtvec = reset_pc;
        self.mepc = 0;
        self.mcause = 0;
        self.mtval = 0;
        Ok(())
    }

    fn step(&mut self) -> Result<CpuStepResult, MelError> {
        let current_pc = self.pc()?;
        if self.breakpoints.contains(&current_pc) {
            return Ok(CpuStepResult::Breakpoint { pc: current_pc });
        }

        if let Some(cause) = self.check_pending_interrupt() {
            return self.deliver_trap(cause);
        }

        match self.uc.emu_start(current_pc, u64::MAX, 0, 1) {
            Ok(()) => {
                self.cycles_executed = self.cycles_executed.wrapping_add(1);
                let next_pc = self.pc()?;
                if self.breakpoints.contains(&next_pc) {
                    Ok(CpuStepResult::Breakpoint { pc: next_pc })
                } else {
                    Ok(CpuStepResult::Ok { cycles: 1 })
                }
            }
            Err(err) => {
                let next_pc = self.pc().unwrap_or(current_pc);
                Err(MelError::Cpu(CpuError::Internal(format!(
                    "Unicorn emulation error at pc {next_pc:#x}: {err:?}"
                ))))
            }
        }
    }

    fn run(&mut self, max_cycles: u64) -> Result<CpuStepResult, MelError> {
        let mut executed = 0;
        while executed < max_cycles {
            let current_pc = self.pc()?;
            if self.breakpoints.contains(&current_pc) {
                return Ok(CpuStepResult::Breakpoint { pc: current_pc });
            }

            if let Some(cause) = self.check_pending_interrupt() {
                return self.deliver_trap(cause);
            }

            match self.uc.emu_start(current_pc, u64::MAX, 0, 1) {
                Ok(()) => {
                    self.cycles_executed = self.cycles_executed.wrapping_add(1);
                    executed += 1;
                    let next_pc = self.pc()?;
                    if self.breakpoints.contains(&next_pc) {
                        return Ok(CpuStepResult::Breakpoint { pc: next_pc });
                    }
                }
                Err(err) => {
                    let next_pc = self.pc().unwrap_or(current_pc);
                    return Err(MelError::Cpu(CpuError::Internal(format!(
                        "Unicorn emulation error at pc {next_pc:#x}: {err:?}"
                    ))));
                }
            }
        }

        Ok(CpuStepResult::Ok { cycles: executed })
    }

    fn read_register(&self, reg: RegisterId) -> Result<u64, MelError> {
        let name = reg.name.to_lowercase();
        match name.as_str() {
            "mstatus" => return Ok(self.mstatus),
            "mie" => return Ok(self.mie),
            "mip" => return Ok(self.mip),
            "mtvec" => return Ok(self.mtvec),
            "mepc" => return Ok(self.mepc),
            "mcause" => return Ok(self.mcause),
            "mtval" => return Ok(self.mtval),
            _ => {}
        }
        let uc_reg = resolve_register(&reg)?;
        self.uc
            .reg_read(uc_reg)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn write_register(&mut self, reg: RegisterId, val: u64) -> Result<(), MelError> {
        let name = reg.name.to_lowercase();
        match name.as_str() {
            "mstatus" => {
                self.mstatus = val;
                return Ok(());
            }
            "mie" => {
                self.mie = val;
                return Ok(());
            }
            "mip" => {
                self.mip = val;
                return Ok(());
            }
            "mtvec" => {
                self.mtvec = val;
                return Ok(());
            }
            "mepc" => {
                self.mepc = val;
                return Ok(());
            }
            "mcause" => {
                self.mcause = val;
                return Ok(());
            }
            "mtval" => {
                self.mtval = val;
                return Ok(());
            }
            _ => {}
        }
        let uc_reg = resolve_register(&reg)?;
        self.uc
            .reg_write(uc_reg, val)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn pc(&self) -> Result<u64, MelError> {
        self.uc
            .reg_read(RegisterRISCV::PC)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn set_pc(&mut self, pc: u64) -> Result<(), MelError> {
        self.uc
            .reg_write(RegisterRISCV::PC, pc)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn set_timer_interrupt(&mut self, pending: bool) {
        self.set_timer_interrupt(pending);
    }

    fn set_software_interrupt(&mut self, pending: bool) {
        self.set_software_interrupt(pending);
    }

    fn set_external_interrupt(&mut self, pending: bool) {
        self.set_external_interrupt(pending);
    }

    fn check_pending_interrupt(&self) -> Option<u64> {
        self.check_pending_interrupt()
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        let mut gprs = [0u64; 32];
        for (i, &reg) in GPR_REGISTERS.iter().enumerate() {
            gprs[i] = self
                .uc
                .reg_read(reg)
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }
        let state = Riscv64State {
            pc: self.pc()?,
            gprs,
            cycles: self.cycles_executed,
            mstatus: self.mstatus,
            mie: self.mie,
            mip: self.mip,
            mtvec: self.mtvec,
            mepc: self.mepc,
            mcause: self.mcause,
            mtval: self.mtval,
        };
        serde_json::to_vec(&state).map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let state: Riscv64State = serde_json::from_slice(data)
            .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        self.set_pc(state.pc)?;
        for (i, &reg) in GPR_REGISTERS.iter().enumerate() {
            self.uc
                .reg_write(reg, state.gprs[i])
                .map_err(|e| MelError::Cpu(CpuError::Internal(e.to_string())))?;
        }
        self.cycles_executed = state.cycles;
        self.mstatus = state.mstatus;
        self.mie = state.mie;
        self.mip = state.mip;
        self.mtvec = state.mtvec;
        self.mepc = state.mepc;
        self.mcause = state.mcause;
        self.mtval = state.mtval;
        Ok(())
    }

    fn mem_write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MelError> {
        self.mem_write(addr, bytes)
    }

    fn mem_read(&self, addr: u64, bytes: &mut [u8]) -> Result<(), MelError> {
        self.mem_read(addr, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_riscv64_registers() {
        let mut cpu = Riscv64Cpu::new().unwrap();
        assert_eq!(cpu.arch(), Architecture::Riscv64);
        assert_eq!(cpu.pc().unwrap(), 0x8000_0000);

        cpu.write_register(RegisterId::named("a0"), 0x12345678)
            .unwrap();
        assert_eq!(
            cpu.read_register(RegisterId::named("x10")).unwrap(),
            0x12345678
        );

        cpu.write_register(RegisterId::named("ra"), 0x8000_1000)
            .unwrap();
        assert_eq!(
            cpu.read_register(RegisterId::indexed("x1", 1)).unwrap(),
            0x8000_1000
        );

        cpu.set_pc(0x8000_0100).unwrap();
        assert_eq!(cpu.pc().unwrap(), 0x8000_0100);
    }

    #[test]
    fn test_riscv64_instruction_execution() {
        let mut cpu = Riscv64Cpu::new().unwrap();
        let ram_base = 0x8000_0000;
        let ram_size = 0x1000;

        cpu.mem_map(ram_base, ram_size, Prot::ALL).unwrap();

        // addi x1, x0, 42 (0x02a00093)
        // addi x2, x1, 8  (0x00808113)
        let instructions: [u8; 8] = [0x93, 0x00, 0xa0, 0x02, 0x13, 0x81, 0x80, 0x00];
        cpu.mem_write(ram_base, &instructions).unwrap();

        let step1 = cpu.step().unwrap();
        assert_eq!(step1, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("x1")).unwrap(), 42);
        assert_eq!(cpu.pc().unwrap(), ram_base + 4);

        let step2 = cpu.step().unwrap();
        assert_eq!(step2, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.read_register(RegisterId::named("x2")).unwrap(), 50);
        assert_eq!(cpu.pc().unwrap(), ram_base + 8);
    }

    #[test]
    fn test_riscv64_save_restore() {
        let mut cpu = Riscv64Cpu::new().unwrap();
        cpu.set_pc(0x8000_0020).unwrap();
        cpu.write_register(RegisterId::named("t0"), 0xDEADBEEF)
            .unwrap();
        cpu.set_mstatus(0x1808);
        cpu.set_mie(0x88);

        let snapshot = cpu.save_state().unwrap();

        let mut restored = Riscv64Cpu::new().unwrap();
        restored.load_state(&snapshot).unwrap();

        assert_eq!(restored.pc().unwrap(), 0x8000_0020);
        assert_eq!(
            restored.read_register(RegisterId::named("t0")).unwrap(),
            0xDEADBEEF
        );
        assert_eq!(restored.mstatus(), 0x1808);
        assert_eq!(restored.mie(), 0x88);
    }

    #[test]
    fn test_riscv64_timer_trap_delivery() {
        let mut cpu = Riscv64Cpu::new().unwrap();
        let ram_base = 0x8000_0000;
        cpu.mem_map(ram_base, 0x1000, Prot::ALL).unwrap();

        // 2 NOPs
        let instructions = [0x13, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00];
        cpu.mem_write(ram_base, &instructions).unwrap();
        cpu.set_pc(ram_base).unwrap();

        // Direct mode trap handler at 0x8000_0800
        cpu.set_mtvec(0x8000_0800);
        // Enable Machine Global Interrupts (MIE) and Timer Interrupts (MTIE)
        cpu.set_mstatus(MSTATUS_MIE);
        cpu.set_mie(MIE_MTIE);

        // Before interrupt is asserted, step executes normally
        let step1 = cpu.step().unwrap();
        assert_eq!(step1, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(cpu.pc().unwrap(), ram_base + 4);

        // Assert timer interrupt
        cpu.set_timer_interrupt(true);

        // Next step should deliver trap
        let step2 = cpu.step().unwrap();
        assert_eq!(
            step2,
            CpuStepResult::Trap {
                cause: MCAUSE_INTERRUPT_BIT | MCAUSE_MTI,
                epc: ram_base + 4,
            }
        );
        assert_eq!(cpu.pc().unwrap(), 0x8000_0800);
        assert_eq!(cpu.mepc(), ram_base + 4);
        assert_eq!(cpu.mcause(), MCAUSE_INTERRUPT_BIT | MCAUSE_MTI);
        // MIE cleared, MPIE set
        assert_eq!(cpu.mstatus() & MSTATUS_MIE, 0);
        assert_ne!(cpu.mstatus() & MSTATUS_MPIE, 0);

        // Clear timer interrupt and return from trap (mret)
        cpu.set_timer_interrupt(false);
        cpu.mret().unwrap();
        assert_eq!(cpu.pc().unwrap(), ram_base + 4);
        assert_ne!(cpu.mstatus() & MSTATUS_MIE, 0);
    }
}
