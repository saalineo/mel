//! Scenario definition schema for challenges, objectives, and constraints.

use mel_core::{Address, Architecture, MelError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// High-level scenario objective or assertion condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Objective {
    /// Architectural register must equal expected value.
    RegisterEquals {
        register: String,
        value: u64,
        #[serde(default)]
        description: Option<String>,
    },
    /// Architectural register must fall within inclusive range [min, max].
    RegisterInRange {
        register: String,
        min: u64,
        max: u64,
        #[serde(default)]
        description: Option<String>,
    },
    /// Memory at `address` must contain the expected byte sequence.
    MemoryContains {
        address: Address,
        expected_bytes: Vec<u8>,
        #[serde(default)]
        description: Option<String>,
    },
    /// Memory 64-bit value at `address` must equal `value`.
    MemoryEqualsU64 {
        address: Address,
        value: u64,
        #[serde(default)]
        description: Option<String>,
    },
    /// UART serial console output must contain the expected substring.
    UartOutputContains {
        expected: String,
        #[serde(default)]
        description: Option<String>,
    },
    /// UART serial console output must exactly equal the expected string.
    UartOutputExact {
        expected: String,
        #[serde(default)]
        description: Option<String>,
    },
    /// Program execution must complete in fewer than `max_cycles`.
    CyclesUnder {
        max_cycles: u64,
        #[serde(default)]
        description: Option<String>,
    },
    /// Execution must complete without any CPU trap exceptions.
    NoTraps {
        #[serde(default)]
        description: Option<String>,
    },
    /// Program counter must halt/stop at the specified target address.
    ProgramCounterEquals {
        address: Address,
        #[serde(default)]
        description: Option<String>,
    },
}

impl Objective {
    /// Return human-readable description of the objective.
    pub fn description(&self) -> String {
        match self {
            Self::RegisterEquals {
                register,
                value,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("Register {register} == {value:#x}")),
            Self::RegisterInRange {
                register,
                min,
                max,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("Register {register} in [{min:#x}, {max:#x}]")),
            Self::MemoryContains {
                address,
                expected_bytes,
                description,
            } => description.clone().unwrap_or_else(|| {
                format!(
                    "Memory at {address:#x} contains {} bytes",
                    expected_bytes.len()
                )
            }),
            Self::MemoryEqualsU64 {
                address,
                value,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("Memory u64 at {address:#x} == {value:#x}")),
            Self::UartOutputContains {
                expected,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("UART output contains {:?}", expected)),
            Self::UartOutputExact {
                expected,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("UART output exactly equals {:?}", expected)),
            Self::CyclesUnder {
                max_cycles,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("Execution completed under {max_cycles} cycles")),
            Self::NoTraps { description } => description
                .clone()
                .unwrap_or_else(|| "Execution finished without unhandled CPU traps".to_string()),
            Self::ProgramCounterEquals {
                address,
                description,
            } => description
                .clone()
                .unwrap_or_else(|| format!("Program counter halted at {address:#x}")),
        }
    }
}

/// Execution constraint applied to guest payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// Maximum size of binary payload in bytes.
    MaxBinarySize { bytes: usize },
    /// Prohibited instructions or mnemonic substrings.
    DisallowedInstructions { opcodes: Vec<String> },
    /// Maximum total cycle limit before force-terminating execution.
    MaxExecutionCycles { cycles: u64 },
    /// Read-only memory boundary.
    ReadOnlyMemory { start: Address, end: Address },
    /// Custom constraint description.
    Custom { description: String },
}

/// Complete challenge scenario definition parsed from YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// Unique scenario identifier (e.g. "scenario_01_first_boot").
    #[serde(default)]
    pub id: Option<String>,
    /// Display title of the scenario.
    pub name: String,
    /// Detailed overview and objective briefing.
    pub description: String,
    /// Target CPU architecture.
    pub architecture: Architecture,
    /// Target machine hardware profile name (e.g. "riscv64-virt").
    #[serde(default)]
    pub machine_profile: Option<String>,
    /// Optional embedded machine profile YAML configuration.
    #[serde(default)]
    pub machine_profile_yaml: Option<String>,
    /// Default execution timeout in VM cycles (default: 1,000,000).
    #[serde(default = "default_timeout_cycles")]
    pub timeout_cycles: u64,
    /// Optional explicit initial PC override.
    #[serde(default)]
    pub initial_pc: Option<Address>,
    /// Available tools and utilities provided to the player.
    #[serde(default)]
    pub provided_tools: Vec<String>,
    /// List of verification objectives required to pass.
    #[serde(default)]
    pub objectives: Vec<Objective>,
    /// Restrictions and limits enforced on payload execution.
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    /// Extensible metadata tags.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_timeout_cycles() -> u64 {
    1_000_000
}

impl Scenario {
    /// Parse scenario definition from a YAML string.
    pub fn from_yaml_str(yaml_str: &str) -> Result<Self, MelError> {
        let scenario: Self = serde_yaml::from_str(yaml_str)
            .map_err(|e| MelError::Scenario(format!("Failed to parse scenario YAML: {e}")))?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Read and parse scenario definition from a YAML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, MelError> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::from_yaml_str(&content)
    }

    /// Serialize scenario definition into a formatted YAML string.
    pub fn to_yaml_string(&self) -> Result<String, MelError> {
        serde_yaml::to_string(self)
            .map_err(|e| MelError::Scenario(format!("Failed to serialize scenario: {e}")))
    }

    /// Validate scenario configuration rules and invariants.
    pub fn validate(&self) -> Result<(), MelError> {
        if self.name.trim().is_empty() {
            return Err(MelError::Scenario("Scenario name cannot be empty".into()));
        }

        if self.description.trim().is_empty() {
            return Err(MelError::Scenario(
                "Scenario description cannot be empty".into(),
            ));
        }

        if self.timeout_cycles == 0 {
            return Err(MelError::Scenario(
                "Scenario timeout_cycles must be greater than 0".into(),
            ));
        }

        if self.objectives.is_empty() {
            return Err(MelError::Scenario(
                "Scenario must specify at least one verification objective".into(),
            ));
        }

        for (idx, obj) in self.objectives.iter().enumerate() {
            match obj {
                Objective::RegisterEquals { register, .. } => {
                    if register.trim().is_empty() {
                        return Err(MelError::Scenario(format!(
                            "Objective #{idx}: Register identifier cannot be empty"
                        )));
                    }
                }
                Objective::RegisterInRange {
                    register, min, max, ..
                } => {
                    if register.trim().is_empty() {
                        return Err(MelError::Scenario(format!(
                            "Objective #{idx}: Register identifier cannot be empty"
                        )));
                    }
                    if min > max {
                        return Err(MelError::Scenario(format!(
                            "Objective #{idx}: Register range min ({min:#x}) cannot be greater than max ({max:#x})"
                        )));
                    }
                }
                Objective::MemoryContains { expected_bytes, .. } if expected_bytes.is_empty() => {
                    return Err(MelError::Scenario(format!(
                        "Objective #{idx}: MemoryContains expected_bytes cannot be empty"
                    )));
                }
                Objective::CyclesUnder { max_cycles, .. } if *max_cycles == 0 => {
                    return Err(MelError::Scenario(format!(
                        "Objective #{idx}: CyclesUnder max_cycles must be greater than 0"
                    )));
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scenario_yaml() {
        let yaml = r#"
name: "First Boot & Stack Setup"
description: "Initialize stack pointer sp and halt cleanly."
architecture: "riscv64"
machine_profile: "riscv64-virt"
timeout_cycles: 50000
initial_pc: 0x80000000
provided_tools:
  - "riscv64-unknown-elf-gcc"
  - "debugger"
objectives:
  - type: "register_equals"
    register: "sp"
    value: 0x80010000
    description: "Stack pointer initialized to 0x80010000"
  - type: "no_traps"
    description: "No unhandled exceptions or traps"
  - type: "uart_output_contains"
    expected: "BOOT OK"
constraints:
  - type: "max_binary_size"
    bytes: 4096
  - type: "max_execution_cycles"
    cycles: 10000
"#;

        let scenario = Scenario::from_yaml_str(yaml).unwrap();
        assert_eq!(scenario.name, "First Boot & Stack Setup");
        assert_eq!(scenario.architecture, Architecture::Riscv64);
        assert_eq!(scenario.timeout_cycles, 50000);
        assert_eq!(scenario.initial_pc, Some(0x8000_0000));
        assert_eq!(scenario.provided_tools.len(), 2);
        assert_eq!(scenario.objectives.len(), 3);
        assert_eq!(scenario.constraints.len(), 2);

        match &scenario.objectives[0] {
            Objective::RegisterEquals {
                register, value, ..
            } => {
                assert_eq!(register, "sp");
                assert_eq!(*value, 0x8001_0000);
            }
            _ => panic!("Expected RegisterEquals"),
        }

        match &scenario.objectives[1] {
            Objective::NoTraps { .. } => {}
            _ => panic!("Expected NoTraps"),
        }

        match &scenario.objectives[2] {
            Objective::UartOutputContains { expected, .. } => {
                assert_eq!(expected, "BOOT OK");
            }
            _ => panic!("Expected UartOutputContains"),
        }
    }

    #[test]
    fn test_scenario_validation_empty_name() {
        let yaml = r#"
name: ""
description: "Some scenario"
architecture: "riscv64"
objectives:
  - type: "no_traps"
"#;
        let err = Scenario::from_yaml_str(yaml).unwrap_err();
        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn test_scenario_validation_no_objectives() {
        let yaml = r#"
name: "Invalid Scenario"
description: "Has no objectives"
architecture: "arm64"
objectives: []
"#;
        let err = Scenario::from_yaml_str(yaml).unwrap_err();
        assert!(
            err.to_string()
                .contains("at least one verification objective")
        );
    }

    #[test]
    fn test_scenario_validation_invalid_objectives() {
        let yaml_bad_range = r#"
name: "Bad Range"
description: "Test invalid range"
architecture: "riscv64"
objectives:
  - type: "register_in_range"
    register: "a0"
    min: 100
    max: 10
"#;
        assert!(Scenario::from_yaml_str(yaml_bad_range).is_err());

        let yaml_empty_reg = r#"
name: "Empty Reg"
description: "Test empty reg"
architecture: "riscv64"
objectives:
  - type: "register_equals"
    register: "  "
    value: 0
"#;
        assert!(Scenario::from_yaml_str(yaml_empty_reg).is_err());

        let yaml_zero_cycles = r#"
name: "Zero Cycles"
description: "Test zero cycles"
architecture: "riscv64"
objectives:
  - type: "cycles_under"
    max_cycles: 0
"#;
        assert!(Scenario::from_yaml_str(yaml_zero_cycles).is_err());
    }

    #[test]
    fn test_scenario_roundtrip_yaml() {
        let scenario = Scenario {
            id: Some("01_boot".into()),
            name: "ARM64 Boot".into(),
            description: "Boot ARM64 system".into(),
            architecture: Architecture::Arm64,
            machine_profile: Some("arm64-virt".into()),
            machine_profile_yaml: None,
            timeout_cycles: 20000,
            initial_pc: Some(0x4000_0000),
            provided_tools: vec!["aarch64-linux-gnu-gcc".into()],
            objectives: vec![
                Objective::RegisterEquals {
                    register: "sp".into(),
                    value: 0x4001_0000,
                    description: None,
                },
                Objective::CyclesUnder {
                    max_cycles: 500,
                    description: Some("Fast boot".into()),
                },
            ],
            constraints: vec![Constraint::MaxBinarySize { bytes: 1024 }],
            metadata: HashMap::new(),
        };

        let yaml_str = scenario.to_yaml_string().unwrap();
        let deserialized = Scenario::from_yaml_str(&yaml_str).unwrap();
        assert_eq!(scenario, deserialized);
    }

    #[test]
    fn test_scenario_from_file() {
        let yaml = r#"
name: "File Test Scenario"
description: "Tests file reading"
architecture: "arm64"
objectives:
  - type: "program_counter_equals"
    address: 0x40000008
"#;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_scenario_{}.yaml", std::process::id()));
        std::fs::write(&temp_file, yaml).unwrap();

        let scenario = Scenario::from_file(&temp_file).unwrap();
        assert_eq!(scenario.name, "File Test Scenario");
        assert_eq!(scenario.architecture, Architecture::Arm64);

        let _ = std::fs::remove_file(temp_file);
    }
}
