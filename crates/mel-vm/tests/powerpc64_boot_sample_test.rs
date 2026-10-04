use mel_core::RegisterId;
use mel_loader::load_raw_binary_from_file;
use mel_vm::create_basic_powerpc64_vm;
use std::path::PathBuf;

/// Canonical byte sequence for Power ISA `boot.bin`.
pub const BOOT_BIN_BYTES: [u8; 72] = [
    // 0x00 (0x100): lis r1, 1
    0x3c, 0x20, 0x00, 0x01,
    // 0x04 (0x104): ori r1, r1, 0 (r1 = 0x00010000, stack pointer)
    0x60, 0x21, 0x00, 0x00,
    // 0x08 (0x108): lis r3, 0x1000
    0x3c, 0x60, 0x10, 0x00,
    // 0x0C (0x10C): ori r3, r3, 0 (r3 = 0x10000000, UART base)
    0x60, 0x63, 0x00, 0x00,
    // 0x10 (0x110): lis r4, 0
    0x3c, 0x80, 0x00, 0x00,
    // 0x14 (0x114): addi r4, r4, 0x134 (r4 = 0x00000134, msg pointer)
    0x38, 0x84, 0x01, 0x34,
    // 0x18 (0x118): print_loop: lbz r5, 0(r4)
    0x88, 0xa4, 0x00, 0x00,
    // 0x1C (0x11C): cmpwi r5, 0
    0x2c, 0x05, 0x00, 0x00,
    // 0x20 (0x120): beq halt_loop (offset 0x130)
    0x41, 0x82, 0x00, 0x10,
    // 0x24 (0x124): stb r5, 0(r3) (write char to UART THR)
    0x98, 0xa3, 0x00, 0x00,
    // 0x28 (0x128): addi r4, r4, 1
    0x38, 0x84, 0x00, 0x01,
    // 0x2C (0x12C): b print_loop (offset 0x118)
    0x4b, 0xff, 0xff, 0xec,
    // 0x30 (0x130): halt_loop: b halt_loop
    0x48, 0x00, 0x00, 0x00,
    // 0x34 (0x134): msg: "Hello MEL PowerPC!\n\0" (20 bytes)
    0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x4d, 0x45,
    0x4c, 0x20, 0x50, 0x6f, 0x77, 0x65, 0x72, 0x50,
    0x43, 0x21, 0x0a, 0x00,
];

#[test]
fn test_powerpc64_boot_sample_fixture_execution() {
    let mut fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_path.pop(); // crates
    fixture_path.pop(); // root
    let bin_path = fixture_path.join("examples/powerpc64/hello_uart/boot.bin");

    // Ensure fixture file exists with canonical bytes
    std::fs::write(&bin_path, BOOT_BIN_BYTES).unwrap();

    let mut vm = create_basic_powerpc64_vm().unwrap();
    let entry_addr = 0x0000_0100;

    // Load binary payload from file at 0x0000_0100
    load_raw_binary_from_file(&mut vm, &bin_path, entry_addr).unwrap();

    // Verify PC initialized to entry address
    assert_eq!(vm.pc().unwrap(), entry_addr);

    // Run execution for up to 500 cycles (sufficient to print string and enter spin loop)
    let run_res = vm.run(500).unwrap();
    assert!(matches!(run_res, mel_cpu::traits::CpuStepResult::Ok { .. }));

    // Verify Stack Pointer (r1) was initialized
    let sp = vm.read_register(RegisterId::named("r1")).unwrap();
    assert_eq!(sp, 0x0001_0000);

    // Verify UART console output captured exact string
    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    let output_str = String::from_utf8_lossy(&uart_out);

    assert_eq!(output_str, "Hello MEL PowerPC!\n");
}
