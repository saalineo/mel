//! Simulation Performance Benchmarking Suite.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use mel_core::Architecture;
use mel_vm::create_basic_riscv64_vm;
use mel_vm::factory::build_vm_for_arch;

fn bench_vm_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_creation");
    for arch in [
        Architecture::Riscv64,
        Architecture::Arm64,
        Architecture::X86_64,
        Architecture::PowerIso64,
    ] {
        group.bench_function(format!("create_vm_{:?}", arch), |b| {
            b.iter(|| {
                let vm = build_vm_for_arch(black_box(arch)).unwrap();
                black_box(vm);
            });
        });
    }
    group.finish();
}

fn bench_instruction_throughput(c: &mut Criterion) {
    c.bench_function("riscv64_instruction_step", |b| {
        let mut vm = create_basic_riscv64_vm().unwrap();
        // 100 NOPs in RAM
        let mut code = Vec::new();
        for _ in 0..100 {
            code.extend_from_slice(&[0x13, 0x00, 0x00, 0x00]);
        }
        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        b.iter(|| {
            let res = vm.step().unwrap();
            black_box(res);
        });
    });
}

fn bench_mmio_throughput(c: &mut Criterion) {
    c.bench_function("uart_mmio_write", |b| {
        let vm = create_basic_riscv64_vm().unwrap();
        let bus = vm.bus();

        b.iter(|| {
            bus.lock()
                .unwrap()
                .write_bytes(0x1000_0000, black_box(b"A"))
                .unwrap();
        });
    });
}

fn bench_snapshot_cycle(c: &mut Criterion) {
    c.bench_function("snapshot_save_and_load", |b| {
        let mut vm = create_basic_riscv64_vm().unwrap();
        b.iter(|| {
            let snap = vm.save_snapshot().unwrap();
            vm.load_snapshot(black_box(&snap)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_vm_creation,
    bench_instruction_throughput,
    bench_mmio_throughput,
    bench_snapshot_cycle
);
criterion_main!(benches);
