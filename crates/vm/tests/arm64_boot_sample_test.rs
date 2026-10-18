use mel_core::RegisterId;
use mel_loader::load_raw_binary_from_file;
use mel_vm::create_basic_arm64_vm;
use std::path::PathBuf;

/// Canonical byte sequence for ARM64 `boot.bin`.
pub const BOOT_BIN_BYTES: [u8; 58] = [
    // 0x00: movz x0, #0x4001, lsl #16 (x0 = 0x40010000)
    0x20, 0x00, 0xa8, 0xd2, // 0x04: mov sp, x0 (sp = 0x40010000)
    0x1f, 0x00, 0x00, 0x91,
    // 0x08: movz x1, #0x0900, lsl #16 (x1 = 0x09000000, UART base)
    0x01, 0x20, 0xa1, 0xd2, // 0x0C: adr x2, msg (x2 = pointer to msg at offset 0x28)
    0xe2, 0x00, 0x00, 0x10,
    // 0x10: print_loop: ldrb w3, [x2], #1 (load char, post-increment pointer)
    0x43, 0x14, 0x40, 0x38,
    // 0x14: cbz w3, halt_loop (branch to halt_loop at offset 0x20 if null terminator)
    0x63, 0x00, 0x00, 0x34, // 0x18: strb w3, [x1] (write char to UART THR)
    0x23, 0x00, 0x00, 0x39,
    // 0x1C: b print_loop (jump back to print_loop at offset 0x10)
    0xfd, 0xff, 0xff, 0x17, // 0x20: halt_loop: wfe (wait for event)
    0x5f, 0x20, 0x03, 0xd5, // 0x24: b halt_loop (spin loop)
    0xff, 0xff, 0xff, 0x17, // 0x28: msg: "Hello MEL ARM64!\n\0" (18 bytes)
    0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x4d, 0x45, 0x4c, 0x20, 0x41, 0x52, 0x4d, 0x36, 0x34, 0x21,
    0x0a, 0x00,
];

#[test]
fn test_arm64_boot_sample_fixture_execution() {
    let mut fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_path.pop(); // crates
    fixture_path.pop(); // root
    let bin_path = fixture_path.join("examples/arm64/hello_uart/boot.bin");

    // Ensure fixture file exists with canonical bytes
    std::fs::write(&bin_path, BOOT_BIN_BYTES).unwrap();

    let mut vm = create_basic_arm64_vm().unwrap();
    let ram_base = 0x4000_0000;

    // Load binary payload from file
    load_raw_binary_from_file(&mut vm, &bin_path, ram_base).unwrap();

    // Verify PC initialized to entry address
    assert_eq!(vm.pc().unwrap(), ram_base);

    // Run execution for up to 500 cycles (sufficient to print string and enter spin loop)
    let run_res = vm.run(500).unwrap();
    assert!(matches!(run_res, mel_cpu::traits::CpuStepResult::Ok { .. }));

    // Verify Stack Pointer was initialized
    let sp = vm.read_register(RegisterId::named("sp")).unwrap();
    assert_eq!(sp, 0x4001_0000);

    // Verify UART console output captured exact string
    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    let output_str = String::from_utf8_lossy(&uart_out);

    assert_eq!(output_str, "Hello MEL ARM64!\n");
}
