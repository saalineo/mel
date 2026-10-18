//! Automated test suite for RISC-V CPU interrupt and trap delivery subsystem.

use mel_core::RegisterId;
use mel_cpu::traits::{CpuBackend, CpuStepResult};
use mel_cpu::{
    MCAUSE_INTERRUPT_BIT, MCAUSE_MEI, MCAUSE_MSI, MCAUSE_MTI, MIE_MEIE, MIE_MSIE, MIE_MTIE,
    MSTATUS_MIE, MSTATUS_MPIE, Riscv64Cpu,
};
use unicorn_engine::unicorn_const::Prot;

#[test]
fn test_timer_trap_preemption_and_csr_transition() {
    let mut cpu = Riscv64Cpu::new().unwrap();
    let ram_base = 0x8000_0000;
    let ram_size = 0x10_000;
    cpu.mem_map(ram_base, ram_size, Prot::ALL).unwrap();

    // Assembly instructions:
    // 0x8000_0000: addi x1, x0, 10
    // 0x8000_0004: addi x2, x0, 20
    // 0x8000_0008: add  x3, x1, x2
    // 0x8000_000C: addi x4, x0, 30
    let mut code = Vec::new();
    code.extend_from_slice(&0x00a00093u32.to_le_bytes()); // addi x1, x0, 10
    code.extend_from_slice(&0x01400113u32.to_le_bytes()); // addi x2, x0, 20
    code.extend_from_slice(&0x002081b3u32.to_le_bytes()); // add x3, x1, x2
    code.extend_from_slice(&0x01e00213u32.to_le_bytes()); // addi x4, x0, 30

    cpu.mem_write(ram_base, &code).unwrap();
    cpu.set_pc(ram_base).unwrap();

    // Configure Trap Vector: Direct mode at 0x8000_1000
    let trap_vector = 0x8000_1000;
    cpu.set_mtvec(trap_vector);

    // Globally enable Machine Interrupts (MIE) and Timer Interrupts (MTIE)
    cpu.set_mstatus(MSTATUS_MIE);
    cpu.set_mie(MIE_MTIE);

    // Step 1: addi x1, x0, 10
    let res1 = cpu.step().unwrap();
    assert_eq!(res1, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(cpu.read_register(RegisterId::named("x1")).unwrap(), 10);
    assert_eq!(cpu.pc().unwrap(), ram_base + 4);

    // Assert Timer Interrupt line (MTIP)
    cpu.set_timer_interrupt(true);

    // Step 2: Preempted by Timer Trap!
    let res2 = cpu.step().unwrap();
    let expected_cause = MCAUSE_INTERRUPT_BIT | MCAUSE_MTI;
    assert_eq!(
        res2,
        CpuStepResult::Trap {
            cause: expected_cause,
            epc: ram_base + 4,
        }
    );

    // Verify architectural state after trap
    assert_eq!(cpu.pc().unwrap(), trap_vector);
    assert_eq!(cpu.mepc(), ram_base + 4);
    assert_eq!(cpu.mcause(), expected_cause);
    assert_eq!(
        cpu.read_register(RegisterId::named("mcause")).unwrap(),
        expected_cause
    );
    assert_eq!(
        cpu.read_register(RegisterId::named("mepc")).unwrap(),
        ram_base + 4
    );

    // mstatus.MIE must be 0 (disabled), mstatus.MPIE must be 1 (preserved previous state)
    assert_eq!(cpu.mstatus() & MSTATUS_MIE, 0);
    assert_ne!(cpu.mstatus() & MSTATUS_MPIE, 0);

    // Deassert timer line and return from trap (mret)
    cpu.set_timer_interrupt(false);
    cpu.mret().unwrap();

    // PC restored to mepc (0x8000_0004), MIE restored to 1
    assert_eq!(cpu.pc().unwrap(), ram_base + 4);
    assert_ne!(cpu.mstatus() & MSTATUS_MIE, 0);

    // Resume execution: addi x2, x0, 20
    let res3 = cpu.step().unwrap();
    assert_eq!(res3, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(cpu.read_register(RegisterId::named("x2")).unwrap(), 20);
    assert_eq!(cpu.pc().unwrap(), ram_base + 8);
}

#[test]
fn test_interrupt_masking_and_enable_bits() {
    let mut cpu = Riscv64Cpu::new().unwrap();
    let ram_base = 0x8000_0000;
    cpu.mem_map(ram_base, 0x1000, Prot::ALL).unwrap();

    let nops = vec![0x13, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00];
    cpu.mem_write(ram_base, &nops).unwrap();
    cpu.set_pc(ram_base).unwrap();
    cpu.set_mtvec(0x8000_0400);

    // 1. Timer interrupt asserted, but global MIE = 0 -> No trap
    cpu.set_mstatus(0); // MIE = 0
    cpu.set_mie(MIE_MTIE);
    cpu.set_timer_interrupt(true);

    let res = cpu.step().unwrap();
    assert_eq!(res, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(cpu.pc().unwrap(), ram_base + 4);

    // 2. Global MIE = 1, but mie.MTIE = 0 -> No trap
    cpu.set_mstatus(MSTATUS_MIE);
    cpu.set_mie(0); // MTIE = 0

    let res2 = cpu.step().unwrap();
    assert_eq!(res2, CpuStepResult::Ok { cycles: 1 });
    assert_eq!(cpu.pc().unwrap(), ram_base + 8);

    // 3. Both enabled -> Immediate trap
    cpu.set_mie(MIE_MTIE);
    let res3 = cpu.step().unwrap();
    assert_eq!(
        res3,
        CpuStepResult::Trap {
            cause: MCAUSE_INTERRUPT_BIT | MCAUSE_MTI,
            epc: ram_base + 8,
        }
    );
    assert_eq!(cpu.pc().unwrap(), 0x8000_0400);
}

#[test]
fn test_external_interrupt_and_priority_resolution() {
    let mut cpu = Riscv64Cpu::new().unwrap();
    let ram_base = 0x8000_0000;
    cpu.mem_map(ram_base, 0x1000, Prot::ALL).unwrap();

    let nops = vec![0x13, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00];
    cpu.mem_write(ram_base, &nops).unwrap();
    cpu.set_pc(ram_base).unwrap();
    cpu.set_mtvec(0x8000_0800);

    cpu.set_mstatus(MSTATUS_MIE);
    cpu.set_mie(MIE_MEIE | MIE_MSIE | MIE_MTIE);

    // Assert both MEIP (external) and MTIP (timer)
    cpu.set_timer_interrupt(true);
    cpu.set_external_interrupt(true);

    // External interrupt has highest priority (MEI = 11 > MSI = 3 > MTI = 7)
    let res = cpu.step().unwrap();
    assert_eq!(
        res,
        CpuStepResult::Trap {
            cause: MCAUSE_INTERRUPT_BIT | MCAUSE_MEI,
            epc: ram_base,
        }
    );
    assert_eq!(cpu.mcause(), MCAUSE_INTERRUPT_BIT | MCAUSE_MEI);

    // Clear external interrupt and return
    cpu.set_external_interrupt(false);
    cpu.mret().unwrap();

    // Now Timer interrupt is serviced
    let res2 = cpu.step().unwrap();
    assert_eq!(
        res2,
        CpuStepResult::Trap {
            cause: MCAUSE_INTERRUPT_BIT | MCAUSE_MTI,
            epc: ram_base,
        }
    );
    assert_eq!(cpu.mcause(), MCAUSE_INTERRUPT_BIT | MCAUSE_MTI);
}

#[test]
fn test_vectored_interrupt_mtvec_mode() {
    let mut cpu = Riscv64Cpu::new().unwrap();
    let ram_base = 0x8000_0000;
    cpu.mem_map(ram_base, 0x1000, Prot::ALL).unwrap();

    let nops = vec![0x13, 0x00, 0x00, 0x00];
    cpu.mem_write(ram_base, &nops).unwrap();
    cpu.set_pc(ram_base).unwrap();

    // Vectored mode: mtvec = base (0x8000_1000) | mode 1 = 0x8000_1001
    let vector_base = 0x8000_1000;
    cpu.set_mtvec(vector_base | 1);

    cpu.set_mstatus(MSTATUS_MIE);
    cpu.set_mie(MIE_MTIE);
    cpu.set_timer_interrupt(true);

    let res = cpu.step().unwrap();
    let expected_cause = MCAUSE_INTERRUPT_BIT | MCAUSE_MTI;
    assert_eq!(
        res,
        CpuStepResult::Trap {
            cause: expected_cause,
            epc: ram_base,
        }
    );

    // In Vectored mode, target PC = base + 4 * cause = 0x8000_1000 + 4 * 7 = 0x8000_101C
    let expected_vector_pc = vector_base + 4 * MCAUSE_MTI;
    assert_eq!(cpu.pc().unwrap(), expected_vector_pc);
}

#[test]
fn test_software_interrupt_delivery() {
    let mut cpu = Riscv64Cpu::new().unwrap();
    let ram_base = 0x8000_0000;
    cpu.mem_map(ram_base, 0x1000, Prot::ALL).unwrap();

    let nops = vec![0x13, 0x00, 0x00, 0x00];
    cpu.mem_write(ram_base, &nops).unwrap();
    cpu.set_pc(ram_base).unwrap();
    cpu.set_mtvec(0x8000_0600);

    cpu.set_mstatus(MSTATUS_MIE);
    cpu.set_mie(MIE_MSIE);
    cpu.set_software_interrupt(true);

    let res = cpu.step().unwrap();
    assert_eq!(
        res,
        CpuStepResult::Trap {
            cause: MCAUSE_INTERRUPT_BIT | MCAUSE_MSI,
            epc: ram_base,
        }
    );
    assert_eq!(cpu.mcause(), MCAUSE_INTERRUPT_BIT | MCAUSE_MSI);
    assert_eq!(cpu.pc().unwrap(), 0x8000_0600);
}
