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
fn test_scenario_05_disk_sector_reader() {
    let p = scenario_path("05_disk_sector_reader.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario = serde_yaml::from_str(&yaml_str)
        .expect("Failed to parse scenarios/05_disk_sector_reader.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Configure MBR sector 0 with boot signature 0xAA55 at offset 510
    let mut sector0 = vec![0u8; 512];
    sector0[510] = 0x55;
    sector0[511] = 0xaa;

    // Simulate block device reading sector 0 into memory at 0x8000_5000
    vm.write_memory(0x8000_5000, &sector0).unwrap();

    // CPU execution payload (spin loop)
    let payload = vec![0x6f, 0x00, 0x00, 0x00]; // j .
    vm.write_memory(0x8000_0000, &payload).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 05 did not pass: {:?}",
        report.feedback
    );
}
