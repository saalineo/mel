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
fn test_scenario_12_user_elf_loader() {
    let p = scenario_path("12_in_kernel_elf_loader.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario = serde_yaml::from_str(&yaml_str)
        .expect("Failed to parse scenarios/12_in_kernel_elf_loader.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // User payload: addi a0, zero, 77 (0x04d00513)
    let user_code = vec![0x13, 0x05, 0xd0, 0x04];
    vm.write_memory(0x8001_0000, &user_code).unwrap();

    // Set a0 = 77
    vm.write_register(RegisterId::named("a0"), 77).unwrap();

    let spin = vec![0x6f, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &spin).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 12 did not pass: {:?}",
        report.feedback
    );
}
