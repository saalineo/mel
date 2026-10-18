//! End-to-End Master Headless Execution & Determinism Test Suite for MEL Phase 1.

use std::collections::HashMap;
use std::path::PathBuf;

use mel_core::RegisterId;
use mel_cpu::traits::CpuStepResult;
use mel_debugger::{DebugStopReason, Debugger, WatchpointKind};
use mel_loader::load_raw_binary_from_file;
use mel_vm::{create_basic_riscv64_vm, VirtualMachine};

fn get_boot_bin_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates
    path.pop(); // workspace root
    path.join("examples/riscv64/hello_uart/boot.bin")
}

/// Helper function building and initializing a RISC-V VM with `boot.bin` loaded at 0x8000_0000.
fn setup_riscv64_test_vm() -> VirtualMachine {
    let bin_path = get_boot_bin_path();
    let mut vm =
        create_basic_riscv64_vm().expect("Failed to build basic RISC-V 64 virtual machine");
    load_raw_binary_from_file(&mut vm, &bin_path, 0x8000_0000)
        .expect("Failed to load boot.bin into virtual machine memory");
    vm
}

#[test]
fn test_e2e_headless_boot_and_uart_output() {
    let mut vm = setup_riscv64_test_vm();

    // Verify initial PC is reset/entry point
    assert_eq!(vm.pc().unwrap(), 0x8000_0000);

    // Execute 10,000 cycles headless
    let run_res = vm
        .run(10_000)
        .expect("VM run failed during headless execution");
    assert_eq!(run_res, CpuStepResult::Ok { cycles: 10_000 });

    // Assert stack pointer is initialized to 0x80010000
    let sp = vm.read_register(RegisterId::named("sp")).unwrap();
    assert_eq!(sp, 0x8001_0000);

    // Assert UART console output exactly matches expected string
    let uart_out = vm
        .uart()
        .expect("UART peripheral must be present")
        .lock()
        .expect("UART lock poisoned")
        .drain_output();

    let output_str = String::from_utf8(uart_out).expect("UART output must be valid UTF-8");
    assert_eq!(output_str, "Hello MEL RISC-V!\n");
}

#[test]
fn test_e2e_headless_10_runs_determinism() {
    let mut reference_registers: Option<HashMap<String, u64>> = None;
    let mut reference_pc: Option<u64> = None;
    let mut reference_output: Option<String> = None;

    let registers_to_check = [
        "pc", "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12",
        "x13", "x14", "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25",
        "x26", "x27", "x28", "x29", "x30", "x31",
    ];

    for run_idx in 1..=10 {
        let mut vm = setup_riscv64_test_vm();

        let run_res = vm.run(10_000).expect("Deterministic run failed");
        assert_eq!(run_res, CpuStepResult::Ok { cycles: 10_000 });

        let current_pc = vm.pc().unwrap();
        let uart_bytes = vm.uart().unwrap().lock().unwrap().drain_output();
        let current_output = String::from_utf8(uart_bytes).unwrap();

        let mut current_registers = HashMap::new();
        for &reg_name in &registers_to_check {
            let val = if reg_name == "pc" {
                current_pc
            } else {
                vm.read_register(RegisterId::named(reg_name)).unwrap()
            };
            current_registers.insert(reg_name.to_string(), val);
        }

        if run_idx == 1 {
            reference_registers = Some(current_registers);
            reference_pc = Some(current_pc);
            reference_output = Some(current_output);
        } else {
            // Verify bit-for-bit determinism against run #1
            assert_eq!(
                Some(current_pc),
                reference_pc,
                "Non-deterministic PC on run #{run_idx}"
            );
            assert_eq!(
                Some(&current_output),
                reference_output.as_ref(),
                "Non-deterministic UART output on run #{run_idx}"
            );
            assert_eq!(
                Some(&current_registers),
                reference_registers.as_ref(),
                "Non-deterministic register state on run #{run_idx}"
            );
        }
    }
}

#[test]
fn test_e2e_boot_rom_to_ram_reset_handoff() {
    let mut vm = setup_riscv64_test_vm();

    // Point PC to Boot ROM reset vector (0x0000_1000)
    vm.set_pc(0x0000_1000).unwrap();

    // Step through Boot ROM reset jump sequence (auipc -> ld -> jr)
    vm.step().unwrap();
    vm.step().unwrap();
    vm.step().unwrap();

    // Verify PC landed squarely on RAM base (0x8000_0000)
    assert_eq!(vm.pc().unwrap(), 0x8000_0000);

    // Continue running payload
    let s_run = vm.run(5_000).unwrap();
    assert_eq!(s_run, CpuStepResult::Ok { cycles: 5_000 });

    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(String::from_utf8_lossy(&uart_out), "Hello MEL RISC-V!\n");
}

#[test]
fn test_e2e_headless_save_state_snapshot_parity() {
    let mut vm = setup_riscv64_test_vm();

    // Step 5 instructions into the payload
    for _ in 0..5 {
        vm.step().unwrap();
    }
    let pc_mid = vm.pc().unwrap();

    // Save snapshot
    let snapshot = vm.save_snapshot().unwrap();

    // Finish executing to 10,000 cycles
    vm.run(10_000).unwrap();
    let pc_final_1 = vm.pc().unwrap();
    let uart_out_1 = vm.uart().unwrap().lock().unwrap().drain_output();

    // Restore from snapshot
    vm.load_snapshot(&snapshot).unwrap();
    assert_eq!(vm.pc().unwrap(), pc_mid);

    // Finish executing again from restored state
    vm.run(10_000).unwrap();
    let pc_final_2 = vm.pc().unwrap();
    let uart_out_2 = vm.uart().unwrap().lock().unwrap().drain_output();

    assert_eq!(pc_final_1, pc_final_2);
    assert_eq!(uart_out_1, uart_out_2);
}

#[test]
fn test_e2e_debugger_breakpoints_stepping_and_disassembly() {
    let vm = setup_riscv64_test_vm();
    let mut dbg = Debugger::new(vm);

    // Disassemble initial instruction at entry
    let insns = dbg.disassemble_current(3).unwrap();
    assert_eq!(insns.len(), 3);
    assert_eq!(insns[0].mnemonic, "lui");
    assert_eq!(insns[0].address, 0x8000_0000);

    // Set breakpoint on UART write instruction (0x8000_0020)
    let bp_uart_write = dbg.add_breakpoint(0x8000_0020);

    // Set write watchpoint on UART THR (0x1000_0000, 1 byte)
    let _wp_thr = dbg.add_watchpoint(0x1000_0000, 1, WatchpointKind::Write);

    // Continue until breakpoint hits
    let stop1 = dbg.continue_execution(Some(100)).unwrap();
    assert_eq!(
        stop1,
        DebugStopReason::Breakpoint {
            id: bp_uart_write,
            address: 0x8000_0020,
            hit_count: 1
        }
    );
    assert_eq!(dbg.pc().unwrap(), 0x8000_0020);

    // Verify character in register a2 is 'H' (ASCII 72)
    let a2_val = dbg.read_register(RegisterId::named("a2")).unwrap();
    assert_eq!(a2_val, b'H' as u64);

    // Step over UART write (sb a2, 0(a0))
    let s_step = dbg.step().unwrap();
    assert_eq!(s_step, DebugStopReason::Step);

    let uart_buf = dbg.vm().uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(uart_buf, b"H");

    // Remove breakpoint and continue to completion
    dbg.remove_breakpoint(bp_uart_write).unwrap();
    let final_stop = dbg.continue_execution(Some(10_000)).unwrap();
    assert!(matches!(
        final_stop,
        DebugStopReason::CycleLimitReached { .. }
    ));

    let full_uart = dbg.vm().uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(String::from_utf8_lossy(&full_uart), "ello MEL RISC-V!\n");
}
