//! Scenario execution and assertion validator engine.

use mel_core::{MelError, RegisterId};
use mel_cpu::traits::CpuStepResult;
use mel_vm::VirtualMachine;
use mel_vm::factory::build_vm_for_arch;
use serde::{Deserialize, Serialize};

use crate::schema::{Objective, Scenario};

/// Status outcome of evaluating an individual objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectiveStatus {
    Passed,
    Failed,
    Error,
}

impl ObjectiveStatus {
    /// Returns true if objective passed successfully.
    pub const fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Evaluation result for a single scenario objective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveEvaluation {
    pub objective: Objective,
    pub status: ObjectiveStatus,
    pub message: String,
    pub actual: Option<String>,
    pub expected: Option<String>,
}

/// Comprehensive scenario grading report with scores and feedback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub scenario_name: String,
    pub passed: bool,
    pub score_percent: f64,
    pub objectives_passed: usize,
    pub objectives_total: usize,
    pub cycles_executed: u64,
    pub wall_time_ns: u64,
    pub stop_reason: String,
    pub results: Vec<ObjectiveEvaluation>,
    pub feedback: Vec<String>,
}

impl ScenarioReport {
    /// Format a human-readable multi-line summary of the scenario evaluation.
    pub fn summary_string(&self) -> String {
        let mut out = String::new();
        let status_tag = if self.passed { "PASSED" } else { "FAILED" };
        out.push_str(&format!(
            "=== Scenario: {} [{}] ===\n",
            self.scenario_name, status_tag
        ));
        out.push_str(&format!(
            "Score: {:.1}% ({}/{} objectives)\n",
            self.score_percent, self.objectives_passed, self.objectives_total
        ));
        out.push_str(&format!(
            "Cycles: {} | Wall Time: {} ns | Stop Reason: {}\n\n",
            self.cycles_executed, self.wall_time_ns, self.stop_reason
        ));

        out.push_str("Objectives:\n");
        for (idx, res) in self.results.iter().enumerate() {
            let symbol = match res.status {
                ObjectiveStatus::Passed => "[PASS]",
                ObjectiveStatus::Failed => "[FAIL]",
                ObjectiveStatus::Error => "[ERR ]",
            };
            out.push_str(&format!(
                "  {} #{}: {}\n",
                symbol,
                idx + 1,
                res.objective.description()
            ));
            if !res.status.is_passed() {
                out.push_str(&format!("         Message: {}\n", res.message));
                if let (Some(exp), Some(act)) = (&res.expected, &res.actual) {
                    out.push_str(&format!("         Expected: {exp}\n"));
                    out.push_str(&format!("         Actual:   {act}\n"));
                }
            }
        }

        if !self.feedback.is_empty() {
            out.push_str("\nFeedback:\n");
            for f in &self.feedback {
                out.push_str(&format!("  - {f}\n"));
            }
        }

        out
    }
}

/// Scenario validator engine.
pub struct ScenarioValidator;

impl ScenarioValidator {
    /// Evaluate a single objective against the current virtual machine state.
    pub fn evaluate_objective(
        objective: &Objective,
        vm: &VirtualMachine,
        _last_step: &Option<CpuStepResult>,
        had_traps: bool,
    ) -> ObjectiveEvaluation {
        match objective {
            Objective::RegisterEquals {
                register,
                value,
                description: _,
            } => {
                let reg_id = RegisterId::named(register);
                match vm.read_register(reg_id) {
                    Ok(actual_val) => {
                        if actual_val == *value {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Passed,
                                message: format!("Register {register} matches expected {value:#x}"),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("{value:#x}")),
                            }
                        } else {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Failed,
                                message: format!(
                                    "Register {register} expected {value:#x}, but found {actual_val:#x}"
                                ),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("{value:#x}")),
                            }
                        }
                    }
                    Err(e) => ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: format!("Failed to read register '{register}': {e}"),
                        actual: None,
                        expected: Some(format!("{value:#x}")),
                    },
                }
            }

            Objective::RegisterInRange {
                register,
                min,
                max,
                description: _,
            } => {
                let reg_id = RegisterId::named(register);
                match vm.read_register(reg_id) {
                    Ok(actual_val) => {
                        if actual_val >= *min && actual_val <= *max {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Passed,
                                message: format!(
                                    "Register {register} ({actual_val:#x}) is within [{min:#x}, {max:#x}]"
                                ),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("[{min:#x}..={max:#x}]")),
                            }
                        } else {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Failed,
                                message: format!(
                                    "Register {register} ({actual_val:#x}) out of range [{min:#x}, {max:#x}]"
                                ),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("[{min:#x}..={max:#x}]")),
                            }
                        }
                    }
                    Err(e) => ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: format!("Failed to read register '{register}': {e}"),
                        actual: None,
                        expected: Some(format!("[{min:#x}..={max:#x}]")),
                    },
                }
            }

            Objective::MemoryContains {
                address,
                expected_bytes,
                description: _,
            } => {
                let mut buffer = vec![0u8; expected_bytes.len()];
                match vm.read_memory(*address, &mut buffer) {
                    Ok(()) => {
                        if buffer == *expected_bytes {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Passed,
                                message: format!(
                                    "Memory at {address:#x} matches expected {} bytes",
                                    expected_bytes.len()
                                ),
                                actual: Some(format!("{buffer:02x?}")),
                                expected: Some(format!("{expected_bytes:02x?}")),
                            }
                        } else {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Failed,
                                message: format!(
                                    "Memory mismatch at {address:#x} (size: {} bytes)",
                                    expected_bytes.len()
                                ),
                                actual: Some(format!("{buffer:02x?}")),
                                expected: Some(format!("{expected_bytes:02x?}")),
                            }
                        }
                    }
                    Err(e) => ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: format!("Failed to read memory at {address:#x}: {e}"),
                        actual: None,
                        expected: Some(format!("{expected_bytes:02x?}")),
                    },
                }
            }

            Objective::MemoryEqualsU64 {
                address,
                value,
                description: _,
            } => {
                let mut buffer = [0u8; 8];
                match vm.read_memory(*address, &mut buffer) {
                    Ok(()) => {
                        let actual_val = u64::from_le_bytes(buffer);
                        if actual_val == *value {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Passed,
                                message: format!("Memory u64 at {address:#x} == {value:#x}"),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("{value:#x}")),
                            }
                        } else {
                            ObjectiveEvaluation {
                                objective: objective.clone(),
                                status: ObjectiveStatus::Failed,
                                message: format!(
                                    "Memory u64 at {address:#x} expected {value:#x}, found {actual_val:#x}"
                                ),
                                actual: Some(format!("{actual_val:#x}")),
                                expected: Some(format!("{value:#x}")),
                            }
                        }
                    }
                    Err(e) => ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: format!("Failed to read memory u64 at {address:#x}: {e}"),
                        actual: None,
                        expected: Some(format!("{value:#x}")),
                    },
                }
            }

            Objective::UartOutputContains {
                expected,
                description: _,
            } => {
                if let Some(uart_arc) = vm.uart() {
                    match uart_arc.lock() {
                        Ok(uart) => {
                            let emitted = uart.peek_output();
                            let output_str = String::from_utf8_lossy(emitted).to_string();
                            if output_str.contains(expected) {
                                ObjectiveEvaluation {
                                    objective: objective.clone(),
                                    status: ObjectiveStatus::Passed,
                                    message: format!(
                                        "UART output contains expected substring {expected:?}"
                                    ),
                                    actual: Some(format!("{output_str:?}")),
                                    expected: Some(format!("{expected:?}")),
                                }
                            } else {
                                ObjectiveEvaluation {
                                    objective: objective.clone(),
                                    status: ObjectiveStatus::Failed,
                                    message: format!(
                                        "UART output does not contain expected substring {expected:?}"
                                    ),
                                    actual: Some(format!("{output_str:?}")),
                                    expected: Some(format!("{expected:?}")),
                                }
                            }
                        }
                        Err(e) => ObjectiveEvaluation {
                            objective: objective.clone(),
                            status: ObjectiveStatus::Error,
                            message: format!("Failed to lock UART device: {e}"),
                            actual: None,
                            expected: Some(format!("{expected:?}")),
                        },
                    }
                } else {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: "No UART device attached to virtual machine".into(),
                        actual: None,
                        expected: Some(format!("{expected:?}")),
                    }
                }
            }

            Objective::UartOutputExact {
                expected,
                description: _,
            } => {
                if let Some(uart_arc) = vm.uart() {
                    match uart_arc.lock() {
                        Ok(uart) => {
                            let emitted = uart.peek_output();
                            let output_str = String::from_utf8_lossy(emitted).to_string();
                            if output_str == *expected {
                                ObjectiveEvaluation {
                                    objective: objective.clone(),
                                    status: ObjectiveStatus::Passed,
                                    message: format!(
                                        "UART output exactly matches expected string {expected:?}"
                                    ),
                                    actual: Some(format!("{output_str:?}")),
                                    expected: Some(format!("{expected:?}")),
                                }
                            } else {
                                ObjectiveEvaluation {
                                    objective: objective.clone(),
                                    status: ObjectiveStatus::Failed,
                                    message: format!(
                                        "UART output expected {expected:?}, but found {output_str:?}"
                                    ),
                                    actual: Some(format!("{output_str:?}")),
                                    expected: Some(format!("{expected:?}")),
                                }
                            }
                        }
                        Err(e) => ObjectiveEvaluation {
                            objective: objective.clone(),
                            status: ObjectiveStatus::Error,
                            message: format!("Failed to lock UART device: {e}"),
                            actual: None,
                            expected: Some(format!("{expected:?}")),
                        },
                    }
                } else {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Error,
                        message: "No UART device attached to virtual machine".into(),
                        actual: None,
                        expected: Some(format!("{expected:?}")),
                    }
                }
            }

            Objective::CyclesUnder {
                max_cycles,
                description: _,
            } => {
                let actual_cycles = vm.cycle_count();
                if actual_cycles < *max_cycles {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Passed,
                        message: format!(
                            "Execution finished in {actual_cycles} cycles (under limit of {max_cycles})"
                        ),
                        actual: Some(format!("{actual_cycles}")),
                        expected: Some(format!("< {max_cycles}")),
                    }
                } else {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Failed,
                        message: format!(
                            "Execution took {actual_cycles} cycles, exceeding limit of {max_cycles}"
                        ),
                        actual: Some(format!("{actual_cycles}")),
                        expected: Some(format!("< {max_cycles}")),
                    }
                }
            }

            Objective::NoTraps { description: _ } => {
                if !had_traps {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Passed,
                        message: "Execution completed with zero unhandled traps".into(),
                        actual: Some("No traps".into()),
                        expected: Some("No traps".into()),
                    }
                } else {
                    ObjectiveEvaluation {
                        objective: objective.clone(),
                        status: ObjectiveStatus::Failed,
                        message: "CPU encountered an unhandled exception or trap".into(),
                        actual: Some("Trap encountered".into()),
                        expected: Some("No traps".into()),
                    }
                }
            }

            Objective::ProgramCounterEquals {
                address,
                description: _,
            } => match vm.pc() {
                Ok(actual_pc) => {
                    if actual_pc == *address {
                        ObjectiveEvaluation {
                            objective: objective.clone(),
                            status: ObjectiveStatus::Passed,
                            message: format!("Program counter is at {address:#x}"),
                            actual: Some(format!("{actual_pc:#x}")),
                            expected: Some(format!("{address:#x}")),
                        }
                    } else {
                        ObjectiveEvaluation {
                            objective: objective.clone(),
                            status: ObjectiveStatus::Failed,
                            message: format!(
                                "Program counter expected {address:#x}, but found {actual_pc:#x}"
                            ),
                            actual: Some(format!("{actual_pc:#x}")),
                            expected: Some(format!("{address:#x}")),
                        }
                    }
                }
                Err(e) => ObjectiveEvaluation {
                    objective: objective.clone(),
                    status: ObjectiveStatus::Error,
                    message: format!("Failed to read PC: {e}"),
                    actual: None,
                    expected: Some(format!("{address:#x}")),
                },
            },
        }
    }

    /// Evaluate all objectives for a scenario against the current VM state.
    pub fn validate(
        scenario: &Scenario,
        vm: &VirtualMachine,
        last_step: Option<CpuStepResult>,
        had_traps: bool,
    ) -> ScenarioReport {
        let mut results = Vec::with_capacity(scenario.objectives.len());
        let mut passed_count = 0usize;
        let mut feedback = Vec::new();

        for obj in &scenario.objectives {
            let eval = Self::evaluate_objective(obj, vm, &last_step, had_traps);
            if eval.status.is_passed() {
                passed_count += 1;
            } else {
                feedback.push(eval.message.clone());
            }
            results.push(eval);
        }

        let total = scenario.objectives.len();
        let score_percent = if total > 0 {
            (passed_count as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let stop_reason = match &last_step {
            Some(CpuStepResult::Halted { reason }) => format!("Halted: {reason}"),
            Some(CpuStepResult::Breakpoint { pc }) => format!("Breakpoint at {pc:#x}"),
            Some(CpuStepResult::Trap { cause, epc }) => {
                format!("Trap cause {cause:#x} at epc {epc:#x}")
            }
            Some(CpuStepResult::Ok { .. }) => "Step completed / Timeout".to_string(),
            None => "Initial state".to_string(),
        };

        ScenarioReport {
            scenario_name: scenario.name.clone(),
            passed: passed_count == total,
            score_percent,
            objectives_passed: passed_count,
            objectives_total: total,
            cycles_executed: vm.cycle_count(),
            wall_time_ns: vm.wall_time_ns(),
            stop_reason,
            results,
            feedback,
        }
    }

    /// Run a payload on a newly instantiated VM and evaluate the scenario.
    pub fn run_and_validate(
        scenario: &Scenario,
        payload: &[u8],
    ) -> Result<ScenarioReport, MelError> {
        let mut vm = build_vm_for_arch(scenario.architecture)?;

        if payload.len() >= 4 && &payload[0..4] == b"\x7fELF" {
            mel_loader::load_elf(&mut vm, payload)?;
        } else {
            let entry_point = scenario
                .initial_pc
                .unwrap_or_else(|| scenario.architecture.default_ram_base());
            mel_loader::load_raw_binary(&mut vm, payload, entry_point)?;
        }

        Self::run_and_validate_vm(scenario, &mut vm)
    }

    /// Execute an already loaded VM up to the scenario timeout and evaluate objectives.
    pub fn run_and_validate_vm(
        scenario: &Scenario,
        vm: &mut VirtualMachine,
    ) -> Result<ScenarioReport, MelError> {
        let mut had_traps = false;
        let mut last_step = None;
        let mut step_count = 0u64;

        while step_count < scenario.timeout_cycles {
            let step_res = vm.step()?;
            match step_res {
                CpuStepResult::Trap { .. } => {
                    had_traps = true;
                    last_step = Some(step_res);
                    break;
                }
                CpuStepResult::Halted { .. } => {
                    last_step = Some(step_res);
                    break;
                }
                CpuStepResult::Breakpoint { .. } => {
                    last_step = Some(step_res);
                    break;
                }
                CpuStepResult::Ok { .. } => {
                    last_step = Some(step_res);
                    step_count += 1;
                }
            }
        }

        Ok(Self::validate(scenario, vm, last_step, had_traps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_core::Architecture;
    use mel_vm::create_basic_riscv64_vm;

    #[test]
    fn test_validator_register_equals() {
        let mut vm = create_basic_riscv64_vm().unwrap();
        vm.write_register(RegisterId::named("x10"), 42).unwrap();

        let obj_pass = Objective::RegisterEquals {
            register: "x10".into(),
            value: 42,
            description: None,
        };
        let eval_pass = ScenarioValidator::evaluate_objective(&obj_pass, &vm, &None, false);
        assert!(eval_pass.status.is_passed());

        let obj_fail = Objective::RegisterEquals {
            register: "x10".into(),
            value: 99,
            description: None,
        };
        let eval_fail = ScenarioValidator::evaluate_objective(&obj_fail, &vm, &None, false);
        assert_eq!(eval_fail.status, ObjectiveStatus::Failed);
    }

    #[test]
    fn test_validator_memory_contains_and_u64() {
        let mut vm = create_basic_riscv64_vm().unwrap();
        let payload = [0xAA, 0xBB, 0xCC, 0xDD];
        vm.write_memory(0x8000_0000, &payload).unwrap();

        let obj_mem = Objective::MemoryContains {
            address: 0x8000_0000,
            expected_bytes: vec![0xAA, 0xBB, 0xCC, 0xDD],
            description: None,
        };
        let eval_mem = ScenarioValidator::evaluate_objective(&obj_mem, &vm, &None, false);
        assert!(eval_mem.status.is_passed());

        let val_u64 = 0x0102030405060708u64;
        vm.write_memory(0x8000_0010, &val_u64.to_le_bytes())
            .unwrap();

        let obj_u64 = Objective::MemoryEqualsU64 {
            address: 0x8000_0010,
            value: val_u64,
            description: None,
        };
        let eval_u64 = ScenarioValidator::evaluate_objective(&obj_u64, &vm, &None, false);
        assert!(eval_u64.status.is_passed());
    }

    #[test]
    fn test_validator_uart_output() {
        let vm = create_basic_riscv64_vm().unwrap();
        for &b in b"HELLO MEL\n" {
            vm.bus()
                .lock()
                .unwrap()
                .write(0x1000_0000, b as u64, 1, mel_core::Endianness::Little)
                .unwrap();
        }

        let obj_contains = Objective::UartOutputContains {
            expected: "HELLO".into(),
            description: None,
        };
        let eval_c = ScenarioValidator::evaluate_objective(&obj_contains, &vm, &None, false);
        assert!(eval_c.status.is_passed());

        let obj_exact = Objective::UartOutputExact {
            expected: "HELLO MEL\n".into(),
            description: None,
        };
        let eval_e = ScenarioValidator::evaluate_objective(&obj_exact, &vm, &None, false);
        assert!(eval_e.status.is_passed());
    }

    #[test]
    fn test_validator_full_scenario_pass() {
        let mut vm = create_basic_riscv64_vm().unwrap();
        // lui x1, 0x10000 -> addi x2, x0, 89 ('Y') -> sb x2, 0(x1) -> j . (infinite spin)
        let code: [u8; 16] = [
            0xb7, 0x00, 0x00, 0x10, // lui x1, 0x10000
            0x13, 0x01, 0x90, 0x05, // addi x2, x0, 89
            0x23, 0x80, 0x20, 0x00, // sb x2, 0(x1)
            0x6f, 0x00, 0x00, 0x00, // j .
        ];
        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        let scenario = Scenario {
            id: Some("test_uart".into()),
            name: "UART Test".into(),
            description: "Test UART output".into(),
            architecture: Architecture::Riscv64,
            machine_profile: None,
            machine_profile_yaml: None,
            timeout_cycles: 10,
            initial_pc: Some(0x8000_0000),
            provided_tools: vec![],
            objectives: vec![
                Objective::NoTraps { description: None },
                Objective::UartOutputContains {
                    expected: "Y".into(),
                    description: None,
                },
                Objective::CyclesUnder {
                    max_cycles: 100,
                    description: None,
                },
            ],
            constraints: vec![],
            metadata: std::collections::HashMap::new(),
        };

        let report = ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).unwrap();
        assert!(report.passed);
        assert_eq!(report.score_percent, 100.0);
        assert_eq!(report.objectives_passed, 3);
        assert!(report.summary_string().contains("[PASS]"));
    }

    #[test]
    fn test_validator_range_and_pc() {
        let mut vm = create_basic_riscv64_vm().unwrap();
        vm.write_register(RegisterId::named("x5"), 100).unwrap();
        vm.set_pc(0x8000_0020).unwrap();

        let obj_range_pass = Objective::RegisterInRange {
            register: "x5".into(),
            min: 50,
            max: 150,
            description: None,
        };
        let eval_rp = ScenarioValidator::evaluate_objective(&obj_range_pass, &vm, &None, false);
        assert!(eval_rp.status.is_passed());

        let obj_range_fail = Objective::RegisterInRange {
            register: "x5".into(),
            min: 200,
            max: 300,
            description: None,
        };
        let eval_rf = ScenarioValidator::evaluate_objective(&obj_range_fail, &vm, &None, false);
        assert_eq!(eval_rf.status, ObjectiveStatus::Failed);

        let obj_pc_pass = Objective::ProgramCounterEquals {
            address: 0x8000_0020,
            description: None,
        };
        let eval_pp = ScenarioValidator::evaluate_objective(&obj_pc_pass, &vm, &None, false);
        assert!(eval_pp.status.is_passed());

        let obj_pc_fail = Objective::ProgramCounterEquals {
            address: 0x8000_0000,
            description: None,
        };
        let eval_pf = ScenarioValidator::evaluate_objective(&obj_pc_fail, &vm, &None, false);
        assert_eq!(eval_pf.status, ObjectiveStatus::Failed);
    }

    #[test]
    fn test_validator_traps_and_failing_report() {
        let vm = create_basic_riscv64_vm().unwrap();

        let scenario = Scenario {
            id: Some("failing_scenario".into()),
            name: "Failing Scenario".into(),
            description: "Assert impossible condition".into(),
            architecture: Architecture::Riscv64,
            machine_profile: None,
            machine_profile_yaml: None,
            timeout_cycles: 100,
            initial_pc: None,
            provided_tools: vec![],
            objectives: vec![
                Objective::RegisterEquals {
                    register: "x1".into(),
                    value: 0xDEADBEEF,
                    description: Some("x1 has deadbeef".into()),
                },
                Objective::NoTraps { description: None },
            ],
            constraints: vec![],
            metadata: std::collections::HashMap::new(),
        };

        let report = ScenarioValidator::validate(&scenario, &vm, None, true);
        assert!(!report.passed);
        assert_eq!(report.objectives_passed, 0);
        assert_eq!(report.objectives_total, 2);
        assert_eq!(report.score_percent, 0.0);
        assert_eq!(report.feedback.len(), 2);
        assert!(report.summary_string().contains("[FAIL]"));
    }
}
