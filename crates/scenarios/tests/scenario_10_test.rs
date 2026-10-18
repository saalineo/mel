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
fn test_scenario_10_fat_driver() {
    let p = scenario_path("10_fat_driver.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/10_fat_driver.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Init.bin payload: addi a0, zero, 100 (0x06400513)
    let init_bin = vec![0x13, 0x05, 0x40, 0x06];
    vm.write_memory(0x8000_6000, &init_bin).unwrap();

    // Set a0 = 100
    vm.write_register(RegisterId::named("a0"), 100).unwrap();

    let spin = vec![0x6f, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &spin).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 10 did not pass: {:?}",
        report.feedback
    );
}
