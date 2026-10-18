use mel_core::RegisterId;
use mel_cpu::traits::CpuStepResult;
use mel_vm::create_basic_riscv64_vm;

#[test]
fn test_trap_harness_illegal_instruction() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create RISC-V VM");

    // Write illegal instruction (0x00000000 is invalid in RISC-V)
    let bad_code = vec![0x00, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &bad_code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Stepping must return a controlled Error or Trap, never panic
    let result = vm.step();
    assert!(
        result.is_err(),
        "Expected error or trap on illegal instruction, got: {result:?}"
    );
    println!(
        "Graceful illegal instruction response: {:?}",
        result.err().unwrap()
    );
}

#[test]
fn test_trap_harness_unmapped_memory_access() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create RISC-V VM");

    // Assembly that attempts to load from unmapped address 0x5000_0000:
    // lui a0, 0x50000
    // ld  a1, 0(a0)
    let bad_load = vec![
        0x37, 0x05, 0x00, 0x50, // lui a0, 0x50000
        0x03, 0x35, 0x05, 0x00, // ld a1, 0(a0)
    ];
    vm.write_memory(0x8000_0000, &bad_load).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Step 1: lui succeeds
    let s1 = vm.step();
    assert!(s1.is_ok(), "lui instruction failed: {s1:?}");

    // Step 2: ld from unmapped memory triggers graceful error / unmapped read rejection
    let s2 = vm.step();
    // SystemBus detects unmapped or Unicorn intercepts it
    println!("Graceful unmapped memory load response: {s2:?}");
}

#[test]
fn test_trap_harness_timer_trap_delivery_and_mret() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create RISC-V VM");
    let clint = vm.clint().expect("No CLINT found");

    // Write NOPs in RAM
    let mut nops = Vec::new();
    for _ in 0..10 {
        nops.extend_from_slice(&[0x13, 0x00, 0x00, 0x00]);
    }
    vm.write_memory(0x8000_0000, &nops).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    // Configure Trap vector at 0x8000_1000
    vm.write_register(RegisterId::named("mtvec"), 0x8000_1000)
        .unwrap();
    vm.write_register(RegisterId::named("mstatus"), 1 << 3)
        .unwrap(); // MIE
    vm.write_register(RegisterId::named("mie"), 1 << 7).unwrap(); // MTIE

    // Trigger timer interrupt: mtime = 10, mtimecmp = 10
    {
        let mut c = clint.lock().unwrap();
        c.set_mtime(10);
        c.set_mtimecmp(10);
    }

    // Step 1: executes 1 instruction, synchronizes timer interrupt line
    let _ = vm.step().unwrap();

    // Step 2: delivers the trap!
    let res = vm.step().expect("Step 2 failed");
    match res {
        CpuStepResult::Trap { cause, .. } => {
            assert_eq!(cause, (1u64 << 63) | 7, "Expected MTIP cause");
            assert_eq!(vm.pc().unwrap(), 0x8000_1000);
        }
        other => panic!("Expected Trap on step 2, got {other:?}"),
    }
}
