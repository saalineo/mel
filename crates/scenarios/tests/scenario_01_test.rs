use mel_scenarios::schema::Scenario;
use mel_scenarios::validator::ScenarioValidator;
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
fn test_scenario_01_first_boot() {
    let p = scenario_path("01_first_boot.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/01_first_boot.yaml");

    // Reference RISC-V payload:
    // lui sp, 0x80010
    // slli sp, sp, 32
    // srli sp, sp, 32
    // 1: j 1b
    let payload = vec![
        0x37, 0x01, 0x01, 0x80, // lui sp, 0x80010
        0x13, 0x11, 0x01, 0x02, // slli sp, sp, 32
        0x13, 0x51, 0x01, 0x02, // srli sp, sp, 32
        0x6f, 0x00, 0x00, 0x00, // j .
    ];

    let report =
        ScenarioValidator::run_and_validate(&scenario, &payload).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 01 did not pass: {:?}",
        report.feedback
    );
    assert_eq!(report.objectives_passed, report.objectives_total);
}
