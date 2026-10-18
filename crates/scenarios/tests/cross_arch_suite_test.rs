use mel_core::{Architecture, RegisterId};
use mel_loader::load_raw_binary_from_file;
use mel_vm::factory::build_vm_for_arch;
use std::path::PathBuf;

struct CrossArchCase {
    arch: Architecture,
    rel_path: &'static str,
    entry_point: u64,
    sp_register: &'static str,
    expected_sp: u64,
    expected_uart: &'static str,
}

const CASES: [CrossArchCase; 4] = [
    CrossArchCase {
        arch: Architecture::Riscv64,
        rel_path: "examples/riscv64/hello_uart/boot.bin",
        entry_point: 0x8000_0000,
        sp_register: "sp",
        expected_sp: 0x8001_0000,
        expected_uart: "Hello MEL RISC-V!\n",
    },
    CrossArchCase {
        arch: Architecture::Arm64,
        rel_path: "examples/arm64/hello_uart/boot.bin",
        entry_point: 0x4000_0000,
        sp_register: "sp",
        expected_sp: 0x4001_0000,
        expected_uart: "Hello MEL ARM64!\n",
    },
    CrossArchCase {
        arch: Architecture::X86_64,
        rel_path: "examples/x86_64/hello_uart/boot.bin",
        entry_point: 0x0010_0000,
        sp_register: "rsp",
        expected_sp: 0x0020_0000,
        expected_uart: "Hello MEL x86-64!\n",
    },
    CrossArchCase {
        arch: Architecture::PowerIso64,
        rel_path: "examples/powerpc64/hello_uart/boot.bin",
        entry_point: 0x0000_0100,
        sp_register: "r1",
        expected_sp: 0x0001_0000,
        expected_uart: "Hello MEL PowerPC!\n",
    },
];

#[test]
fn test_cross_arch_portability_suite() {
    let mut root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root_path.pop(); // crates
    root_path.pop(); // root

    for case in &CASES {
        let bin_path = root_path.join(case.rel_path);
        assert!(bin_path.exists(), "Payload fixture missing: {:?}", bin_path);

        let mut vm = build_vm_for_arch(case.arch)
            .unwrap_or_else(|e| panic!("Failed to build VM for {:?}: {e}", case.arch));

        load_raw_binary_from_file(&mut vm, &bin_path, case.entry_point)
            .unwrap_or_else(|e| panic!("Failed to load payload for {:?}: {e}", case.arch));

        // Run machine
        let step_res = vm.run(20_000).expect("VM run error");
        println!("[{:?}] Execution completed: {:?}", case.arch, step_res);

        // Verify stack pointer initialized
        let sp_val = vm
            .read_register(RegisterId::named(case.sp_register))
            .expect("Failed to read stack pointer");
        assert_eq!(
            sp_val, case.expected_sp,
            "Stack pointer mismatch for {:?}",
            case.arch
        );

        // Verify UART output
        let uart_arc = vm.uart().expect("No UART mapped");
        let uart = uart_arc.lock().unwrap();
        let emitted = String::from_utf8_lossy(uart.peek_output());
        assert!(
            emitted.contains(case.expected_uart),
            "Expected UART output containing {:?}, got {:?}",
            case.expected_uart,
            emitted
        );
    }
}
