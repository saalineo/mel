use mel_core::{Architecture, RegisterId};
use mel_loader::load_raw_binary_from_file;
use mel_vm::factory::build_vm_for_arch;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

struct ArchBootCase {
    arch: Architecture,
    rel_path: &'static str,
    entry_point: u64,
    sp_register: &'static str,
    expected_sp: u64,
    expected_uart: &'static str,
}

const ARCH_CASES: [ArchBootCase; 4] = [
    ArchBootCase {
        arch: Architecture::Riscv64,
        rel_path: "examples/riscv64/hello_uart/boot.bin",
        entry_point: 0x8000_0000,
        sp_register: "sp",
        expected_sp: 0x8001_0000,
        expected_uart: "Hello MEL RISC-V!\n",
    },
    ArchBootCase {
        arch: Architecture::Arm64,
        rel_path: "examples/arm64/hello_uart/boot.bin",
        entry_point: 0x4000_0000,
        sp_register: "sp",
        expected_sp: 0x4001_0000,
        expected_uart: "Hello MEL ARM64!\n",
    },
    ArchBootCase {
        arch: Architecture::X86_64,
        rel_path: "examples/x86_64/hello_uart/boot.bin",
        entry_point: 0x0010_0000,
        sp_register: "rsp",
        expected_sp: 0x0020_0000,
        expected_uart: "Hello MEL x86-64!\n",
    },
    ArchBootCase {
        arch: Architecture::PowerIso64,
        rel_path: "examples/powerpc64/hello_uart/boot.bin",
        entry_point: 0x0000_0100,
        sp_register: "r1",
        expected_sp: 0x0001_0000,
        expected_uart: "Hello MEL PowerPC!\n",
    },
];

fn run_arch_case(case: &ArchBootCase) {
    let mut root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root_path.pop(); // crates
    root_path.pop(); // root
    let bin_path = root_path.join(case.rel_path);

    assert!(
        bin_path.exists(),
        "Payload fixture must exist: {:?}",
        bin_path
    );

    let mut vm = build_vm_for_arch(case.arch)
        .unwrap_or_else(|e| panic!("Failed to build VM for {:?}: {e}", case.arch));

    load_raw_binary_from_file(&mut vm, &bin_path, case.entry_point)
        .unwrap_or_else(|e| panic!("Failed to load payload for {:?}: {e}", case.arch));

    assert_eq!(vm.pc().unwrap(), case.entry_point);

    let run_res = vm.run(500).unwrap();
    assert!(
        matches!(run_res, mel_cpu::traits::CpuStepResult::Ok { .. }),
        "Expected Ok run result for {:?}",
        case.arch
    );

    let sp = vm
        .read_register(RegisterId::named(case.sp_register))
        .unwrap();
    assert_eq!(
        sp, case.expected_sp,
        "Stack pointer mismatch for {:?}",
        case.arch
    );

    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    let output_str = String::from_utf8_lossy(&uart_out);
    assert_eq!(
        output_str, case.expected_uart,
        "UART output mismatch for {:?}",
        case.arch
    );
}

#[test]
fn test_multi_arch_sequential_matrix_execution() {
    for case in &ARCH_CASES {
        run_arch_case(case);
    }
}

#[test]
fn test_multi_arch_concurrent_matrix_execution() {
    let barrier = Arc::new(Barrier::new(ARCH_CASES.len()));
    let mut handles = Vec::new();

    for case in ARCH_CASES {
        let b = barrier.clone();
        let handle = thread::spawn(move || {
            b.wait();
            run_arch_case(&case);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
