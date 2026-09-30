//! Unified debugger runtime tying VirtualMachine, breakpoints, disassembly, and execution control.

use mel_core::{Address, Architecture, MelResult, RegisterId};
use mel_cpu::traits::CpuStepResult;
use mel_vm::VirtualMachine;

use crate::breakpoints::{
    Breakpoint, BreakpointCondition, BreakpointHit, BreakpointManager, DebugStopReason, Watchpoint,
    WatchpointKind,
};
use crate::disasm::{disassemble, DisassembledInstruction};

/// Interactive and programmatic debugger session manager.
pub struct Debugger {
    vm: VirtualMachine,
    breakpoints: BreakpointManager,
    last_stop_reason: Option<DebugStopReason>,
}

impl Debugger {
    /// Create a new debugger attached to a virtual machine.
    pub fn new(vm: VirtualMachine) -> Self {
        Self {
            vm,
            breakpoints: BreakpointManager::new(),
            last_stop_reason: None,
        }
    }

    /// Access reference to the virtual machine.
    pub fn vm(&self) -> &VirtualMachine {
        &self.vm
    }

    /// Access mutable reference to the virtual machine.
    pub fn vm_mut(&mut self) -> &mut VirtualMachine {
        &mut self.vm
    }

    /// Access reference to the breakpoint manager.
    pub fn breakpoints(&self) -> &BreakpointManager {
        &self.breakpoints
    }

    /// Access mutable reference to the breakpoint manager.
    pub fn breakpoints_mut(&mut self) -> &mut BreakpointManager {
        &mut self.breakpoints
    }

    /// Most recent stop reason, if any.
    pub fn last_stop_reason(&self) -> Option<&DebugStopReason> {
        self.last_stop_reason.as_ref()
    }

    /// Reset VM and CPU state.
    pub fn reset(&mut self) -> MelResult<()> {
        self.vm.reset()?;
        self.last_stop_reason = None;
        Ok(())
    }

    /// Read the current Program Counter (PC).
    pub fn pc(&self) -> MelResult<Address> {
        self.vm.pc()
    }

    /// Set the Program Counter (PC).
    pub fn set_pc(&mut self, pc: Address) -> MelResult<()> {
        self.vm.set_pc(pc)
    }

    /// Read an architectural register.
    pub fn read_register(&self, reg: RegisterId) -> MelResult<u64> {
        self.vm.read_register(reg)
    }

    /// Write an architectural register.
    pub fn write_register(&mut self, reg: RegisterId, val: u64) -> MelResult<()> {
        self.vm.write_register(reg, val)
    }

    /// Add a software breakpoint on an address.
    pub fn add_breakpoint(&mut self, address: Address) -> u32 {
        self.breakpoints.add_breakpoint(address)
    }

    /// Add a temporary software breakpoint on an address.
    pub fn add_temporary_breakpoint(&mut self, address: Address) -> u32 {
        self.breakpoints.add_temporary_breakpoint(address)
    }

    /// Add a conditional software breakpoint.
    pub fn add_conditional_breakpoint(
        &mut self,
        address: Address,
        cond: BreakpointCondition,
    ) -> u32 {
        self.breakpoints.add_conditional_breakpoint(address, cond)
    }

    /// Remove a breakpoint by ID.
    pub fn remove_breakpoint(&mut self, id: u32) -> MelResult<Breakpoint> {
        self.breakpoints.remove_breakpoint(id)
    }

    /// Add a memory watchpoint.
    pub fn add_watchpoint(&mut self, address: Address, size: usize, kind: WatchpointKind) -> u32 {
        self.breakpoints.add_watchpoint(address, size, kind)
    }

    /// Add a temporary memory watchpoint.
    pub fn add_temporary_watchpoint(
        &mut self,
        address: Address,
        size: usize,
        kind: WatchpointKind,
    ) -> u32 {
        self.breakpoints
            .add_temporary_watchpoint(address, size, kind)
    }

    /// Remove a watchpoint by ID.
    pub fn remove_watchpoint(&mut self, id: u32) -> MelResult<Watchpoint> {
        self.breakpoints.remove_watchpoint(id)
    }

    /// Execute a single instruction step.
    ///
    /// Evaluates breakpoints at the destination PC or CPU trap / halt results.
    pub fn step(&mut self) -> MelResult<DebugStopReason> {
        let step_res = self.vm.step()?;

        let reason = match step_res {
            CpuStepResult::Breakpoint { pc } => {
                if let Some(hit) = self.breakpoints.check_pc(pc) {
                    match hit {
                        BreakpointHit::Breakpoint(bp) => DebugStopReason::Breakpoint {
                            id: bp.id,
                            address: bp.address,
                            hit_count: bp.hit_count,
                        },
                        _ => DebugStopReason::Breakpoint {
                            id: 0,
                            address: pc,
                            hit_count: 1,
                        },
                    }
                } else {
                    DebugStopReason::Breakpoint {
                        id: 0,
                        address: pc,
                        hit_count: 1,
                    }
                }
            }
            CpuStepResult::Halted { reason } => DebugStopReason::Halted { reason },
            CpuStepResult::Trap { cause, epc } => DebugStopReason::Trap { cause, epc },
            CpuStepResult::Ok { .. } => {
                let current_pc = self.vm.pc()?;
                if let Some(hit) = self.breakpoints.check_pc(current_pc) {
                    match hit {
                        BreakpointHit::Breakpoint(bp) => DebugStopReason::Breakpoint {
                            id: bp.id,
                            address: bp.address,
                            hit_count: bp.hit_count,
                        },
                        _ => DebugStopReason::Step,
                    }
                } else {
                    DebugStopReason::Step
                }
            }
        };

        self.last_stop_reason = Some(reason.clone());
        Ok(reason)
    }

    /// Continue execution until a breakpoint is hit, watchpoint fires, CPU traps/halts,
    /// or max_steps limit is reached.
    pub fn continue_execution(&mut self, max_steps: Option<u64>) -> MelResult<DebugStopReason> {
        let limit = max_steps.unwrap_or(1_000_000);
        let mut executed_steps = 0;

        while executed_steps < limit {
            let stop_reason = self.step()?;
            executed_steps += 1;

            match stop_reason {
                DebugStopReason::Step => {
                    // Normal instruction executed, continue next instruction
                }
                other => {
                    return Ok(other);
                }
            }
        }

        let limit_reason = DebugStopReason::CycleLimitReached {
            cycles: executed_steps,
        };
        self.last_stop_reason = Some(limit_reason.clone());
        Ok(limit_reason)
    }

    /// Write memory to VM and evaluate any triggered watchpoints.
    pub fn write_memory(
        &mut self,
        addr: Address,
        data: &[u8],
    ) -> MelResult<Option<DebugStopReason>> {
        self.vm.write_memory(addr, data)?;

        if let Some(BreakpointHit::Watchpoint {
            watchpoint,
            accessed_address,
            access_size,
            kind,
        }) = self.breakpoints.check_watchpoint(addr, data.len(), true)
        {
            let reason = DebugStopReason::Watchpoint {
                id: watchpoint.id,
                address: watchpoint.address,
                size: watchpoint.size,
                kind,
                accessed_address,
                access_size,
                hit_count: watchpoint.hit_count,
            };
            self.last_stop_reason = Some(reason.clone());
            return Ok(Some(reason));
        }

        Ok(None)
    }

    /// Read memory from VM and evaluate any triggered watchpoints.
    pub fn read_memory(
        &mut self,
        addr: Address,
        data: &mut [u8],
    ) -> MelResult<Option<DebugStopReason>> {
        self.vm.read_memory(addr, data)?;

        if let Some(BreakpointHit::Watchpoint {
            watchpoint,
            accessed_address,
            access_size,
            kind,
        }) = self.breakpoints.check_watchpoint(addr, data.len(), false)
        {
            let reason = DebugStopReason::Watchpoint {
                id: watchpoint.id,
                address: watchpoint.address,
                size: watchpoint.size,
                kind,
                accessed_address,
                access_size,
                hit_count: watchpoint.hit_count,
            };
            self.last_stop_reason = Some(reason.clone());
            return Ok(Some(reason));
        }

        Ok(None)
    }

    /// Disassemble instructions starting at a given address.
    pub fn disassemble_at(
        &self,
        addr: Address,
        count: usize,
    ) -> MelResult<Vec<DisassembledInstruction>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let max_bytes = match self.vm.arch() {
            Architecture::X86_64 => count * 15,
            _ => count * 4,
        };

        let mut buffer = vec![0u8; max_bytes];
        self.vm.read_memory(addr, &mut buffer)?;

        let all_insns = disassemble(self.vm.arch(), &buffer, addr)?;
        Ok(all_insns.into_iter().take(count).collect())
    }

    /// Disassemble instructions at the current Program Counter (PC).
    pub fn disassemble_current(&self, count: usize) -> MelResult<Vec<DisassembledInstruction>> {
        let pc = self.vm.pc()?;
        self.disassemble_at(pc, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_core::Architecture;
    use mel_vm::{create_basic_riscv64_vm, create_mock_vm};

    #[test]
    fn test_debugger_mock_vm_stepping_and_breakpoints() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let mut dbg = Debugger::new(vm);

        assert_eq!(dbg.pc().unwrap(), 0x8000_0000);

        // Add breakpoint at 0x8000_0008
        let bp_id = dbg.add_breakpoint(0x8000_0008);

        // Step 1: 0x8000_0000 -> 0x8000_0004 (Ok)
        let s1 = dbg.step().unwrap();
        assert_eq!(s1, DebugStopReason::Step);
        assert_eq!(dbg.pc().unwrap(), 0x8000_0004);

        // Step 2: 0x8000_0004 -> 0x8000_0008 (Hits breakpoint!)
        let s2 = dbg.step().unwrap();
        assert_eq!(
            s2,
            DebugStopReason::Breakpoint {
                id: bp_id,
                address: 0x8000_0008,
                hit_count: 1
            }
        );
        assert_eq!(dbg.pc().unwrap(), 0x8000_0008);
    }

    #[test]
    fn test_debugger_continue_execution() {
        let vm = create_mock_vm(Architecture::Arm64).unwrap();
        let mut dbg = Debugger::new(vm);

        let bp_target = 0x4000_0010;
        let bp_id = dbg.add_breakpoint(bp_target);

        let reason = dbg.continue_execution(Some(100)).unwrap();
        assert_eq!(
            reason,
            DebugStopReason::Breakpoint {
                id: bp_id,
                address: bp_target,
                hit_count: 1
            }
        );
        assert_eq!(dbg.pc().unwrap(), bp_target);
    }

    #[test]
    fn test_debugger_watchpoints() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let mut dbg = Debugger::new(vm);

        let wp_id = dbg.add_watchpoint(0x8000_0100, 4, WatchpointKind::Write);

        // Writing to non-watched address produces no watchpoint stop
        let res_none = dbg.write_memory(0x8000_0050, &[0xAA; 4]).unwrap();
        assert!(res_none.is_none());

        // Writing to watched address triggers watchpoint
        let res_wp = dbg
            .write_memory(0x8000_0100, &[0x11, 0x22, 0x33, 0x44])
            .unwrap();
        assert_eq!(
            res_wp,
            Some(DebugStopReason::Watchpoint {
                id: wp_id,
                address: 0x8000_0100,
                size: 4,
                kind: WatchpointKind::Write,
                accessed_address: 0x8000_0100,
                access_size: 4,
                hit_count: 1,
            })
        );
    }

    #[test]
    fn test_debugger_riscv64_real_execution_and_disassembly() {
        let mut vm = create_basic_riscv64_vm().unwrap();

        // 3 instructions:
        // lui x1, 0x10000
        // addi x2, x0, 88
        // sb x2, 0(x1)
        let code: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x80, 0x05, 0x23, 0x80, 0x20, 0x00,
        ];
        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        let mut dbg = Debugger::new(vm);

        // Disassemble current instructions
        let disasm = dbg.disassemble_current(3).unwrap();
        assert_eq!(disasm.len(), 3);
        assert_eq!(disasm[0].mnemonic, "lui");
        assert_eq!(disasm[1].mnemonic, "addi");
        assert_eq!(disasm[2].mnemonic, "sb");

        // Set breakpoint on 3rd instruction (0x8000_0008)
        let bp_id = dbg.add_breakpoint(0x8000_0008);

        // Continue execution to breakpoint
        let stop = dbg.continue_execution(Some(10)).unwrap();
        assert_eq!(
            stop,
            DebugStopReason::Breakpoint {
                id: bp_id,
                address: 0x8000_0008,
                hit_count: 1
            }
        );
        assert_eq!(dbg.pc().unwrap(), 0x8000_0008);

        // Finish last step (sb x2, 0(x1))
        let final_step = dbg.step().unwrap();
        assert_eq!(final_step, DebugStopReason::Step);
        assert_eq!(dbg.pc().unwrap(), 0x8000_000c);

        let uart_out = dbg.vm().uart().unwrap().lock().unwrap().drain_output();
        assert_eq!(uart_out, b"X");
    }
}
