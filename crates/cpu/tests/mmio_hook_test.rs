use mel_bus::SystemBus;
use mel_core::RegisterId;
use mel_cpu::traits::{CpuBackend, CpuStepResult};
use mel_cpu::Riscv64Cpu;
use mel_devices::{SharedMmioBusAdapter, Uart16550};
use mel_memory::Ram;
use std::sync::{Arc, Mutex};
use unicorn_engine::unicorn_const::Prot;

#[test]
fn test_riscv64_mmio_uart_single_write() {
    let mut bus = SystemBus::new();
    let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
    bus.map_ram("RAM", 0x8000_0000, ram).unwrap();

    let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
    let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
    bus.map_device("UART", 0x1000_0000, 0x100, uart_adapter)
        .unwrap();

    let bus_arc = Arc::new(Mutex::new(bus));
    let mut cpu = Riscv64Cpu::with_bus(bus_arc).unwrap();

    cpu.mem_map(0x8000_0000, 0x10_000, Prot::ALL).unwrap();

    // RISC-V:
    // lui x1, 0x10000
    // addi x2, x0, 65 ('A')
    // sb x2, 0(x1)
    let code: [u8; 12] = [
        0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x10, 0x04, 0x23, 0x80, 0x20, 0x00,
    ];

    cpu.mem_write(0x8000_0000, &code).unwrap();
    cpu.set_pc(0x8000_0000).unwrap();

    let res = cpu.run(3).unwrap();
    assert_eq!(res, CpuStepResult::Ok { cycles: 3 });

    let output = uart.lock().unwrap().drain_output();
    assert_eq!(output, b"A");
}

#[test]
fn test_riscv64_mmio_uart_write_sequence() {
    let mut bus = SystemBus::new();
    let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
    bus.map_ram("RAM", 0x8000_0000, ram).unwrap();

    let uart = Arc::new(Mutex::new(Uart16550::new("uart0")));
    let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
    bus.map_device("UART", 0x1000_0000, 0x100, uart_adapter)
        .unwrap();

    let bus_arc = Arc::new(Mutex::new(bus));
    let mut cpu = Riscv64Cpu::with_bus(bus_arc).unwrap();

    cpu.mem_map(0x8000_0000, 0x10_000, Prot::ALL).unwrap();

    // RISC-V instructions:
    // lui x1, 0x10000    (0x100000b7)
    // addi x2, x0, 77    (0x04d00113 -> 'M')
    // sb x2, 0(x1)       (0x00208023)
    // addi x3, x0, 69    (0x04500193 -> 'E')
    // sb x3, 0(x1)       (0x00308023)
    // addi x4, x0, 76    (0x04c00213 -> 'L')
    // sb x4, 0(x1)       (0x00408023)
    let code: [u8; 28] = [
        0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0xd0, 0x04, 0x23, 0x80, 0x20, 0x00, 0x93, 0x01, 0x50,
        0x04, 0x23, 0x80, 0x30, 0x00, 0x13, 0x02, 0xc0, 0x04, 0x23, 0x80, 0x40, 0x00,
    ];

    cpu.mem_write(0x8000_0000, &code).unwrap();
    cpu.set_pc(0x8000_0000).unwrap();

    let res = cpu.run(7).unwrap();
    assert_eq!(res, CpuStepResult::Ok { cycles: 7 });

    let output = uart.lock().unwrap().drain_output();
    assert_eq!(output, b"MEL");
}

#[test]
fn test_riscv64_mmio_uart_read() {
    let mut bus = SystemBus::new();
    let ram = Arc::new(Mutex::new(Ram::new(0x10_000)));
    bus.map_ram("RAM", 0x8000_0000, ram).unwrap();

    let mut uart_dev = Uart16550::new("uart0");
    uart_dev.push_input(0x4D); // 'M'
    let uart = Arc::new(Mutex::new(uart_dev));
    let uart_adapter = Arc::new(Mutex::new(SharedMmioBusAdapter(uart.clone())));
    bus.map_device("UART", 0x1000_0000, 0x100, uart_adapter)
        .unwrap();

    let bus_arc = Arc::new(Mutex::new(bus));
    let mut cpu = Riscv64Cpu::with_bus(bus_arc).unwrap();

    cpu.mem_map(0x8000_0000, 0x10_000, Prot::ALL).unwrap();

    // lui x1, 0x10000   (0x100000b7)
    // lbu x3, 0(x1)     (0x0000c183)
    let code: [u8; 8] = [0xb7, 0x00, 0x00, 0x10, 0x83, 0xc1, 0x00, 0x00];

    cpu.mem_write(0x8000_0000, &code).unwrap();
    cpu.set_pc(0x8000_0000).unwrap();

    let res = cpu.run(2).unwrap();
    assert_eq!(res, CpuStepResult::Ok { cycles: 2 });

    let val = cpu.read_register(RegisterId::named("x3")).unwrap();
    assert_eq!(val, 0x4D);
}
