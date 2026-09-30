use mel_core::{Architecture, RegisterId};
use mel_cpu::traits::CpuStepResult;
use mel_vm::{create_basic_riscv64_vm, create_mock_vm};

#[test]
fn test_mock_vm_snapshot_roundtrip_and_state_equality() {
    let mut vm = create_mock_vm(Architecture::Riscv64).unwrap();

    // 1. Setup initial state
    vm.write_register(RegisterId::named("x1"), 0x1111_2222_3333_4444)
        .unwrap();
    vm.write_register(RegisterId::named("x2"), 0x5555_6666_7777_8888)
        .unwrap();

    let ram_base = Architecture::Riscv64.default_ram_base();
    let initial_data = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11];
    vm.write_memory(ram_base, &initial_data).unwrap();

    // Advance 1 step
    vm.step().unwrap();
    let pc_before_snapshot = vm.pc().unwrap();

    // 2. Take snapshot
    let snapshot = vm.save_snapshot().unwrap();
    assert_eq!(snapshot.arch, Architecture::Riscv64);
    assert_eq!(snapshot.ram_base, ram_base);

    // 3. Mutate VM state extensively
    vm.step().unwrap();
    vm.step().unwrap();
    vm.write_register(RegisterId::named("x1"), 0x9999_9999)
        .unwrap();
    vm.write_register(RegisterId::named("x2"), 0x0000_0000)
        .unwrap();
    vm.write_memory(ram_base, &[0x00; 8]).unwrap();

    assert_ne!(vm.pc().unwrap(), pc_before_snapshot);
    assert_ne!(
        vm.read_register(RegisterId::named("x1")).unwrap(),
        0x1111_2222_3333_4444
    );

    // 4. Restore from snapshot
    vm.load_snapshot(&snapshot).unwrap();

    // 5. Verify restored state matches exactly
    assert_eq!(vm.pc().unwrap(), pc_before_snapshot);
    assert_eq!(
        vm.read_register(RegisterId::named("x1")).unwrap(),
        0x1111_2222_3333_4444
    );
    assert_eq!(
        vm.read_register(RegisterId::named("x2")).unwrap(),
        0x5555_6666_7777_8888
    );

    let mut restored_data = [0u8; 8];
    vm.read_memory(ram_base, &mut restored_data).unwrap();
    assert_eq!(restored_data, initial_data);
}

#[test]
fn test_snapshot_file_persistence() {
    let mut vm = create_mock_vm(Architecture::Arm64).unwrap();
    let ram_base = Architecture::Arm64.default_ram_base();

    vm.write_register(RegisterId::named("x0"), 0xCAFE_BABE)
        .unwrap();
    vm.write_memory(ram_base, &[0x42; 16]).unwrap();

    let temp_dir = std::env::temp_dir();
    let snapshot_path = temp_dir.join("test_machine_snapshot.mel");

    // Save to disk
    vm.save_snapshot_to_file(&snapshot_path).unwrap();
    assert!(snapshot_path.exists());

    // Mutate
    vm.write_register(RegisterId::named("x0"), 0x0).unwrap();
    vm.write_memory(ram_base, &[0x00; 16]).unwrap();

    // Restore from disk
    vm.load_snapshot_from_file(&snapshot_path).unwrap();

    assert_eq!(
        vm.read_register(RegisterId::named("x0")).unwrap(),
        0xCAFE_BABE
    );
    let mut check_mem = [0u8; 16];
    vm.read_memory(ram_base, &mut check_mem).unwrap();
    assert_eq!(check_mem, [0x42; 16]);

    // Clean up
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn test_riscv64_real_vm_snapshot_and_resume_execution() {
    let mut vm = create_basic_riscv64_vm().unwrap();

    // RISC-V assembly payload (12 bytes):
    // 1: lui x1, 0x10000   (load UART base address)
    // 2: addi x2, x0, 89   ('Y')
    // 3: sb x2, 0(x1)      (store byte to UART TX register)
    let code: [u8; 12] = [
        0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x90, 0x05, 0x23, 0x80, 0x20, 0x00,
    ];

    vm.write_memory(0x8000_0000, &code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Step 1: lui x1, 0x10000
    let s1 = vm.step().unwrap();
    assert_eq!(s1, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(vm.pc().unwrap(), 0x8000_0004);
    assert_eq!(
        vm.read_register(RegisterId::named("x1")).unwrap(),
        0x1000_0000
    );

    // Save snapshot at instruction 2 boundary
    let snapshot = vm.save_snapshot().unwrap();

    // Step 2 & 3: run to completion
    let s2 = vm.step().unwrap();
    assert_eq!(s2, CpuStepResult::Ok { cycles: 1 });
    let s3 = vm.step().unwrap();
    assert_eq!(s3, CpuStepResult::Ok { cycles: 1 });

    let uart_out = vm.uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(uart_out, b"Y");

    // Restore machine from snapshot
    vm.load_snapshot(&snapshot).unwrap();

    // Verify machine state is precisely back at instruction 2
    assert_eq!(vm.pc().unwrap(), 0x8000_0004);
    assert_eq!(
        vm.read_register(RegisterId::named("x1")).unwrap(),
        0x1000_0000
    );
    assert_eq!(vm.read_register(RegisterId::named("x2")).unwrap(), 0);

    // Step 2 and 3 again from restored state
    vm.step().unwrap();
    assert_eq!(vm.read_register(RegisterId::named("x2")).unwrap(), 89);
    vm.step().unwrap();

    let second_out = vm.uart().unwrap().lock().unwrap().drain_output();
    assert_eq!(second_out, b"Y");
}

#[test]
fn test_snapshot_architecture_mismatch_rejected() {
    let arm_vm = create_mock_vm(Architecture::Arm64).unwrap();
    let arm_snapshot = arm_vm.save_snapshot().unwrap();

    let mut riscv_vm = create_mock_vm(Architecture::Riscv64).unwrap();
    let err = riscv_vm.load_snapshot(&arm_snapshot);

    assert!(err.is_err());
}
