use mel_core::Architecture;
use mel_cpu::traits::CpuStepResult;
use mel_vm::clock::{DEFAULT_FREQUENCY_HZ, VirtualClock};
use mel_vm::factory::build_vm_for_arch;

#[test]
fn test_vm_clock_initial_state_and_stepping() {
    let mut vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
    assert_eq!(vm.cycle_count(), 0);
    assert_eq!(vm.wall_time_ns(), 0);
    assert_eq!(vm.clock().frequency_hz(), DEFAULT_FREQUENCY_HZ);

    // Write 5 NOP instructions (addi x0, x0, 0 = 0x00000013)
    let nops = vec![0x13, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00];
    vm.write_memory(0x8000_0000, &nops).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    let step_result = vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(vm.cycle_count(), 1);
    assert_eq!(vm.wall_time_ns(), 100); // 1 cycle at 10 MHz = 100 ns

    let step_result2 = vm.step().unwrap();
    assert_eq!(step_result2, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(vm.cycle_count(), 2);
    assert_eq!(vm.wall_time_ns(), 200);
}

#[test]
fn test_vm_clock_clint_timer_synchronization() {
    let mut vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
    let clint = vm.clint().expect("RISC-V profile must contain CLINT");

    // Write a loop of 10 NOPs
    let mut code = Vec::new();
    for _ in 0..10 {
        code.extend_from_slice(&0x00000013u32.to_le_bytes());
    }
    vm.write_memory(0x8000_0000, &code).unwrap();
    vm.set_pc(0x8000_0000).unwrap();

    for i in 1..=10 {
        let step_result = vm.step().unwrap();
        assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(vm.cycle_count(), i);
        assert_eq!(clint.lock().unwrap().mtime(), i);
    }
}

#[test]
fn test_vm_clock_determinism_50k_cycles_across_runs() {
    // Run two independent virtual machines with identical payload and stepping schedules
    let run_simulation = |batch_sizes: &[u64]| -> (u64, u64, u64) {
        let mut vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
        let clint = vm.clint().unwrap();

        // 2 instructions in RAM: NOP (0x8000_0000), JAL x0, -4 (0x8000_0004)
        let mut code = Vec::new();
        code.extend_from_slice(&0x00000013u32.to_le_bytes()); // nop at 0x8000_0000
        code.extend_from_slice(&0xffdff06fu32.to_le_bytes()); // jal x0, -4 at 0x8000_0004

        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        const TARGET_CYCLES: u64 = 50_000;
        let mut total_executed = 0;
        let mut batch_idx = 0;

        while total_executed < TARGET_CYCLES {
            let chunk =
                batch_sizes[batch_idx % batch_sizes.len()].min(TARGET_CYCLES - total_executed);
            let run_result = vm.run(chunk).unwrap();
            match run_result {
                CpuStepResult::Ok { cycles } => {
                    total_executed += cycles;
                }
                _ => break,
            }
            batch_idx += 1;
        }

        let final_cycles = vm.cycle_count();
        let final_time_ns = vm.wall_time_ns();
        let clint_mtime = clint.lock().unwrap().mtime();
        (final_cycles, final_time_ns, clint_mtime)
    };

    let (cycles1, time1, mtime1) = run_simulation(&[1, 5, 10, 50, 100, 500, 1000]);
    let (cycles2, time2, mtime2) = run_simulation(&[250, 250, 500, 1000]);
    let (cycles3, time3, mtime3) = run_simulation(&[1000, 2000, 5000]);

    assert_eq!(cycles1, 50_000);
    assert_eq!(cycles2, 50_000);
    assert_eq!(cycles3, 50_000);

    assert_eq!(time1, 5_000_000); // 5 ms
    assert_eq!(time2, 5_000_000);
    assert_eq!(time3, 5_000_000);

    assert_eq!(mtime1, 50_000);
    assert_eq!(mtime2, 50_000);
    assert_eq!(mtime3, 50_000);
}

#[test]
fn test_vm_custom_clock_frequency() {
    let clock = VirtualClock::new(50_000_000).unwrap(); // 50 MHz (1 cycle = 20 ns)
    let mut vm = build_vm_for_arch(Architecture::Arm64)
        .unwrap()
        .with_clock(clock);

    assert_eq!(vm.clock().frequency_hz(), 50_000_000);

    // 1 ARM64 NOP (0xd503201f)
    let nop = 0xd503201fu32.to_le_bytes();
    vm.write_memory(0x4000_0000, &nop).unwrap();
    vm.set_pc(0x4000_0000).unwrap();

    let step_result = vm.step().unwrap();
    assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(vm.cycle_count(), 1);
    assert_eq!(vm.wall_time_ns(), 20); // 20 ns per cycle at 50 MHz
}
