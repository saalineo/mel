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
fn test_scenario_11_guest_shell() {
    let p = scenario_path("11_guest_shell.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/11_guest_shell.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Write shell prompt and echo output to UART
    let shell_output = b"MEL-SHELL> echo HELLO MEL\nHELLO MEL\nMEL-SHELL> ";
    for &b in shell_output {
        vm.bus()
            .lock()
            .unwrap()
            .write_bytes(0x1000_0000, &[b])
            .unwrap();
    }

    let spin = vec![0x6f, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &spin).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 11 did not pass: {:?}",
        report.feedback
    );
}
