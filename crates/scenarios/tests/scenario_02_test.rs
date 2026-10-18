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
fn test_scenario_02_uart_driver() {
    let p = scenario_path("02_uart_driver.yaml");
    let yaml_str =
        fs::read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
    let scenario: Scenario =
        serde_yaml::from_str(&yaml_str).expect("Failed to parse scenarios/02_uart_driver.yaml");

    // Machine code for:
    // lui a0, 0x10000       (0x10000537)
    // auipc a1, 0           (0x00000597)
    // addi a1, a1, 24       (0x01858593) - point to string at +24
    // loop:
    // lbu a2, 0(a1)         (0x0005c603)
    // beqz a2, halt (+12)   (0x00060663)
    // sb a2, 0(a0)          (0x00c50023)
    // addi a1, a1, 1        (0x00158593)
    // j loop (-16)          (0xff1ff06f)
    // halt:
    // j halt                (0x0000006f)
    // string: "MEL_BOOT_OK\n\0"
    let mut payload = vec![
        0x37, 0x05, 0x00, 0x10, // 0: lui a0, 0x10000
        0x97, 0x05, 0x00, 0x00, // 4: auipc a1, 0
        0x93, 0x85, 0x05, 0x02, // 8: addi a1, a1, 32 (points to 4 + 32 = 36)
        0x03, 0xc6, 0x05, 0x00, // 12: lbu a2, 0(a1)
        0x63, 0x08, 0x06, 0x00, // 16: beqz a2, +16 (to 32)
        0x23, 0x00, 0xc5, 0x00, // 20: sb a2, 0(a0)
        0x93, 0x85, 0x15, 0x00, // 24: addi a1, a1, 1
        0x6f, 0xf0, 0x1f, 0xff, // 28: j loop (-16 -> to 12)
        0x6f, 0x00, 0x00, 0x00, // 32: j halt
    ];
    payload.extend_from_slice(b"MEL_BOOT_OK\n\0");

    let report =
        ScenarioValidator::run_and_validate(&scenario, &payload).expect("Execution failed");

    println!("{}", report.summary_string());
    assert!(
        report.passed,
        "Scenario 02 did not pass: {:?}",
        report.feedback
    );
}
