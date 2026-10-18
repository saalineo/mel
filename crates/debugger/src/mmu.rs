//! Unified Multi-Architecture MMU Paging Inspector.

use mel_core::Architecture;
use serde::{Deserialize, Serialize};

use crate::sv39::{PageSize, Satp, Sv39Validator};

/// Unified architecture-neutral translation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResult {
    pub arch: Architecture,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub page_size_bytes: u64,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user: bool,
    pub levels_walked: usize,
    pub details: String,
}

/// Unified MMU inspector across RISC-V, ARM64, x86-64, and Power ISA.
pub struct MmuInspector;

impl MmuInspector {
    /// Inspect and translate a virtual address across supported architectures.
    pub fn translate<F>(
        arch: Architecture,
        root_register: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<TranslationResult, String>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        match arch {
            Architecture::Riscv64 => {
                Self::translate_riscv_sv39(root_register, vaddr, read_phys_u64)
            }
            Architecture::X86_64 => Self::translate_x86_64(root_register, vaddr, read_phys_u64),
            Architecture::Arm64 => Self::translate_arm64(root_register, vaddr, read_phys_u64),
            Architecture::PowerIso64 => {
                Self::translate_powerisa(root_register, vaddr, read_phys_u64)
            }
        }
    }

    fn translate_riscv_sv39<F>(
        satp_raw: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<TranslationResult, String>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        let satp = Satp::from_u64(satp_raw);
        let trans = Sv39Validator::walk(satp_raw, vaddr, read_phys_u64)
            .map_err(|e| format!("RISC-V Sv39 Translation Fault: {e:?}"))?;

        let size_bytes = match trans.page_size {
            PageSize::Page4KB => 4096,
            PageSize::Megapage2MB => 2 * 1024 * 1024,
            PageSize::Gigapage1GB => 1024 * 1024 * 1024,
        };

        Ok(TranslationResult {
            arch: Architecture::Riscv64,
            virtual_address: vaddr,
            physical_address: trans.physical_address,
            page_size_bytes: size_bytes,
            readable: trans.pte.flags.read,
            writable: trans.pte.flags.write,
            executable: trans.pte.flags.execute,
            user: trans.pte.flags.user,
            levels_walked: trans.levels_walked,
            details: format!(
                "Sv39 satp={:#x} (mode={}, asid={}) -> PTE={:#x}",
                satp_raw, satp.mode, satp.asid, trans.pte.raw
            ),
        })
    }

    /// x86-64 4-level paging (PML4 -> PDPT -> PD -> PT -> 4KB Page).
    fn translate_x86_64<F>(
        cr3: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<TranslationResult, String>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        let pml4_base = cr3 & 0x000F_FFFF_FFFF_F000;
        let pml4_idx = (vaddr >> 39) & 0x1FF;
        let pdpt_idx = (vaddr >> 30) & 0x1FF;
        let pd_idx = (vaddr >> 21) & 0x1FF;
        let pt_idx = (vaddr >> 12) & 0x1FF;
        let offset = vaddr & 0xFFF;

        // Level 4: PML4
        let pml4e_addr = pml4_base + pml4_idx * 8;
        let pml4e = read_phys_u64(pml4e_addr)?;
        if (pml4e & 1) == 0 {
            return Err(format!("x86-64 PML4 entry not present at {pml4e_addr:#x}"));
        }

        // Level 3: PDPT
        let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpte_addr = pdpt_base + pdpt_idx * 8;
        let pdpte = read_phys_u64(pdpte_addr)?;
        if (pdpte & 1) == 0 {
            return Err(format!("x86-64 PDPT entry not present at {pdpte_addr:#x}"));
        }
        // Check 1GB page (PS bit 7)
        if (pdpte & (1 << 7)) != 0 {
            let paddr = (pdpte & 0x000F_FFFF_C000_0000) | (vaddr & 0x3FFF_FFFF);
            return Ok(TranslationResult {
                arch: Architecture::X86_64,
                virtual_address: vaddr,
                physical_address: paddr,
                page_size_bytes: 1024 * 1024 * 1024,
                readable: true,
                writable: (pdpte & 2) != 0,
                executable: (pdpte & (1u64 << 63)) == 0,
                user: (pdpte & 4) != 0,
                levels_walked: 2,
                details: format!("x86-64 1GB superpage via PDPT={pdpte:#x}"),
            });
        }

        // Level 2: PD
        let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
        let pde_addr = pd_base + pd_idx * 8;
        let pde = read_phys_u64(pde_addr)?;
        if (pde & 1) == 0 {
            return Err(format!("x86-64 PD entry not present at {pde_addr:#x}"));
        }
        // Check 2MB page (PS bit 7)
        if (pde & (1 << 7)) != 0 {
            let paddr = (pde & 0x000F_FFFF_FFE0_0000) | (vaddr & 0x1F_FFFF);
            return Ok(TranslationResult {
                arch: Architecture::X86_64,
                virtual_address: vaddr,
                physical_address: paddr,
                page_size_bytes: 2 * 1024 * 1024,
                readable: true,
                writable: (pde & 2) != 0,
                executable: (pde & (1u64 << 63)) == 0,
                user: (pde & 4) != 0,
                levels_walked: 3,
                details: format!("x86-64 2MB superpage via PDE={pde:#x}"),
            });
        }

        // Level 1: PT
        let pt_base = pde & 0x000F_FFFF_FFFF_F000;
        let pte_addr = pt_base + pt_idx * 8;
        let pte = read_phys_u64(pte_addr)?;
        if (pte & 1) == 0 {
            return Err(format!("x86-64 PT entry not present at {pte_addr:#x}"));
        }

        let paddr = (pte & 0x000F_FFFF_FFFF_F000) | offset;
        Ok(TranslationResult {
            arch: Architecture::X86_64,
            virtual_address: vaddr,
            physical_address: paddr,
            page_size_bytes: 4096,
            readable: true,
            writable: (pte & 2) != 0,
            executable: (pte & (1u64 << 63)) == 0, // NX bit 63
            user: (pte & 4) != 0,
            levels_walked: 4,
            details: format!("x86-64 4KB page via PTE={pte:#x}"),
        })
    }

    /// ARM64 4KB Granule Translation (TTBR0 -> L1 -> L2 -> L3 -> 4KB Page).
    fn translate_arm64<F>(
        ttbr0: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<TranslationResult, String>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        let root_base = ttbr0 & 0x0000_FFFF_FFFF_F000;
        let l1_idx = (vaddr >> 30) & 0x1FF;
        let l2_idx = (vaddr >> 21) & 0x1FF;
        let l3_idx = (vaddr >> 12) & 0x1FF;
        let offset = vaddr & 0xFFF;

        // Level 1 Descriptor
        let l1_addr = root_base + l1_idx * 8;
        let l1_desc = read_phys_u64(l1_addr)?;
        if (l1_desc & 1) == 0 {
            return Err(format!("ARM64 L1 descriptor invalid at {l1_addr:#x}"));
        }
        // Check 1GB block
        if (l1_desc & 2) == 0 {
            let paddr = (l1_desc & 0x0000_FFFF_C000_0000) | (vaddr & 0x3FFF_FFFF);
            return Ok(TranslationResult {
                arch: Architecture::Arm64,
                virtual_address: vaddr,
                physical_address: paddr,
                page_size_bytes: 1024 * 1024 * 1024,
                readable: true,
                writable: (l1_desc & (1 << 7)) == 0, // AP[2] = 0 is read/write
                executable: (l1_desc & (1u64 << 54)) == 0, // UXN
                user: (l1_desc & (1 << 6)) != 0,
                levels_walked: 1,
                details: format!("ARM64 1GB block at L1={l1_desc:#x}"),
            });
        }

        // Level 2 Descriptor
        let l2_base = l1_desc & 0x0000_FFFF_FFFF_F000;
        let l2_addr = l2_base + l2_idx * 8;
        let l2_desc = read_phys_u64(l2_addr)?;
        if (l2_desc & 1) == 0 {
            return Err(format!("ARM64 L2 descriptor invalid at {l2_addr:#x}"));
        }
        // Check 2MB block
        if (l2_desc & 2) == 0 {
            let paddr = (l2_desc & 0x0000_FFFF_FFE0_0000) | (vaddr & 0x1F_FFFF);
            return Ok(TranslationResult {
                arch: Architecture::Arm64,
                virtual_address: vaddr,
                physical_address: paddr,
                page_size_bytes: 2 * 1024 * 1024,
                readable: true,
                writable: (l2_desc & (1 << 7)) == 0,
                executable: (l2_desc & (1u64 << 54)) == 0,
                user: (l2_desc & (1 << 6)) != 0,
                levels_walked: 2,
                details: format!("ARM64 2MB block at L2={l2_desc:#x}"),
            });
        }

        // Level 3 Descriptor (4KB Page)
        let l3_base = l2_desc & 0x0000_FFFF_FFFF_F000;
        let l3_addr = l3_base + l3_idx * 8;
        let l3_desc = read_phys_u64(l3_addr)?;
        if (l3_desc & 1) == 0 {
            return Err(format!("ARM64 L3 descriptor invalid at {l3_addr:#x}"));
        }

        let paddr = (l3_desc & 0x0000_FFFF_FFFF_F000) | offset;
        Ok(TranslationResult {
            arch: Architecture::Arm64,
            virtual_address: vaddr,
            physical_address: paddr,
            page_size_bytes: 4096,
            readable: true,
            writable: (l3_desc & (1 << 7)) == 0,
            executable: (l3_desc & (1u64 << 54)) == 0,
            user: (l3_desc & (1 << 6)) != 0,
            levels_walked: 3,
            details: format!("ARM64 4KB page at L3={l3_desc:#x}"),
        })
    }

    /// Power ISA Radix Tree Paging.
    fn translate_powerisa<F>(
        root_base: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<TranslationResult, String>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        // 64KB Radix Paging: root table -> Level 1 -> PTE
        let idx = (vaddr >> 16) & 0x1FF;
        let offset = vaddr & 0xFFFF;
        let pte_addr = root_base + idx * 8;
        let pte = read_phys_u64(pte_addr)?;
        if (pte & 1) == 0 {
            return Err(format!("Power ISA PTE not valid at {pte_addr:#x}"));
        }

        let paddr = (pte & 0x000F_FFFF_FFFF_0000) | offset;
        Ok(TranslationResult {
            arch: Architecture::PowerIso64,
            virtual_address: vaddr,
            physical_address: paddr,
            page_size_bytes: 65536,
            readable: (pte & 2) != 0,
            writable: (pte & 4) != 0,
            executable: (pte & 8) != 0,
            user: (pte & 16) != 0,
            levels_walked: 1,
            details: format!("Power ISA Radix PTE={pte:#x}"),
        })
    }
}
