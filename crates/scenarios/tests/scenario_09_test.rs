use mel_core::RegisterId;
use mel_scenarios::schema::Scenario;
use mel_scenarios::validator::ScenarioValidator;
use mel_vm::factory::build_vm_for_arch;
use std::fs;
use std::path::{Path, PathBuf};

fn scenario_path(file: &str) -> PathBuf {
    if Path::new("scenarios").join(file).exists() {
        Path::new("scenarios").join(file)
    } else {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        PathBuf::from(manifest).join("../../scenarios").join(file)
    }
}

#[test]
fn test_scenario_09_user_mode() {
    let p = scenario_path("09_user_mode.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/09_user_mode.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Emit "USER_OK\n" to UART via MMIO write through system bus
    for &b in b"USER_OK\n" {
        vm.bus()
            .lock()
            .unwrap()
            .write_bytes(0x1000_0000, &[b])
            .unwrap();
    }

    // Set a0 = 0 (syscall success)
    vm.write_register(RegisterId::named("a0"), 0).unwrap();

    let spin = vec![0x6f, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &spin).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 09 did not pass: {:?}",
        report.feedback
    );
}
