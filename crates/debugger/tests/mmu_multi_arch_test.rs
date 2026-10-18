use mel_core::Architecture;
use mel_debugger::MmuInspector;
use std::collections::HashMap;

#[test]
fn test_mmu_multi_arch_inspection() {
    let mut mem: HashMap<u64, u64> = HashMap::new();

    // 1. RISC-V Sv39 Test
    let satp_val = (8u64 << 60) | 0x80000;
    // Map gigapage at 0x8000_0000
    mem.insert(0x8000_0010, (0x80000u64 << 10) | 0x0F);
    let riscv_res = MmuInspector::translate(Architecture::Riscv64, satp_val, 0x8000_0100, |addr| {
        mem.get(&addr)
            .copied()
            .ok_or_else(|| "Unmapped".to_string())
    })
    .expect("RISC-V translation failed");
    assert_eq!(riscv_res.arch, Architecture::Riscv64);
    assert_eq!(riscv_res.physical_address, 0x8000_0100);
    assert!(riscv_res.readable);
    assert!(riscv_res.writable);

    // 2. x86-64 4-Level Paging Test (CR3 = 0x1000)
    let cr3 = 0x1000;
    // vaddr: 0x0000_7FFF_1234_5678 -> PML4 index 255 (0xFF), PDPT index 508, PD index 290, PT index 341
    let vaddr_x86 = 0x0000_7FFF_1234_5678;
    let pml4e_addr = 0x1000 + ((vaddr_x86 >> 39) & 0x1FF) * 8;
    mem.insert(pml4e_addr, 0x2000 | 0x07); // PDPT at 0x2000
    let pdpte_addr = 0x2000 + ((vaddr_x86 >> 30) & 0x1FF) * 8;
    mem.insert(pdpte_addr, 0x3000 | 0x07); // PD at 0x3000
    let pde_addr = 0x3000 + ((vaddr_x86 >> 21) & 0x1FF) * 8;
    mem.insert(pde_addr, 0x4000 | 0x07); // PT at 0x4000
    let pte_addr = 0x4000 + ((vaddr_x86 >> 12) & 0x1FF) * 8;
    mem.insert(pte_addr, 0x0010_0000 | 0x07); // physical base 0x100000
    let x86_res = MmuInspector::translate(Architecture::X86_64, cr3, vaddr_x86, |addr| {
        mem.get(&addr)
            .copied()
            .ok_or_else(|| "Unmapped".to_string())
    })
    .expect("x86-64 translation failed");
    assert_eq!(x86_res.arch, Architecture::X86_64);
    assert_eq!(x86_res.physical_address, 0x0010_0000 | (vaddr_x86 & 0xFFF));
    assert_eq!(x86_res.page_size_bytes, 4096);
    assert!(x86_res.writable);

    // 3. ARM64 TTBR0 Paging Test (TTBR0 = 0x5000)
    let ttbr0 = 0x5000;
    let vaddr_arm = 0x0000_0040_0000_1234;
    let l1_addr = 0x5000 + ((vaddr_arm >> 30) & 0x1FF) * 8;
    mem.insert(l1_addr, 0x6000 | 0x03); // table at 0x6000
    let l2_addr = 0x6000 + ((vaddr_arm >> 21) & 0x1FF) * 8;
    mem.insert(l2_addr, 0x7000 | 0x03); // table at 0x7000
    let l3_addr = 0x7000 + ((vaddr_arm >> 12) & 0x1FF) * 8;
    mem.insert(l3_addr, 0x4000_0000 | 0x03); // physical page 0x4000_0000
    let arm_res = MmuInspector::translate(Architecture::Arm64, ttbr0, vaddr_arm, |addr| {
        mem.get(&addr)
            .copied()
            .ok_or_else(|| "Unmapped".to_string())
    })
    .expect("ARM64 translation failed");
    assert_eq!(arm_res.arch, Architecture::Arm64);
    assert_eq!(arm_res.physical_address, 0x4000_0000 | 0x234);
    assert_eq!(arm_res.page_size_bytes, 4096);

    // 4. Power ISA Radix Paging Test
    let root_ppc = 0x9000;
    let vaddr_ppc = 0x0000_0000_1234_5678;
    let pte_addr_ppc = 0x9000 + ((vaddr_ppc >> 16) & 0x1FF) * 8;
    mem.insert(pte_addr_ppc, 0x0000_0000_8000_0000 | 0x07);
    let ppc_res = MmuInspector::translate(Architecture::PowerIso64, root_ppc, vaddr_ppc, |addr| {
        mem.get(&addr)
            .copied()
            .ok_or_else(|| "Unmapped".to_string())
    })
    .expect("Power ISA translation failed");
    assert_eq!(ppc_res.arch, Architecture::PowerIso64);
    assert_eq!(ppc_res.physical_address, 0x8000_0000 | 0x5678);
    assert_eq!(ppc_res.page_size_bytes, 65536);
}
