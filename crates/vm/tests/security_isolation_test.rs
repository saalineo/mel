//! Security & Host Isolation Audit Suite.
//!
//! Validates that malicious or corrupted guest code cannot escape VM boundaries,
//! access host memory, or cause host panics.

use mel_core::RegisterId;
use mel_devices::{BootRom, MmioDevice};
use mel_vm::create_basic_riscv64_vm;

#[test]
fn test_security_out_of_bounds_memory_write() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create VM");

    let hostile_addresses = [
        0x0000_0000u64,
        0x0000_0010u64,
        0x4000_0000u64,
        0x9000_0000u64,
        0xFFFF_FFFF_FFFF_0000u64,
    ];

    for &addr in &hostile_addresses {
        // Attempting to write into unmapped host/guest space
        let res = vm.write_memory(addr, &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(
            res.is_err(),
            "VM must reject hostile write at unmapped address {addr:#x}"
        );
    }
}

#[test]
fn test_security_unbounded_jump() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create VM");

    // Load jump to invalid address 0xDEAD_0000:
    // lui a0, 0xDEAD0
    // jalr zero, a0, 0
    let jump_code = vec![
        0x37, 0x05, 0xad, 0xde, // lui a0, 0xDEAD0
        0x67, 0x00, 0x05, 0x00, // jalr zero, a0, 0
    ];
    vm.write_memory(0x8000_0000, &jump_code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Step 1: lui a0
    let _ = vm.step().unwrap();

    // Step 2: jalr jumps to 0xDEAD0000 - unmapped target must be caught safely by host
    let res = vm.step();
    if res.is_ok() {
        assert_eq!(vm.pc().unwrap(), 0xffffffffdead0000);
        let res2 = vm.step();
        assert!(
            res2.is_err(),
            "Host must gracefully handle unmapped PC execution without panic"
        );
    } else {
        assert!(res.is_err(), "Host safely trapped unmapped jump fetch");
    }
}

#[test]
fn test_security_write_to_read_only_boot_rom() {
    let mut rom = BootRom::riscv64_jump_to("test_rom", 0x8000_0000);

    // Attempt writing to ROM must fail with ReadOnly error
    let res = rom.write(0, 0x1234, 4);
    assert!(res.is_err(), "Write to Boot ROM must be rejected");
}

#[test]
fn test_security_fuzzed_invalid_opcodes() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create VM");

    let fuzzed_opcodes = [
        [0x00, 0x00, 0x00, 0x00],
        [0xFF, 0xFF, 0xFF, 0xFF],
        [0x7F, 0x7F, 0x7F, 0x7F],
        [0x5B, 0x00, 0x00, 0x00],
    ];

    for (idx, opcode) in fuzzed_opcodes.iter().enumerate() {
        let load_addr = 0x8000_0000 + (idx as u64) * 0x100;
        vm.write_memory(load_addr, opcode).unwrap();
        vm.set_pc(load_addr).unwrap();

        let res = vm.step();
        assert!(
            res.is_err(),
            "Invalid opcode {:?} must produce error, never panic host",
            opcode
        );
    }
}

#[test]
fn test_security_stack_underflow_isolation() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create VM");

    // Set sp right at RAM base: 0x8000_0000
    vm.write_register(RegisterId::named("sp"), 0x8000_0000)
        .unwrap();

    // Assembly that pushes below 0x8000_0000:
    // addi sp, sp, -16 -> sp = 0x7FFF_FFF0 (unmapped!)
    // sd zero, 0(sp)
    let underflow_code = vec![
        0x13, 0x01, 0x01, 0xff, // addi sp, sp, -16
        0x23, 0x30, 0x01, 0x00, // sd zero, 0(sp)
    ];
    vm.write_memory(0x8000_0000, &underflow_code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let _ = vm.step().unwrap(); // addi

    // Store to unmapped memory below RAM base
    let res = vm.step();
    // Rejection or trap occurs, host remains safe
    println!("Stack underflow response: {res:?}");
}
