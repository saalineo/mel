//! Integration test suite for multi-architecture disassembly engine in mel-debugger.

use mel_core::Architecture;
use mel_debugger::disasm::{create_disassembler, disassemble};

#[test]
fn test_create_disassembler_all_architectures() {
    for arch in [
        Architecture::Riscv64,
        Architecture::Arm64,
        Architecture::X86_64,
        Architecture::PowerIso64,
    ] {
        let cs = create_disassembler(arch);
        assert!(
            cs.is_ok(),
            "create_disassembler failed for arch {arch:?}: {:?}",
            cs.err()
        );
    }
}

#[test]
fn test_disassemble_riscv64_stream() {
    // Sequence of RV64 instructions:
    // 0x8000_0000: lui a0, 0x10000        -> b7 00 00 10
    // 0x8000_0004: addi a0, a0, 42        -> 93 05 a5 02
    // 0x8000_0008: c.nop (16-bit)         -> 01 00
    // 0x8000_000a: ret (jalr x0, x1, 0)   -> 67 80 00 00 (32-bit)
    let code: [u8; 14] = [
        0xb7, 0x00, 0x00, 0x10, // lui a0, 0x10000
        0x93, 0x05, 0xa5, 0x02, // addi a0, a0, 42
        0x01, 0x00,             // c.nop (16-bit)
        0x67, 0x80, 0x00, 0x00, // ret (jalr x0, ra, 0)
    ];

    let instructions = disassemble(Architecture::Riscv64, &code, 0x8000_0000).unwrap();
    assert_eq!(instructions.len(), 4);

    assert_eq!(instructions[0].address, 0x8000_0000);
    assert_eq!(instructions[0].size, 4);
    assert_eq!(instructions[0].mnemonic, "lui");

    assert_eq!(instructions[1].address, 0x8000_0004);
    assert_eq!(instructions[1].size, 4);
    assert_eq!(instructions[1].mnemonic, "addi");

    assert_eq!(instructions[2].address, 0x8000_0008);
    assert_eq!(instructions[2].size, 2);
    assert_eq!(instructions[2].mnemonic, "c.nop");

    assert_eq!(instructions[3].address, 0x8000_000a);
    assert_eq!(instructions[3].size, 4);
    assert_eq!(instructions[3].mnemonic, "ret");

    // Test unified display formatting
    let line = instructions[0].format_line();
    assert!(line.contains("0x80000000:"));
    assert!(line.contains("b7 00 00 10"));
    assert!(line.contains("lui"));
}

#[test]
fn test_disassemble_arm64_stream() {
    // Sequence of ARM64 instructions:
    // 0x4000_0000: mov x0, #42        -> 40 05 80 d2 (movz x0, #0x2a)
    // 0x4000_0004: add x1, x0, #10    -> 21 28 00 91 (add x1, x0, #0xa)
    // 0x4000_0008: str x1, [x0, #8]   -> 01 04 00 f9
    // 0x4000_000c: ret                -> c0 03 5f d6
    let code: [u8; 16] = [
        0x40, 0x05, 0x80, 0xd2, // mov x0, #42
        0x21, 0x28, 0x00, 0x91, // add x1, x0, #10
        0x01, 0x04, 0x00, 0xf9, // str x1, [x0, #8]
        0xc0, 0x03, 0x5f, 0xd6, // ret
    ];

    let instructions = disassemble(Architecture::Arm64, &code, 0x4000_0000).unwrap();
    assert_eq!(instructions.len(), 4);

    assert_eq!(instructions[0].address, 0x4000_0000);
    assert_eq!(instructions[0].size, 4);
    assert_eq!(instructions[0].mnemonic, "mov");

    assert_eq!(instructions[1].address, 0x4000_0004);
    assert_eq!(instructions[1].size, 4);
    assert_eq!(instructions[1].mnemonic, "add");

    assert_eq!(instructions[2].address, 0x4000_0008);
    assert_eq!(instructions[2].size, 4);
    assert_eq!(instructions[2].mnemonic, "str");

    assert_eq!(instructions[3].address, 0x4000_000c);
    assert_eq!(instructions[3].size, 4);
    assert_eq!(instructions[3].mnemonic, "ret");

    // Test formatted output string
    let display_str = format!("{}", instructions[3]);
    assert!(display_str.contains("0x4000000c:"));
    assert!(display_str.contains("ret"));
}

#[test]
fn test_disassemble_x86_64_stream() {
    // Sequence of variable-length x86-64 instructions:
    // 0x1000: mov rax, 42           -> 48 c7 c0 2a 00 00 00 (7 bytes)
    // 0x1007: add rax, rbx          -> 48 01 d8             (3 bytes)
    // 0x100a: nop                   -> 90                   (1 byte)
    // 0x100b: ret                   -> c3                   (1 byte)
    let code: [u8; 12] = [
        0x48, 0xc7, 0xc0, 0x2a, 0x00, 0x00, 0x00, // mov rax, 42
        0x48, 0x01, 0xd8,                         // add rax, rbx
        0x90,                                     // nop
        0xc3,                                     // ret
    ];

    let instructions = disassemble(Architecture::X86_64, &code, 0x1000).unwrap();
    assert_eq!(instructions.len(), 4);

    assert_eq!(instructions[0].address, 0x1000);
    assert_eq!(instructions[0].size, 7);
    assert_eq!(instructions[0].mnemonic, "mov");

    assert_eq!(instructions[1].address, 0x1007);
    assert_eq!(instructions[1].size, 3);
    assert_eq!(instructions[1].mnemonic, "add");

    assert_eq!(instructions[2].address, 0x100a);
    assert_eq!(instructions[2].size, 1);
    assert_eq!(instructions[2].mnemonic, "nop");

    assert_eq!(instructions[3].address, 0x100b);
    assert_eq!(instructions[3].size, 1);
    assert_eq!(instructions[3].mnemonic, "ret");

    let line = instructions[0].format_line();
    assert!(line.contains("0x00001000:"));
    assert!(line.contains("mov"));
}

#[test]
fn test_disassemble_power_iso64_stream() {
    // Sequence of PowerPC 64-bit big-endian instructions:
    // 0x0000_0100: li r3, 42 (addi r3, 0, 42) -> 38 60 00 2a
    // 0x0000_0104: add r4, r3, r5             -> 7c 83 2a 14
    // 0x0000_0108: nop (ori 0, 0, 0)          -> 60 00 00 00
    // 0x0000_010c: blr (branch to LR)         -> 4e 80 00 20
    let code: [u8; 16] = [
        0x38, 0x60, 0x00, 0x2a, // li r3, 42
        0x7c, 0x83, 0x2a, 0x14, // add r4, r3, r5
        0x60, 0x00, 0x00, 0x00, // nop
        0x4e, 0x80, 0x00, 0x20, // blr
    ];

    let instructions = disassemble(Architecture::PowerIso64, &code, 0x0000_0100).unwrap();
    println!("PPC insns: {:?}", instructions);
    assert_eq!(instructions.len(), 4);

    assert_eq!(instructions[0].address, 0x0000_0100);
    assert_eq!(instructions[0].size, 4);
    assert!(instructions[0].mnemonic == "li" || instructions[0].mnemonic == "addi");

    assert_eq!(instructions[1].address, 0x0000_0104);
    assert_eq!(instructions[1].size, 4);
    assert_eq!(instructions[1].mnemonic, "add");

    assert_eq!(instructions[2].address, 0x0000_0108);
    assert_eq!(instructions[2].size, 4);
    assert!(instructions[2].mnemonic == "nop" || instructions[2].mnemonic == "ori");

    assert_eq!(instructions[3].address, 0x0000_010c);
    assert_eq!(instructions[3].size, 4);
    assert_eq!(instructions[3].mnemonic, "blr");
}

#[test]
fn test_disassemble_empty_and_zeroes() {
    for arch in [
        Architecture::Riscv64,
        Architecture::Arm64,
        Architecture::X86_64,
        Architecture::PowerIso64,
    ] {
        let empty = disassemble(arch, &[], 0x1000).unwrap();
        assert!(empty.is_empty());
    }
}
