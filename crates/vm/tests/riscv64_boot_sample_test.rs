use mel_core::RegisterId;
use mel_loader::load_raw_binary_from_file;
use mel_vm::create_basic_riscv64_vm;
use std::path::PathBuf;

/// Canonical byte sequence for `boot.bin`.
pub const BOOT_BIN_BYTES: [u8; 67] = [
    // 0x00: lui sp, 0x80010
    0x37, 0x01, 0x01, 0x80, // 0x04: slli sp, sp, 32
    0x13, 0x11, 0x01, 0x02, // 0x08: srli sp, sp, 32 (sp = 0x0000000080010000)
    0x13, 0x51, 0x01, 0x02, // 0x0C: lui a0, 0x10000 (a0 = 0x10000000, UART base)
    0x37, 0x05, 0x00, 0x10, // 0x10: auipc a1, 0
    0x97, 0x05, 0x00, 0x00, // 0x14: addi a1, a1, 32 (a1 = pointer to msg at offset 0x30)
    0x93, 0x85, 0x05, 0x02, // 0x18: print_loop: lbu a2, 0(a1)
    0x03, 0xc6, 0x05, 0x00, // 0x1C: beqz a2, 16 (branch to halt_loop at offset 0x2C)
    0x63, 0x08, 0x06, 0x00, // 0x20: sb a2, 0(a0) (write char to UART THR)
    0x23, 0x00, 0xc5, 0x00, // 0x24: addi a1, a1, 1 (advance string pointer)
    0x93, 0x85, 0x15, 0x00, // 0x28: jal x0, -16 (jump back to print_loop at offset 0x18)
    0x6f, 0xf0, 0x1f, 0xff, // 0x2C: halt_loop: jal x0, 0 (spin loop)
    0x6f, 0x00, 0x00, 0x00, // 0x30: msg: "Hello MEL RISC-V!\n\0" (19 bytes)
    0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x4d, 0x45, 0x4c, 0x20, 0x52, 0x49, 0x53, 0x43, 0x2d, 0x56,
    0x21, 0x0a, 0x00,
];

#[test]
fn test_riscv64_boot_sample_fixture_execution() {
    let mut fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_path.pop(); // crates
    fixture_path.pop(); // root
    let bin_path = fixture_path.join("examples/riscv64/hello_uart/boot.bin");

    // Ensure fixture file exists with canonical bytes
    std::fs::write(&bin_path, BOOT_BIN_BYTES).unwrap();

    let mut vm = create_basic_riscv64_vm().unwrap();
    let ram_base = 0x8000_0000;

    // Load binary payload from file
    load_raw_binary_from_file(&mut vm, &bin_path, ram_base).unwrap();

    // Verify PC initialized to entry address
    assert_eq!(vm.pc().unwrap(), ram_base);

    // Run execution for up to 500 cycles (sufficient to print string and enter spin loop)
    let run_res = vm.run(500).unwrap();
    assert!(matches!(run_res, mel_cpu::traits::CpuStepResult::Ok { .. }));

    // Verify Stack Pointer was initialized
    let sp = vm.read_register(RegisterId::named("sp")).unwrap();
    assert_eq!(sp, 0x8001_0000);

    // Verify UART console output captured exact string
    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    let output_str = String::from_utf8_lossy(&uart_out);

    assert_eq!(output_str, "Hello MEL RISC-V!\n");
}
