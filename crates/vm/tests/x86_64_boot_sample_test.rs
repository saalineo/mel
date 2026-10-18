use mel_core::RegisterId;
use mel_loader::load_raw_binary_from_file;
use mel_vm::create_basic_x86_64_vm;
use std::path::PathBuf;

/// Canonical byte sequence for x86-64 `boot.bin`.
pub const BOOT_BIN_BYTES: [u8; 67] = [
    // 0x00: movabsq $0x00200000, %rsp
    0x48, 0xbc, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x0A: movabsq $0x10000000, %rdi (UART base)
    0x48, 0xbf, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
    // 0x14: leaq 0x15(%rip), %rsi (pointer to msg at offset 0x30)
    0x48, 0x8d, 0x35, 0x15, 0x00, 0x00, 0x00, // 0x1B: print_loop: movzbq (%rsi), %rax
    0x48, 0x0f, 0xb6, 0x06, // 0x1F: testb %al, %al
    0x84, 0xc0, // 0x21: je halt_loop (offset 0x2A)
    0x74, 0x07, // 0x23: movb %al, (%rdi) (write char to UART THR)
    0x88, 0x07, // 0x25: incq %rsi
    0x48, 0xff, 0xc6, // 0x28: jmp print_loop (offset 0x1B)
    0xeb, 0xf1, // 0x2A: halt_loop: pause
    0xf3, 0x90, // 0x2C: jmp halt_loop (offset 0x2A)
    0xeb, 0xfc, // 0x2E: padding bytes (int3 / align)
    0xcc, 0xcc, // 0x30: msg: "Hello MEL x86-64!\n\0" (19 bytes)
    0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x4d, 0x45, 0x4c, 0x20, 0x78, 0x38, 0x36, 0x2d, 0x36, 0x34,
    0x21, 0x0a, 0x00,
];

#[test]
fn test_x86_64_boot_sample_fixture_execution() {
    let mut fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_path.pop(); // crates
    fixture_path.pop(); // root
    let bin_path = fixture_path.join("examples/x86_64/hello_uart/boot.bin");

    // Ensure fixture file exists with canonical bytes
    std::fs::write(&bin_path, BOOT_BIN_BYTES).unwrap();

    let mut vm = create_basic_x86_64_vm().unwrap();
    let ram_base = 0x0010_0000;

    // Load binary payload from file
    load_raw_binary_from_file(&mut vm, &bin_path, ram_base).unwrap();

    // Verify PC initialized to entry address
    assert_eq!(vm.pc().unwrap(), ram_base);

    // Run execution for up to 500 cycles (sufficient to print string and enter spin loop)
    let run_res = vm.run(500).unwrap();
    assert!(matches!(run_res, mel_cpu::traits::CpuStepResult::Ok { .. }));

    // Verify Stack Pointer was initialized
    let sp = vm.read_register(RegisterId::named("rsp")).unwrap();
    assert_eq!(sp, 0x0020_0000);

    // Verify UART console output captured exact string
    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    let output_str = String::from_utf8_lossy(&uart_out);

    assert_eq!(output_str, "Hello MEL x86-64!\n");
}
