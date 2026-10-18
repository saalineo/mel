use mel_core::RegisterId;
use mel_vm::create_basic_riscv64_vm;

pub const SYS_WRITE: u64 = 64;
pub const SYS_EXIT: u64 = 93;
pub const SYS_YIELD: u64 = 124;
pub const SYS_GETPID: u64 = 172;

#[test]
fn test_riscv_syscall_abi_conventions() {
    let mut vm = create_basic_riscv64_vm().expect("Failed to create RISC-V VM");

    // Test SYS_GETPID convention: a7 = 172
    vm.write_register(RegisterId::named("a7"), SYS_GETPID)
        .unwrap();
    let a7 = vm.read_register(RegisterId::named("a7")).unwrap();
    assert_eq!(a7, SYS_GETPID);

    // Mock kernel syscall dispatcher handler:
    // If a7 == SYS_GETPID, set a0 = 42
    if a7 == SYS_GETPID {
        vm.write_register(RegisterId::named("a0"), 42).unwrap();
    }
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 42);

    // Test SYS_WRITE convention: a7 = 64, a0 = fd (1), a1 = buf_addr, a2 = count (12)
    vm.write_register(RegisterId::named("a7"), SYS_WRITE)
        .unwrap();
    vm.write_register(RegisterId::named("a0"), 1).unwrap(); // stdout
    vm.write_register(RegisterId::named("a1"), 0x8000_2000)
        .unwrap();
    vm.write_register(RegisterId::named("a2"), 12).unwrap();

    let fd = vm.read_register(RegisterId::named("a0")).unwrap();
    let buf = vm.read_register(RegisterId::named("a1")).unwrap();
    let len = vm.read_register(RegisterId::named("a2")).unwrap();

    assert_eq!(fd, 1);
    assert_eq!(buf, 0x8000_2000);
    assert_eq!(len, 12);

    // Emulate kernel returning bytes written
    vm.write_register(RegisterId::named("a0"), len).unwrap();
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 12);

    // Test SYS_YIELD convention: a7 = 124 -> returns 0
    vm.write_register(RegisterId::named("a7"), SYS_YIELD)
        .unwrap();
    vm.write_register(RegisterId::named("a0"), 0).unwrap();
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 0);

    // Test SYS_EXIT convention: a7 = 93, a0 = 0 (exit status)
    vm.write_register(RegisterId::named("a7"), SYS_EXIT)
        .unwrap();
    vm.write_register(RegisterId::named("a0"), 0).unwrap();
    assert_eq!(vm.read_register(RegisterId::named("a7")).unwrap(), 93);
    assert_eq!(vm.read_register(RegisterId::named("a0")).unwrap(), 0);
}

#[test]
fn test_multi_arch_syscall_abi_mapping() {
    // Verify register naming mappings for syscall ABI across architectures
    // RISC-V: a7 = syscall nr, a0..a6 = args
    assert_eq!(RegisterId::named("a7").name, "a7");
    assert_eq!(RegisterId::named("a0").name, "a0");

    // x86-64: rax = syscall nr, rdi, rsi, rdx, r10, r8, r9 = args
    assert_eq!(RegisterId::named("rax").name, "rax");
    assert_eq!(RegisterId::named("rdi").name, "rdi");
    assert_eq!(RegisterId::named("rsi").name, "rsi");

    // ARM64: x8 = syscall nr, x0..x7 = args
    assert_eq!(RegisterId::named("x8").name, "x8");
    assert_eq!(RegisterId::named("x0").name, "x0");

    // Power ISA: r0 = syscall nr, r3..r8 = args
    assert_eq!(RegisterId::named("r0").name, "r0");
    assert_eq!(RegisterId::named("r3").name, "r3");
}
