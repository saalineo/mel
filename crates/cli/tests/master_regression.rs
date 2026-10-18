//! Master End-to-End Regression Suite for MEL.
//!
//! Validates:
//! - All 4 CPU architectures (RISC-V 64, ARM64, x86-64, Power ISA) boot & run
//! - Save / restore snapshots across multiple architectures
//! - All 12 scenario YAML specifications and grading
//! - Sv39 and Multi-Arch MMU address translation
//! - Mini-compiler challenge AST evaluation & execution
//! - Headless Ratatui TUI debugger rendering & event dispatch
//! - Full suite of CLI commands (`machine`, `doctor`, `docs`, `test-scenarios`)

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use mel_cli::args::{DocsArgs, MachineArgs, TestScenariosArgs};
use mel_cli::tui::{App, FocusedPane};
use mel_cli::{
    build_vm_for_arch, execute_docs, execute_doctor, execute_machine, execute_test_scenarios,
};
use mel_core::{Architecture, RegisterId};
use mel_debugger::{Debugger, MmuInspector, ReplSession, Sv39Validator};
use mel_loader::load_raw_binary;
use mel_scenarios::compiler_challenge::{CompilerChallenge, Expr};
use mel_scenarios::schema::Scenario;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn test_regression_all_four_architectures_boot() {
    let architectures = [
        Architecture::Riscv64,
        Architecture::Arm64,
        Architecture::X86_64,
        Architecture::PowerIso64,
    ];

    for arch in architectures {
        let mut vm = build_vm_for_arch(arch)
            .unwrap_or_else(|e| panic!("Failed to build VM for {arch:?}: {e}"));

        let initial_pc = vm.pc().expect("Initial PC must be queryable");
        assert!(initial_pc > 0, "Initial PC must be non-zero for {arch:?}");

        // Load 4 NOP / benign instructions
        let code: Vec<u8> = match arch {
            Architecture::Riscv64 => vec![
                0x13, 0x00, 0x00, 0x00, // nop
                0x13, 0x00, 0x00, 0x00, // nop
                0x13, 0x00, 0x00, 0x00, // nop
                0x13, 0x00, 0x00, 0x00, // nop
            ],
            Architecture::Arm64 => vec![
                0x1f, 0x20, 0x03, 0xd5, // nop
                0x1f, 0x20, 0x03, 0xd5, // nop
                0x1f, 0x20, 0x03, 0xd5, // nop
                0x1f, 0x20, 0x03, 0xd5, // nop
            ],
            Architecture::X86_64 => vec![
                0x90, // nop
                0x90, // nop
                0x90, // nop
                0x90, // nop
            ],
            Architecture::PowerIso64 => vec![
                0x60, 0x00, 0x00, 0x00, // nop
                0x60, 0x00, 0x00, 0x00, // nop
                0x60, 0x00, 0x00, 0x00, // nop
                0x60, 0x00, 0x00, 0x00, // nop
            ],
        };

        load_raw_binary(&mut vm, &code, initial_pc)
            .unwrap_or_else(|e| panic!("Failed to load test code for {arch:?}: {e}"));

        let res = vm.run(4);
        assert!(res.is_ok(), "Execution failed on {arch:?}: {res:?}");
        assert!(
            vm.pc().unwrap() > initial_pc,
            "PC did not advance on {arch:?}"
        );
    }
}

#[test]
fn test_regression_save_restore_snapshots() {
    let mut vm = build_vm_for_arch(Architecture::Riscv64).expect("Failed to build RV64 VM");

    let code: [u8; 8] = [
        0x13, 0x05, 0xa0, 0x02, // addi a0, zero, 42
        0x93, 0x05, 0x80, 0x00, // addi a1, zero, 8
    ];
    let entry = Architecture::Riscv64.default_reset_vector();
    load_raw_binary(&mut vm, &code, entry).unwrap();

    // Step 1: addi a0, zero, 42
    vm.step().unwrap();
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 42);
    let pc_after_step1 = vm.pc().unwrap();

    // Take snapshot
    let snapshot = vm.save_snapshot().expect("Failed to save snapshot");

    // Step 2: addi a1, zero, 8
    vm.step().unwrap();
    assert_eq!(vm.read_register(RegisterId::named("a1")).unwrap(), 8);
    assert_ne!(vm.pc().unwrap(), pc_after_step1);

    // Restore snapshot
    vm.load_snapshot(&snapshot)
        .expect("Failed to restore snapshot");
    assert_eq!(vm.pc().unwrap(), pc_after_step1);
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 42);
    assert_eq!(vm.read_register(RegisterId::named("a1")).unwrap(), 0);
}

#[test]
fn test_regression_all_12_scenarios_and_suite() {
    let scenarios_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("scenarios").join("01_first_boot.yaml").exists())
        .map(|p| p.join("scenarios"))
        .unwrap_or_else(|| PathBuf::from("scenarios"));
    assert!(
        scenarios_dir.exists(),
        "Scenarios directory must exist in workspace root (checked {:?})",
        scenarios_dir
    );

    let expected_files = [
        "01_first_boot.yaml",
        "02_uart_driver.yaml",
        "03_timer_tick.yaml",
        "04_bootloader_handoff.yaml",
        "05_disk_sector_reader.yaml",
        "06_page_tables.yaml",
        "07_context_switch.yaml",
        "08_preemptive_scheduler.yaml",
        "09_user_mode.yaml",
        "10_fat_driver.yaml",
        "11_guest_shell.yaml",
        "12_in_kernel_elf_loader.yaml",
    ];

    for file in &expected_files {
        let p = scenarios_dir.join(file);
        assert!(p.exists(), "Scenario file '{}' missing", file);

        let content = fs::read_to_string(&p).expect("Failed to read scenario YAML");
        let scenario: Scenario =
            serde_yaml::from_str(&content).expect("Failed to deserialize scenario YAML");

        assert!(!scenario.name.is_empty(), "Scenario name cannot be empty");
        assert!(
            !scenario.objectives.is_empty(),
            "Scenario must have at least one objective"
        );
    }

    // Run test_scenarios CLI logic
    let mut out = Vec::new();
    let args = TestScenariosArgs {
        dir: scenarios_dir.to_str().unwrap().to_string(),
        filter: None,
    };
    execute_test_scenarios(&args, &mut out).expect("Scenario runner failed");
    let out_str = String::from_utf8_lossy(&out);
    assert!(out_str.contains("12/12 scenarios passed (100.0%)"));
}

#[test]
fn test_regression_mmu_sv39_and_multi_arch() {
    let root_ppn = 0x8000_1000u64 >> 12;
    let satp = (8u64 << 60) | root_ppn;
    let pte_val: u64 = (0x8000_0000u64 >> 2) | 0x0F; // V | R | W | X

    let mock_read = move |addr: u64| -> Result<u64, String> {
        if addr == 0x8000_1000 {
            Ok(pte_val)
        } else {
            Err(format!("Unmapped phys addr {addr:#x}"))
        }
    };

    // 1. Sv39 validation
    let trans = Sv39Validator::walk(satp, 0x0000_0000, mock_read);
    assert!(trans.is_ok(), "Sv39 1GB translation must succeed");

    // 2. Multi-arch MMU inspector
    let trans_mmu = MmuInspector::translate(Architecture::Riscv64, satp, 0x0000_0000, mock_read);
    assert!(trans_mmu.is_ok(), "Multi-arch translation must succeed");
}

#[test]
fn test_regression_compiler_challenge_codegen_and_eval() {
    // AST: (10 + 20) * 2 = 60
    let ast = Expr::Mul(
        Box::new(Expr::Add(
            Box::new(Expr::Literal(10)),
            Box::new(Expr::Literal(20)),
        )),
        Box::new(Expr::Literal(2)),
    );
    assert_eq!(ast.eval(), 60);

    // Compile and run in VM
    let vm_res = CompilerChallenge::run_in_vm(&ast).expect("Compile and run must succeed");
    assert_eq!(vm_res, 60);
}

#[test]
fn test_regression_headless_tui_render_and_events() {
    let mut vm = build_vm_for_arch(Architecture::Riscv64).expect("Failed to build RV64 VM");
    let code: [u8; 8] = [
        0x13, 0x05, 0xa0, 0x00, // addi a0, zero, 10
        0x73, 0x00, 0x10, 0x00, // ebreak
    ];
    let entry = Architecture::Riscv64.default_reset_vector();
    load_raw_binary(&mut vm, &code, entry).unwrap();

    let debugger = Debugger::new(vm);
    let repl = ReplSession::new(debugger);
    let mut app = App::new(repl);

    // Hotkey F10 (Step)
    app.handle_key(KeyEvent {
        code: KeyCode::F(10),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(app.repl.debugger().vm().pc().unwrap(), entry + 4);

    // Hotkey F9 (Toggle Breakpoint)
    app.handle_key(KeyEvent {
        code: KeyCode::F(9),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert!(app.disasm_state.breakpoints.contains(&(entry + 4)));

    // Focus switching
    app.handle_key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(app.active_pane, FocusedPane::Registers);

    // Render headless with TestBackend
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);
    assert!(content.contains("[1] CPU Registers"));
    assert!(content.contains("[2] Disassembly"));
    assert!(content.contains("[3] Memory"));
    assert!(content.contains("[4] UART Interactive Terminal"));
    assert!(content.contains("[5] Command Bar & Status"));
}

#[test]
fn test_regression_cli_commands() {
    // 1. Machine list
    let mut out = Vec::new();
    let machine_args = MachineArgs { command: None };
    execute_machine(&machine_args, &mut out).expect("execute_machine failed");
    let out_str = String::from_utf8_lossy(&out);
    assert!(out_str.contains("Supported CPU Architectures"));
    assert!(out_str.contains("riscv64"));
    assert!(out_str.contains("arm64"));
    assert!(out_str.contains("x86_64"));
    assert!(out_str.contains("powerpc64"));

    // 2. Doctor
    let mut out = Vec::new();
    execute_doctor(&mut out).expect("execute_doctor failed");
    let out_str = String::from_utf8_lossy(&out);
    assert!(out_str.contains("MEL Host Toolchain Doctor"));

    // 3. Docs export
    let out_dir = std::env::temp_dir().join("mel_regression_docs");
    let mut out = Vec::new();
    let doc_args = DocsArgs {
        arch: None,
        out: out_dir.to_str().unwrap().to_string(),
    };
    execute_docs(&doc_args, &mut out).expect("execute_docs failed");
    assert!(out_dir.join("riscv64_hardware.md").exists());
    assert!(out_dir.join("arm64_hardware.md").exists());
    assert!(out_dir.join("x86_64_hardware.md").exists());
    assert!(out_dir.join("powerpc64_hardware.md").exists());
    let _ = fs::remove_dir_all(&out_dir);
}
