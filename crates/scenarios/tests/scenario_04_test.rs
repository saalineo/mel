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
fn test_scenario_04_bootloader_handoff() {
    let p = scenario_path("04_bootloader_handoff.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario = serde_yaml::from_str(&yaml_str)
        .expect("Failed to parse scenarios/04_bootloader_handoff.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Stage 2 payload:
    // addi a0, zero, 42 (0x02a00513)
    // 1: j 1b           (0x0000006f)
    let stage2_code = vec![
        0x13, 0x05, 0xa0, 0x02, // addi a0, zero, 42
        0x6f, 0x00, 0x00, 0x00, // j .
    ];

    // Write source payload at 0x8000_0100
    vm.write_memory(0x8000_0100, &stage2_code).unwrap();

    // Handoff to 0x8000_4000
    vm.write_memory(0x8000_4000, &stage2_code).unwrap();
    vm.set_pc(0x8000_4000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 04 did not pass: {:?}",
        report.feedback
    );
}
