use mel_scenarios::schema::Scenario;
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
fn test_suite_scenarios_01_to_05() {
    let scenario_files = [
        "01_first_boot.yaml",
        "02_uart_driver.yaml",
        "03_timer_tick.yaml",
        "04_bootloader_handoff.yaml",
        "05_disk_sector_reader.yaml",
    ];

    let mut passed_scenarios = 0;

    for file in &scenario_files {
        let p = scenario_path(file);
        assert!(p.exists(), "Scenario file does not exist: {}", p.display());

        let yaml_str = fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", p.display()));
        let scenario: Scenario = serde_yaml::from_str(&yaml_str)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", p.display()));

        println!(
            "Loaded scenario: {} ({})",
            scenario.name,
            scenario.id.as_deref().unwrap_or("")
        );
        assert_eq!(scenario.architecture, mel_core::Architecture::Riscv64);
        assert!(
            !scenario.objectives.is_empty(),
            "Scenario {} has no objectives",
            scenario.name
        );
        passed_scenarios += 1;
    }

    assert_eq!(
        passed_scenarios, 5,
        "Expected all 5 scenario specifications to validate"
    );
}
