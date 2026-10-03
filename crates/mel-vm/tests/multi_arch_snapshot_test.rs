use mel_core::{Architecture, MelError, RegisterId};
use mel_cpu::traits::CpuStepResult;
use mel_vm::factory::build_vm_for_arch;
use std::env::temp_dir;

#[test]
fn test_multi_arch_snapshot_riscv64() {
    let mut vm = build_vm_for_arch(Architecture::Riscv64).unwrap();

    // lui x1, 0x10000; addi x2, x0, 42; nop; nop
    let code: [u8; 16] = [
        0xb7, 0x00, 0x00, 0x10, // lui x1, 0x10000
        0x13, 0x01, 0xa0, 0x02, // addi x2, x0, 42
        0x13, 0x00, 0x00, 0x00, // nop
        0x13, 0x00, 0x00, 0x00, // nop
    ];
    vm.write_memory(0x8000_0000, &code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Step 2 instructions (lui, addi)
    vm.step().unwrap();
    vm.step().unwrap();

    assert_eq!(vm.pc().unwrap(), 0x8000_0008);
    assert_eq!(vm.read_register(RegisterId::named("x1")).unwrap(), 0x1000_0000);
    assert_eq!(vm.read_register(RegisterId::named("x2")).unwrap(), 42);
    assert_eq!(vm.cycle_count(), 2);

    // Save snapshot
    let snapshot = vm.save_snapshot().unwrap();

    // Step further (nop)
    vm.step().unwrap();
    assert_eq!(vm.pc().unwrap(), 0x8000_000c);
    assert_eq!(vm.cycle_count(), 3);

    // Restore into fresh RISC-V VM
    let mut restored_vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
    restored_vm.load_snapshot(&snapshot).unwrap();

    assert_eq!(restored_vm.pc().unwrap(), 0x8000_0008);
    assert_eq!(restored_vm.read_register(RegisterId::named("x1")).unwrap(), 0x1000_0000);
    assert_eq!(restored_vm.read_register(RegisterId::named("x2")).unwrap(), 42);
    assert_eq!(restored_vm.cycle_count(), 2);
    assert_eq!(restored_vm.wall_time_ns(), 200);

    // Verify RAM restored correctly
    let mut ram_check = [0u8; 16];
    restored_vm.read_memory(0x8000_0000, &mut ram_check).unwrap();
    assert_eq!(ram_check, code);

    // Step the restored VM: executes the 3rd instruction (nop)
    let step_result = restored_vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(restored_vm.pc().unwrap(), 0x8000_000c);
    assert_eq!(restored_vm.cycle_count(), 3);
}

#[test]
fn test_multi_arch_snapshot_arm64() {
    let mut vm = build_vm_for_arch(Architecture::Arm64).unwrap();

    // 4 ARM64 NOPs (0xd503201f)
    let mut code = Vec::new();
    for _ in 0..4 {
        code.extend_from_slice(&0xd503201fu32.to_le_bytes());
    }
    vm.write_memory(0x4000_0000, &code).unwrap();
    vm.set_pc(0x4000_0000).unwrap();

    vm.write_register(RegisterId::named("x0"), 0xDEAD_BEEF).unwrap();
    vm.write_register(RegisterId::named("x1"), 0xCAFE_BABE).unwrap();

    // Step 2 NOPs
    vm.step().unwrap();
    vm.step().unwrap();

    assert_eq!(vm.pc().unwrap(), 0x4000_0008);
    assert_eq!(vm.cycle_count(), 2);
    assert_eq!(vm.read_register(RegisterId::named("x0")).unwrap(), 0xDEAD_BEEF);
    assert_eq!(vm.read_register(RegisterId::named("x1")).unwrap(), 0xCAFE_BABE);

    // Save snapshot
    let snapshot = vm.save_snapshot().unwrap();

    // Restore into fresh ARM64 VM
    let mut restored_vm = build_vm_for_arch(Architecture::Arm64).unwrap();
    restored_vm.load_snapshot(&snapshot).unwrap();

    assert_eq!(restored_vm.pc().unwrap(), 0x4000_0008);
    assert_eq!(restored_vm.cycle_count(), 2);
    assert_eq!(restored_vm.read_register(RegisterId::named("x0")).unwrap(), 0xDEAD_BEEF);
    assert_eq!(restored_vm.read_register(RegisterId::named("x1")).unwrap(), 0xCAFE_BABE);

    // Step 3rd instruction
    let step_result = restored_vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(restored_vm.pc().unwrap(), 0x4000_000c);
}

#[test]
fn test_multi_arch_snapshot_x86_64() {
    let mut vm = build_vm_for_arch(Architecture::X86_64).unwrap();

    // 4 x86 NOPs (0x90)
    let code = [0x90, 0x90, 0x90, 0x90];
    vm.write_memory(0x0010_0000, &code).unwrap();
    vm.set_pc(0x0010_0000).unwrap();

    vm.write_register(RegisterId::named("rax"), 0x1234_5678_9ABC_DEF0).unwrap();
    vm.write_register(RegisterId::named("rbx"), 0x0FED_CBA9_8765_4321).unwrap();

    // Step 2 NOPs
    vm.step().unwrap();
    vm.step().unwrap();

    assert_eq!(vm.pc().unwrap(), 0x0010_0002);
    assert_eq!(vm.cycle_count(), 2);
    assert_eq!(vm.read_register(RegisterId::named("rax")).unwrap(), 0x1234_5678_9ABC_DEF0);
    assert_eq!(vm.read_register(RegisterId::named("rbx")).unwrap(), 0x0FED_CBA9_8765_4321);

    // Save snapshot
    let snapshot = vm.save_snapshot().unwrap();

    // Restore into fresh x86-64 VM
    let mut restored_vm = build_vm_for_arch(Architecture::X86_64).unwrap();
    restored_vm.load_snapshot(&snapshot).unwrap();

    assert_eq!(restored_vm.pc().unwrap(), 0x0010_0002);
    assert_eq!(restored_vm.cycle_count(), 2);
    assert_eq!(restored_vm.read_register(RegisterId::named("rax")).unwrap(), 0x1234_5678_9ABC_DEF0);
    assert_eq!(restored_vm.read_register(RegisterId::named("rbx")).unwrap(), 0x0FED_CBA9_8765_4321);

    // Step 3rd NOP
    let step_result = restored_vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(restored_vm.pc().unwrap(), 0x0010_0003);
}

#[test]
fn test_multi_arch_snapshot_power_isa() {
    let mut vm = build_vm_for_arch(Architecture::PowerIso64).unwrap();

    // 4 PowerPC NOPs (ori 0, 0, 0 = 0x60000000)
    let mut code = Vec::new();
    for _ in 0..4 {
        code.extend_from_slice(&0x6000_0000u32.to_be_bytes());
    }
    vm.write_memory(0x0000_0100, &code).unwrap();
    vm.set_pc(0x0000_0100).unwrap();

    vm.write_register(RegisterId::named("r3"), 0xAAAA_BBBB).unwrap();
    vm.write_register(RegisterId::named("r4"), 0xCCCC_DDDD).unwrap();

    // Step 2 NOPs
    vm.step().unwrap();
    vm.step().unwrap();

    assert_eq!(vm.pc().unwrap(), 0x0000_0108);
    assert_eq!(vm.cycle_count(), 2);
    assert_eq!(vm.read_register(RegisterId::named("r3")).unwrap(), 0xAAAA_BBBB);
    assert_eq!(vm.read_register(RegisterId::named("r4")).unwrap(), 0xCCCC_DDDD);

    // Save snapshot
    let snapshot = vm.save_snapshot().unwrap();

    // Restore into fresh PowerPC VM
    let mut restored_vm = build_vm_for_arch(Architecture::PowerIso64).unwrap();
    restored_vm.load_snapshot(&snapshot).unwrap();

    assert_eq!(restored_vm.pc().unwrap(), 0x0000_0108);
    assert_eq!(restored_vm.cycle_count(), 2);
    assert_eq!(restored_vm.read_register(RegisterId::named("r3")).unwrap(), 0xAAAA_BBBB);
    assert_eq!(restored_vm.read_register(RegisterId::named("r4")).unwrap(), 0xCCCC_DDDD);

    // Step 3rd NOP
    let step_result = restored_vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(restored_vm.pc().unwrap(), 0x0000_010c);
}

#[test]
fn test_multi_arch_snapshot_cross_architecture_mismatch_rejected() {
    let riscv_vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
    let snapshot = riscv_vm.save_snapshot().unwrap();

    let mut arm_vm = build_vm_for_arch(Architecture::Arm64).unwrap();
    assert!(matches!(
        arm_vm.load_snapshot(&snapshot),
        Err(MelError::Config(_))
    ));

    let mut x86_vm = build_vm_for_arch(Architecture::X86_64).unwrap();
    assert!(matches!(
        x86_vm.load_snapshot(&snapshot),
        Err(MelError::Config(_))
    ));

    let mut ppc_vm = build_vm_for_arch(Architecture::PowerIso64).unwrap();
    assert!(matches!(
        ppc_vm.load_snapshot(&snapshot),
        Err(MelError::Config(_))
    ));
}

#[test]
fn test_multi_arch_snapshot_file_persistence_all_archs() {
    let target_temp_dir = temp_dir();

    for (i, &arch) in Architecture::ALL.iter().enumerate() {
        let file_path = target_temp_dir.join(format!("test_multi_arch_snap_{i}.mels"));
        let mut vm = build_vm_for_arch(arch).unwrap();
        vm.clock_mut().advance(1234);

        vm.save_snapshot_to_file(&file_path).unwrap();

        let mut restored_vm = build_vm_for_arch(arch).unwrap();
        restored_vm.load_snapshot_from_file(&file_path).unwrap();

        assert_eq!(restored_vm.arch(), arch);
        assert_eq!(restored_vm.cycle_count(), 1234);

        let _ = std::fs::remove_file(&file_path);
    }
}
