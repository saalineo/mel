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
fn test_scenario_06_page_tables() {
    let p = scenario_path("06_page_tables.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/06_page_tables.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Write Page Table Entry at 0x8000_2010 (PPN=0x80000, Flags=D|A|X|W|R|V)
    let pte_bytes = vec![0xcf, 0x00, 0x02, 0x20, 0x00, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_2010, &pte_bytes).unwrap();

    // Set register a0 = 0x8000_0000
    vm.write_register(RegisterId::named("a0"), 0x8000_0000)
        .unwrap();

    // Execution code: spin loop
    let code = vec![0x6f, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 06 did not pass: {:?}",
        report.feedback
    );
}
