use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use mel_cli::tui::{App, FocusedPane};
use mel_core::Architecture;
use mel_debugger::{Debugger, ReplSession};
use mel_loader::load_raw_binary;
use mel_vm::build_vm_for_arch;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn mk_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn create_test_app() -> App {
    let arch = Architecture::Riscv64;
    let mut vm = build_vm_for_arch(arch).expect("Failed to build RV64 VM");

    // Sample payload:
    // addi x1, x0, 10 (0x00a00093)
    // addi x2, x0, 20 (0x01400113)
    // add  x3, x1, x2 (0x002081b3)
    // wfi / ebreak    (0x10500073 / 0x00100073)
    let code: [u8; 16] = [
        0x93, 0x00, 0xa0, 0x00, // addi x1, x0, 10
        0x13, 0x01, 0x40, 0x01, // addi x2, x0, 20
        0xb3, 0x81, 0x20, 0x00, // add x3, x1, x2
        0x73, 0x00, 0x10, 0x00, // ebreak
    ];

    let entry = arch.default_reset_vector();
    load_raw_binary(&mut vm, &code, entry).expect("Failed to load binary");

    let debugger = Debugger::new(vm);
    let repl = ReplSession::new(debugger);
    App::new(repl)
}

#[test]
fn test_app_initialization() {
    let app = create_test_app();
    assert_eq!(app.active_pane, FocusedPane::CommandBar);
    assert!(!app.is_running);
    assert!(!app.should_quit);
    assert_eq!(app.command_input, "");

    let pc = app.repl.debugger().vm().pc().unwrap();
    assert_eq!(pc, Architecture::Riscv64.default_reset_vector());
    assert_eq!(app.disasm_state.pc, pc);
}

#[test]
fn test_step_hotkey_f10() {
    let mut app = create_test_app();
    let initial_pc = app.repl.debugger().vm().pc().unwrap();

    // Step 1: F10
    app.handle_key(mk_key(KeyCode::F(10), KeyModifiers::NONE));

    let new_pc = app.repl.debugger().vm().pc().unwrap();
    assert_eq!(new_pc, initial_pc + 4);
    assert!(app.status_message.contains("[Step]"));

    // Check register x1 is updated
    let x1 = app
        .repl
        .debugger()
        .vm()
        .read_register(mel_core::RegisterId::named("x1"))
        .unwrap();
    assert_eq!(x1, 10);
}

#[test]
fn test_breakpoint_toggle_hotkey_f9() {
    let mut app = create_test_app();
    let pc = app.repl.debugger().vm().pc().unwrap();

    assert!(app
        .repl
        .debugger()
        .breakpoints()
        .get_breakpoint_at(pc)
        .is_none());

    // Toggle on: F9
    app.handle_key(mk_key(KeyCode::F(9), KeyModifiers::NONE));
    assert!(app
        .repl
        .debugger()
        .breakpoints()
        .get_breakpoint_at(pc)
        .is_some());
    assert!(app.status_message.contains("Set breakpoint"));
    assert!(app.disasm_state.breakpoints.contains(&pc));

    // Toggle off: F9
    app.handle_key(mk_key(KeyCode::F(9), KeyModifiers::NONE));
    assert!(app
        .repl
        .debugger()
        .breakpoints()
        .get_breakpoint_at(pc)
        .is_none());
    assert!(app.status_message.contains("Removed"));
    assert!(!app.disasm_state.breakpoints.contains(&pc));
}

#[test]
fn test_continue_and_pause_f5_and_esc() {
    let mut app = create_test_app();
    assert!(!app.is_running);

    // F5: Resume
    app.handle_key(mk_key(KeyCode::F(5), KeyModifiers::NONE));
    assert!(app.is_running);
    assert!(app.status_message.contains("Execution resumed"));

    // ESC: Pause
    app.handle_key(mk_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.is_running);
    assert!(app.status_message.contains("paused"));
}

#[test]
fn test_pane_focus_cycling_tab_and_backtab() {
    let mut app = create_test_app();
    assert_eq!(app.active_pane, FocusedPane::CommandBar);

    // Tab -> Registers
    app.handle_key(mk_key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::Registers);

    // Tab -> Disassembly
    app.handle_key(mk_key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::Disassembly);

    // Tab -> Memory
    app.handle_key(mk_key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::Memory);

    // Tab -> Uart
    app.handle_key(mk_key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::Uart);

    // Tab -> CommandBar
    app.handle_key(mk_key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::CommandBar);

    // BackTab -> Uart
    app.handle_key(mk_key(KeyCode::BackTab, KeyModifiers::NONE));
    assert_eq!(app.active_pane, FocusedPane::Uart);
}

#[test]
fn test_command_bar_repl_execution_and_history() {
    let mut app = create_test_app();
    let initial_pc = app.repl.debugger().vm().pc().unwrap();

    // Type "step"
    for c in "step".chars() {
        app.handle_key(mk_key(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert_eq!(app.command_input, "step");

    // Enter
    app.handle_key(mk_key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.command_input, "");
    assert_eq!(app.repl.debugger().vm().pc().unwrap(), initial_pc + 4);

    // History navigation with Up
    app.handle_key(mk_key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.command_input, "step");

    // Down to clear
    app.handle_key(mk_key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.command_input, "");

    // Quit command
    for c in "quit".chars() {
        app.handle_key(mk_key(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key(mk_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.should_quit);
}

#[test]
fn test_non_blocking_tick_loop_with_breakpoint() {
    let mut app = create_test_app();
    let entry = app.repl.debugger().vm().pc().unwrap();
    let target_bp = entry + 8; // add x3, x1, x2

    // Set breakpoint at target_bp
    app.repl
        .debugger_mut()
        .breakpoints_mut()
        .add_breakpoint(target_bp);

    // Resume execution
    app.handle_key(mk_key(KeyCode::F(5), KeyModifiers::NONE));
    assert!(app.is_running);

    // Advance background tick loop
    app.tick(100);

    // Execution should have stopped at breakpoint
    assert!(!app.is_running);
    let current_pc = app.repl.debugger().vm().pc().unwrap();
    assert_eq!(current_pc, target_bp);
    assert!(app.status_message.contains("[Breakpoint]"));

    // Registers x1 and x2 should be populated
    assert_eq!(
        app.repl
            .debugger()
            .vm()
            .read_register(mel_core::RegisterId::named("x1"))
            .unwrap(),
        10
    );
    assert_eq!(
        app.repl
            .debugger()
            .vm()
            .read_register(mel_core::RegisterId::named("x2"))
            .unwrap(),
        20
    );
}

#[test]
fn test_headless_ratatui_render() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = create_test_app();
    app.command_input = "help".to_string();

    terminal.draw(|f| app.render(f)).unwrap();

    let buffer = terminal.backend().buffer();
    let content = format!("{:?}", buffer);

    assert!(content.contains("[1] CPU Registers"));
    assert!(content.contains("[2] Disassembly"));
    assert!(content.contains("[3] Memory"));
    assert!(content.contains("[4] UART Interactive Terminal"));
    assert!(content.contains("[5] Command Bar & Status"));
    assert!(content.contains("(dbg)> help"));
}
