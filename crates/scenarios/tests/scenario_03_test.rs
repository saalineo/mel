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
fn test_scenario_03_timer_tick() {
    let p = scenario_path("03_timer_tick.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/03_timer_tick.yaml");

    let mut vm = build_vm_for_arch(scenario.architecture).expect("Failed to create VM");

    // Write NOP sled into memory at 0x8000_0000
    let mut payload = Vec::new();
    for _ in 0..20 {
        payload.extend_from_slice(&[0x13, 0x00, 0x00, 0x00]); // nop
    }
    vm.write_memory(0x8000_0000, &payload)
        .expect("write_memory failed");
    vm.set_pc(0x8000_0000).expect("set_pc failed");

    // Configure Trap Vector at 0x8000_1000
    vm.write_register(RegisterId::named("mtvec"), 0x8000_1000)
        .unwrap();
    // Enable Machine global interrupts (MIE = bit 3) and timer interrupts (MTIE = bit 7)
    vm.write_register(RegisterId::named("mstatus"), 1 << 3)
        .unwrap();
    vm.write_register(RegisterId::named("mie"), 1 << 7).unwrap();

    // Configure CLINT: mtime = 0, mtimecmp = 5
    if let Some(clint) = vm.clint() {
        let mut c = clint.lock().unwrap();
        c.set_mtime(0);
        c.set_mtimecmp(5);
    }

    let report =
        ScenarioValidator::run_and_validate_vm(&scenario, &mut vm).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 03 did not pass: {:?}",
        report.feedback
    );
}
