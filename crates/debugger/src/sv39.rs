//! RISC-V Sv39 Virtual Memory Translation and Page Table Validator.

use serde::{Deserialize, Serialize};

/// Decoded RISC-V `satp` register representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Satp {
    pub raw: u64,
    pub mode: u8,
    pub asid: u16,
    pub ppn: u64,
}

impl Satp {
    pub const MODE_BARE: u8 = 0;
    pub const MODE_SV39: u8 = 8;
    pub const MODE_SV48: u8 = 9;

    pub fn from_u64(raw: u64) -> Self {
        let mode = ((raw >> 60) & 0xF) as u8;
        let asid = ((raw >> 44) & 0xFFFF) as u16;
        let ppn = raw & 0x0FFF_FFFF_FFFF;
        Self {
            raw,
            mode,
            asid,
            ppn,
        }
    }

    pub fn root_page_table_addr(&self) -> u64 {
        self.ppn << 12
    }
}

/// Sv39 Page Table Entry (PTE) permission and control flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PteFlags {
    pub valid: bool,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub user: bool,
    pub global: bool,
    pub accessed: bool,
    pub dirty: bool,
    pub rsw: u8,
}

impl PteFlags {
    pub fn from_raw(raw: u64) -> Self {
        Self {
            valid: (raw & 1) != 0,
            read: (raw & 2) != 0,
            write: (raw & 4) != 0,
            execute: (raw & 8) != 0,
            user: (raw & 16) != 0,
            global: (raw & 32) != 0,
            accessed: (raw & 64) != 0,
            dirty: (raw & 128) != 0,
            rsw: ((raw >> 8) & 3) as u8,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.read || self.execute
    }

    pub fn is_table_pointer(&self) -> bool {
        !self.read && !self.write && !self.execute
    }
}

/// Decoded Sv39 Page Table Entry (PTE).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTableEntry {
    pub raw: u64,
    pub flags: PteFlags,
    pub ppn: u64,
    pub physical_address: u64,
}

impl PageTableEntry {
    pub fn from_raw(raw: u64) -> Self {
        let flags = PteFlags::from_raw(raw);
        let ppn = (raw >> 10) & 0x003F_FFFF_FFFF_FFFF;
        let physical_address = ppn << 12;
        Self {
            raw,
            flags,
            ppn,
            physical_address,
        }
    }
}

/// Page granularity decoded during translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSize {
    Page4KB,
    Megapage2MB,
    Gigapage1GB,
}

/// Successful Sv39 translation resolution report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sv39Translation {
    pub virtual_address: u64,
    pub physical_address: u64,
    pub page_size: PageSize,
    pub pte: PageTableEntry,
    pub levels_walked: usize,
}

/// Sv39 translation or validation failure reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sv39Fault {
    BareMode,
    InvalidMode(u8),
    SignExtensionFault(u64),
    PteInvalid {
        level: usize,
        pte_addr: u64,
        raw_pte: u64,
    },
    PteWriteWithoutRead {
        level: usize,
        pte_addr: u64,
        raw_pte: u64,
    },
    MisalignedSuperpage {
        level: usize,
        pte: PageTableEntry,
    },
    MemoryReadError {
        addr: u64,
        message: String,
    },
    PointerAtLevelZero {
        pte_addr: u64,
        raw_pte: u64,
    },
}

/// Sv39 3-level page table walker and validator.
pub struct Sv39Validator;

impl Sv39Validator {
    /// Walk the 3-level Sv39 page table to translate `vaddr` to physical address.
    pub fn walk<F>(
        satp_raw: u64,
        vaddr: u64,
        read_phys_u64: F,
    ) -> Result<Sv39Translation, Sv39Fault>
    where
        F: Fn(u64) -> Result<u64, String>,
    {
        let satp = Satp::from_u64(satp_raw);
        if satp.mode == Satp::MODE_BARE {
            return Err(Sv39Fault::BareMode);
        }
        if satp.mode != Satp::MODE_SV39 {
            return Err(Sv39Fault::InvalidMode(satp.mode));
        }

        // Validate canonical Sv39 sign-extension: bits 63..39 must match bit 38
        let bit38 = (vaddr >> 38) & 1;
        let upper_bits = vaddr >> 39;
        let expected_upper = if bit38 == 1 { 0x1FFFFFF } else { 0 };
        if upper_bits != expected_upper {
            return Err(Sv39Fault::SignExtensionFault(vaddr));
        }

        let vpn = [
            (vaddr >> 12) & 0x1FF, // VPN[0]
            (vaddr >> 21) & 0x1FF, // VPN[1]
            (vaddr >> 30) & 0x1FF, // VPN[2]
        ];
        let offset = vaddr & 0xFFF;

        let mut a = satp.root_page_table_addr();

        // Walk levels i = 2 down to 0
        for i in (0..=2).rev() {
            let pte_addr = a.wrapping_add(vpn[i].wrapping_mul(8));
            let raw_pte = read_phys_u64(pte_addr).map_err(|e| Sv39Fault::MemoryReadError {
                addr: pte_addr,
                message: e,
            })?;

            let pte = PageTableEntry::from_raw(raw_pte);

            // Step 1: Valid check
            if !pte.flags.valid {
                return Err(Sv39Fault::PteInvalid {
                    level: i,
                    pte_addr,
                    raw_pte,
                });
            }

            // Step 2: Write without Read is reserved/invalid
            if pte.flags.write && !pte.flags.read {
                return Err(Sv39Fault::PteWriteWithoutRead {
                    level: i,
                    pte_addr,
                    raw_pte,
                });
            }

            // Step 3: Check if leaf or pointer
            if pte.flags.is_leaf() {
                // Leaf PTE found!
                match i {
                    2 => {
                        // 1 GB gigapage: PPN[1:0] must be 0
                        if (pte.ppn & 0x3FFFF) != 0 {
                            return Err(Sv39Fault::MisalignedSuperpage { level: 2, pte });
                        }
                        let paddr = (pte.ppn << 12) | (vaddr & 0x3FFF_FFFF);
                        return Ok(Sv39Translation {
                            virtual_address: vaddr,
                            physical_address: paddr,
                            page_size: PageSize::Gigapage1GB,
                            pte,
                            levels_walked: 1,
                        });
                    }
                    1 => {
                        // 2 MB megapage: PPN[0] must be 0
                        if (pte.ppn & 0x1FF) != 0 {
                            return Err(Sv39Fault::MisalignedSuperpage { level: 1, pte });
                        }
                        let paddr = (pte.ppn << 12) | (vaddr & 0x1F_FFFF);
                        return Ok(Sv39Translation {
                            virtual_address: vaddr,
                            physical_address: paddr,
                            page_size: PageSize::Megapage2MB,
                            pte,
                            levels_walked: 2,
                        });
                    }
                    0 => {
                        // 4 KB page
                        let paddr = (pte.ppn << 12) | offset;
                        return Ok(Sv39Translation {
                            virtual_address: vaddr,
                            physical_address: paddr,
                            page_size: PageSize::Page4KB,
                            pte,
                            levels_walked: 3,
                        });
                    }
                    _ => unreachable!(),
                }
            } else {
                // Non-leaf pointer to next level
                if i == 0 {
                    return Err(Sv39Fault::PointerAtLevelZero { pte_addr, raw_pte });
                }
                a = pte.physical_address;
            }
        }

        Err(Sv39Fault::PteInvalid {
            level: 0,
            pte_addr: a,
            raw_pte: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_satp_parsing() {
        // Mode 8 (Sv39), ASID 0x12, PPN 0x80000
        let satp_val = (8u64 << 60) | (0x12u64 << 44) | 0x80000;
        let satp = Satp::from_u64(satp_val);
        assert_eq!(satp.mode, 8);
        assert_eq!(satp.asid, 0x12);
        assert_eq!(satp.ppn, 0x80000);
        assert_eq!(satp.root_page_table_addr(), 0x8000_0000);
    }

    #[test]
    fn test_sv39_gigapage_translation() {
        let mut mem: HashMap<u64, u64> = HashMap::new();
        let satp_val = (8u64 << 60) | 0x80000; // root table at 0x8000_0000

        // Map vaddr 0x8000_0000 (VPN[2] = 2) as 1GB gigapage identity to 0x8000_0000
        // PTE at 0x8000_0000 + 2*8 = 0x8000_0010
        // PPN = 0x80000 (physical addr 0x8000_0000), flags: V=1, R=1, W=1, X=1 (0xF)
        let pte_raw = (0x80000u64 << 10) | 0x0F;
        mem.insert(0x8000_0010, pte_raw);

        let trans = Sv39Validator::walk(satp_val, 0x8000_0124, |addr| {
            mem.get(&addr)
                .copied()
                .ok_or_else(|| "Unmapped".to_string())
        })
        .unwrap();

        assert_eq!(trans.page_size, PageSize::Gigapage1GB);
        assert_eq!(trans.physical_address, 0x8000_0124);
        assert_eq!(trans.levels_walked, 1);
    }

    #[test]
    fn test_sv39_4kb_page_translation() {
        let mut mem: HashMap<u64, u64> = HashMap::new();
        let satp_val = (8u64 << 60) | 0x80000; // root table at 0x8000_0000

        // Virtual address: 0x1000_2345
        // VPN[2] = 0 -> root table index 0
        // Level 2 PTE: pointer to Level 1 table at 0x8000_1000 (PPN = 0x80001), V=1
        let l2_pte = (0x80001u64 << 10) | 0x01; // pointer (R=0, W=0, X=0, V=1)
        mem.insert(0x8000_0000, l2_pte);

        // VPN[1] = (0x1000_2345 >> 21) & 0x1FF = 0x80 = 128
        // Level 1 PTE at 0x8000_1000 + 128*8 = 0x8000_1400: pointer to Level 0 table at 0x8000_2000 (PPN = 0x80002)
        let l1_pte = (0x80002u64 << 10) | 0x01;
        mem.insert(0x8000_1400, l1_pte);

        // VPN[0] = (0x1000_2345 >> 12) & 0x1FF = 2
        // Level 0 PTE at 0x8000_2000 + 2*8 = 0x8000_2010: maps to physical 0x8005_0000 (PPN = 0x80050), R=1, W=1, V=1 (0x07)
        let l0_pte = (0x80050u64 << 10) | 0x07;
        mem.insert(0x8000_2010, l0_pte);

        let trans = Sv39Validator::walk(satp_val, 0x1000_2345, |addr| {
            mem.get(&addr)
                .copied()
                .ok_or_else(|| "Unmapped".to_string())
        })
        .unwrap();

        assert_eq!(trans.page_size, PageSize::Page4KB);
        assert_eq!(trans.physical_address, 0x8005_0345);
        assert_eq!(trans.levels_walked, 3);
        assert!(trans.pte.flags.read);
        assert!(trans.pte.flags.write);
    }

    #[test]
    fn test_sv39_faults() {
        let _mem: HashMap<u64, u64> = HashMap::new();

        // 1. Bare mode error
        let err = Sv39Validator::walk(0, 0x8000_0000, |_| Ok(0)).unwrap_err();
        assert_eq!(err, Sv39Fault::BareMode);

        // 2. Sign extension fault
        let satp_val = (8u64 << 60) | 0x80000;
        let err2 = Sv39Validator::walk(satp_val, 0x0000_0080_0000_0000, |_| Ok(0)).unwrap_err();
        assert!(matches!(err2, Sv39Fault::SignExtensionFault(_)));

        // 3. Invalid PTE
        let err3 = Sv39Validator::walk(satp_val, 0x0, |_| Ok(0)).unwrap_err(); // raw_pte = 0 -> V=0
        assert!(matches!(err3, Sv39Fault::PteInvalid { .. }));
    }
}
