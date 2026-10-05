//! Integration tests for multi-architecture ELF64 segment loading into VirtualMachine.

use mel_core::{Architecture, MelError};
use mel_loader::{load_elf, load_elf_from_file};
use mel_vm::{create_basic_arm64_vm, create_basic_riscv64_vm, create_basic_x86_64_vm, create_mock_vm};
use object::elf;

struct TestElfBuilder {
    machine: u16,
    is_big_endian: bool,
    entry_point: u64,
    code: Vec<u8>,
    text_vaddr: u64,
    bss_vaddr: u64,
    bss_size: u64,
}

impl TestElfBuilder {
    fn new(machine: u16, is_big_endian: bool, entry_point: u64, code: &[u8]) -> Self {
        Self {
            machine,
            is_big_endian,
            entry_point,
            code: code.to_vec(),
            text_vaddr: entry_point,
            bss_vaddr: entry_point + 0x1000,
            bss_size: 0x200,
        }
    }

    fn with_bss(mut self, bss_vaddr: u64, bss_size: u64) -> Self {
        self.bss_vaddr = bss_vaddr;
        self.bss_size = bss_size;
        self
    }

    fn build(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let write_u16 = |buf: &mut Vec<u8>, val: u16| {
            if self.is_big_endian {
                buf.extend_from_slice(&val.to_be_bytes());
            } else {
                buf.extend_from_slice(&val.to_le_bytes());
            }
        };

        let write_u32 = |buf: &mut Vec<u8>, val: u32| {
            if self.is_big_endian {
                buf.extend_from_slice(&val.to_be_bytes());
            } else {
                buf.extend_from_slice(&val.to_le_bytes());
            }
        };

        let write_u64 = |buf: &mut Vec<u8>, val: u64| {
            if self.is_big_endian {
                buf.extend_from_slice(&val.to_be_bytes());
            } else {
                buf.extend_from_slice(&val.to_le_bytes());
            }
        };

        let ph_offset = 64u64;
        let ph_count = if self.bss_size > 0 { 2u16 } else { 1u16 };

        let text_offset = 0x100u64;
        let text_size = self.code.len() as u64;

        let shstrtab_offset = 0x200u64;
        let mut shstrtab = Vec::new();
        shstrtab.push(0);
        let text_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".text\0");
        let bss_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".bss\0");
        let shstrtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");

        let sh_offset = 0x300u64;
        let sh_count = 4u16;
        let shstrndx = 3u16;

        // 1. ELF Header (64 bytes)
        buf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
        buf.push(2); // ELFCLASS64
        buf.push(if self.is_big_endian { 2 } else { 1 });
        buf.push(1); // EV_CURRENT
        buf.push(0); // System V
        buf.extend_from_slice(&[0u8; 8]); // padding

        write_u16(&mut buf, elf::ET_EXEC);
        write_u16(&mut buf, self.machine);
        write_u32(&mut buf, 1);
        write_u64(&mut buf, self.entry_point);
        write_u64(&mut buf, ph_offset);
        write_u64(&mut buf, sh_offset);
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 64);
        write_u16(&mut buf, 56);
        write_u16(&mut buf, ph_count);
        write_u16(&mut buf, 64);
        write_u16(&mut buf, sh_count);
        write_u16(&mut buf, shstrndx);

        // 2. Program Headers
        // Segment 1: .text (RX)
        write_u32(&mut buf, elf::PT_LOAD);
        write_u32(&mut buf, elf::PF_R | elf::PF_X);
        write_u64(&mut buf, text_offset);
        write_u64(&mut buf, self.text_vaddr);
        write_u64(&mut buf, self.text_vaddr);
        write_u64(&mut buf, text_size);
        write_u64(&mut buf, text_size);
        write_u64(&mut buf, 0x1000);

        // Segment 2: .bss (RW) if present
        if self.bss_size > 0 {
            write_u32(&mut buf, elf::PT_LOAD);
            write_u32(&mut buf, elf::PF_R | elf::PF_W);
            write_u64(&mut buf, text_offset + text_size);
            write_u64(&mut buf, self.bss_vaddr);
            write_u64(&mut buf, self.bss_vaddr);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, self.bss_size);
            write_u64(&mut buf, 0x1000);
        }

        // Pad to text_offset
        if buf.len() < text_offset as usize {
            buf.resize(text_offset as usize, 0);
        }
        buf.extend_from_slice(&self.code);

        // Pad to shstrtab_offset
        if buf.len() < shstrtab_offset as usize {
            buf.resize(shstrtab_offset as usize, 0);
        }
        buf.extend_from_slice(&shstrtab);

        // Pad to sh_offset
        if buf.len() < sh_offset as usize {
            buf.resize(sh_offset as usize, 0);
        }

        // Section 0: NULL
        for _ in 0..8 {
            write_u64(&mut buf, 0);
        }

        // Section 1: .text
        write_u32(&mut buf, text_name_off);
        write_u32(&mut buf, elf::SHT_PROGBITS);
        write_u64(&mut buf, (elf::SHF_ALLOC | elf::SHF_EXECINSTR) as u64);
        write_u64(&mut buf, self.text_vaddr);
        write_u64(&mut buf, text_offset);
        write_u64(&mut buf, text_size);
        write_u32(&mut buf, 0);
        write_u32(&mut buf, 0);
        write_u64(&mut buf, 4);
        write_u64(&mut buf, 0);

        // Section 2: .bss
        write_u32(&mut buf, bss_name_off);
        write_u32(&mut buf, elf::SHT_NOBITS);
        write_u64(&mut buf, (elf::SHF_ALLOC | elf::SHF_WRITE) as u64);
        write_u64(&mut buf, self.bss_vaddr);
        write_u64(&mut buf, text_offset + text_size);
        write_u64(&mut buf, self.bss_size);
        write_u32(&mut buf, 0);
        write_u32(&mut buf, 0);
        write_u64(&mut buf, 8);
        write_u64(&mut buf, 0);

        // Section 3: .shstrtab
        write_u32(&mut buf, shstrtab_name_off);
        write_u32(&mut buf, elf::SHT_STRTAB);
        write_u64(&mut buf, 0);
        write_u64(&mut buf, 0);
        write_u64(&mut buf, shstrtab_offset);
        write_u64(&mut buf, shstrtab.len() as u64);
        write_u32(&mut buf, 0);
        write_u32(&mut buf, 0);
        write_u64(&mut buf, 1);
        write_u64(&mut buf, 0);

        buf
    }
}

#[test]
fn test_load_riscv64_elf_and_execute() {
    let mut vm = create_basic_riscv64_vm().unwrap();

    // Fill target BSS region with non-zero garbage to prove BSS zeroing works
    let garbage = vec![0xFFu8; 512];
    vm.write_memory(0x8000_1000, &garbage).unwrap();

    // RISC-V assembly:
    // lui x1, 0x10000     (UART base 0x10000000)
    // addi x2, x0, 79     ('O')
    // sb x2, 0(x1)
    // addi x2, x0, 75     ('K')
    // sb x2, 0(x1)
    let mut code = Vec::new();
    code.extend_from_slice(&0x100000b7u32.to_le_bytes()); // lui x1, 0x10000
    code.extend_from_slice(&0x04f00113u32.to_le_bytes()); // addi x2, x0, 79 ('O')
    code.extend_from_slice(&0x00208023u32.to_le_bytes()); // sb x2, 0(x1)
    code.extend_from_slice(&0x04b00113u32.to_le_bytes()); // addi x2, x0, 75 ('K')
    code.extend_from_slice(&0x00208023u32.to_le_bytes()); // sb x2, 0(x1)

    let elf_bytes = TestElfBuilder::new(elf::EM_RISCV, false, 0x8000_0000, &code)
        .with_bss(0x8000_1000, 512)
        .build();

    let summary = load_elf(&mut vm, &elf_bytes).unwrap();
    assert_eq!(summary.architecture, Architecture::Riscv64);
    assert_eq!(summary.entry_point, 0x8000_0000);
    assert_eq!(summary.segments_loaded, 2);
    assert_eq!(summary.total_bytes_loaded, code.len() as u64);
    assert_eq!(summary.total_bss_zeroed, 512);
    assert_eq!(vm.pc().unwrap(), 0x8000_0000);

    // Verify BSS was zeroed
    let mut bss_read = vec![0xAAu8; 512];
    vm.read_memory(0x8000_1000, &mut bss_read).unwrap();
    assert_eq!(bss_read, vec![0u8; 512]);

    // Execute the loaded instructions
    vm.run(5).unwrap();

    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(uart_out, b"OK");
}

#[test]
fn test_load_arm64_elf_and_execute() {
    let mut vm = create_basic_arm64_vm().unwrap();

    // Fill target BSS region with non-zero garbage
    let garbage = vec![0xEEu8; 256];
    vm.write_memory(0x4000_1000, &garbage).unwrap();

    // ARM64 assembly:
    // movz x0, #0x0900, lsl #16  (UART base 0x09000000)
    // movz w1, #65               ('A')
    // strb w1, [x0]
    // movz w1, #82               ('R')
    // strb w1, [x0]
    // movz w1, #77               ('M')
    // strb w1, [x0]
    let mut code = Vec::new();
    code.extend_from_slice(&0xd2a12000u32.to_le_bytes()); // movz x0, #0x0900, lsl #16
    code.extend_from_slice(&0xd2800821u32.to_le_bytes()); // movz w1, #65 ('A')
    code.extend_from_slice(&0x39000001u32.to_le_bytes()); // strb w1, [x0]
    code.extend_from_slice(&0xd2800a41u32.to_le_bytes()); // movz w1, #82 ('R')
    code.extend_from_slice(&0x39000001u32.to_le_bytes()); // strb w1, [x0]
    code.extend_from_slice(&0xd28009a1u32.to_le_bytes()); // movz w1, #77 ('M')
    code.extend_from_slice(&0x39000001u32.to_le_bytes()); // strb w1, [x0]

    let elf_bytes = TestElfBuilder::new(elf::EM_AARCH64, false, 0x4000_0000, &code)
        .with_bss(0x4000_1000, 256)
        .build();

    let summary = load_elf(&mut vm, &elf_bytes).unwrap();
    assert_eq!(summary.architecture, Architecture::Arm64);
    assert_eq!(summary.entry_point, 0x4000_0000);
    assert_eq!(summary.segments_loaded, 2);
    assert_eq!(summary.total_bytes_loaded, code.len() as u64);
    assert_eq!(summary.total_bss_zeroed, 256);
    assert_eq!(vm.pc().unwrap(), 0x4000_0000);

    // Verify BSS was zeroed
    let mut bss_read = vec![0x55u8; 256];
    vm.read_memory(0x4000_1000, &mut bss_read).unwrap();
    assert_eq!(bss_read, vec![0u8; 256]);

    // Execute the loaded instructions
    vm.run(7).unwrap();

    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(uart_out, b"ARM");
}

#[test]
fn test_load_x86_64_elf() {
    let mut vm = create_basic_x86_64_vm().unwrap();

    // x86_64 assembly:
    // mov dx, 0x1000 (COM1 MMIO/Port)
    // mov al, 0x58   ('X')
    let code: [u8; 7] = [0x66, 0xba, 0x00, 0x10, 0xb0, 0x58, 0x90];

    let elf_bytes = TestElfBuilder::new(elf::EM_X86_64, false, 0x0010_0000, &code).build();

    let summary = load_elf(&mut vm, &elf_bytes).unwrap();
    assert_eq!(summary.architecture, Architecture::X86_64);
    assert_eq!(summary.entry_point, 0x0010_0000);
    assert_eq!(vm.pc().unwrap(), 0x0010_0000);

    let mut mem_read = [0u8; 7];
    vm.read_memory(0x0010_0000, &mut mem_read).unwrap();
    assert_eq!(mem_read, code);
}

#[test]
fn test_elf_architecture_mismatch_rejected() {
    let mut vm = create_basic_riscv64_vm().unwrap();
    let code = [0x00, 0x00, 0x80, 0xd2]; // ARM64 instruction
    let arm_elf = TestElfBuilder::new(elf::EM_AARCH64, false, 0x4000_0000, &code).build();

    let err = load_elf(&mut vm, &arm_elf).unwrap_err();
    assert!(matches!(err, MelError::Loader(_)));
    assert!(err.to_string().contains("Architecture mismatch"));
}

#[test]
fn test_elf_unmapped_segment_fails() {
    let mut vm = create_mock_vm(Architecture::Riscv64).unwrap();
    let code = [0x13, 0x00, 0x00, 0x00];
    // Address 0x5000_0000 is not mapped in mock VM
    let elf_bytes = TestElfBuilder::new(elf::EM_RISCV, false, 0x5000_0000, &code).build();

    let err = load_elf(&mut vm, &elf_bytes);
    assert!(err.is_err());
}

#[test]
fn test_load_elf_from_file() {
    let mut vm = create_basic_riscv64_vm().unwrap();
    let code = [0x13, 0x05, 0xa0, 0x02]; // addi a0, zero, 42
    let elf_bytes = TestElfBuilder::new(elf::EM_RISCV, false, 0x8000_0000, &code).build();

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("test_mel_elf_{}.elf", std::process::id()));
    std::fs::write(&temp_file, &elf_bytes).unwrap();

    let summary = load_elf_from_file(&mut vm, &temp_file).unwrap();
    assert_eq!(summary.architecture, Architecture::Riscv64);
    assert_eq!(summary.entry_point, 0x8000_0000);
    assert_eq!(vm.pc().unwrap(), 0x8000_0000);

    let _ = std::fs::remove_file(temp_file);
}
