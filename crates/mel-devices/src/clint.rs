//! RISC-V CLINT (Core Local Interruptor) MMIO peripheral.
//!
//! Provides memory-mapped registers for timer facilities (`mtime`, `mtimecmp`)
//! and software interrupts (`msip`) following the RISC-V privileged specification
//! and SiFive CLINT standard register layout.

use crate::device_trait::MmioDevice;
use mel_core::{Endianness, MelError, MemoryError};
use serde::{Deserialize, Serialize};

/// Standard CLINT base address on RISC-V platforms.
pub const CLINT_BASE: u64 = 0x0200_0000;

/// Standard CLINT MMIO window size (64 KB).
pub const CLINT_SIZE: u64 = 0x10000;

/// Base offset for Hart 0 MSIP (Machine Software Interrupt Pending) register (4 bytes per hart).
pub const CLINT_MSIP_BASE: u64 = 0x0000;

/// Base offset for Hart 0 MTIMECMP (Machine Timer Compare) register (8 bytes per hart).
pub const CLINT_MTIMECMP_BASE: u64 = 0x4000;

/// Offset for real-time timer register MTIME (8 bytes, shared across all harts).
pub const CLINT_MTIME: u64 = 0xBFF8;

/// RISC-V Core Local Interruptor (CLINT) peripheral.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clint {
    name: String,
    size: u64,
    msip: Vec<u32>,
    mtimecmp: Vec<u64>,
    mtime: u64,
}

impl Clint {
    /// Create a new CLINT peripheral with 1 hart and a 64 KB address window.
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_harts(name, 1)
    }

    /// Create a new CLINT peripheral for `num_harts` hardware threads.
    pub fn with_harts(name: impl Into<String>, num_harts: usize) -> Self {
        let harts = num_harts.max(1);
        Self {
            name: name.into(),
            size: CLINT_SIZE,
            msip: vec![0; harts],
            mtimecmp: vec![0; harts],
            mtime: 0,
        }
    }

    /// Number of configured harts.
    pub fn num_harts(&self) -> usize {
        self.msip.len()
    }

    /// Current real-time timer count `mtime`.
    pub fn mtime(&self) -> u64 {
        self.mtime
    }

    /// Set `mtime` timer count.
    pub fn set_mtime(&mut self, val: u64) {
        self.mtime = val;
    }

    /// Hart 0 timer compare value `mtimecmp`.
    pub fn mtimecmp(&self) -> u64 {
        self.mtimecmp_hart(0).unwrap_or(0)
    }

    /// Set Hart 0 timer compare value `mtimecmp`.
    pub fn set_mtimecmp(&mut self, val: u64) {
        let _ = self.set_mtimecmp_hart(0, val);
    }

    /// Read `mtimecmp` for a specific hart index.
    pub fn mtimecmp_hart(&self, hart: usize) -> Option<u64> {
        self.mtimecmp.get(hart).copied()
    }

    /// Set `mtimecmp` for a specific hart index.
    pub fn set_mtimecmp_hart(&mut self, hart: usize, val: u64) -> Result<(), MelError> {
        if let Some(target) = self.mtimecmp.get_mut(hart) {
            *target = val;
            Ok(())
        } else {
            Err(MelError::Device {
                device: self.name.clone(),
                message: format!("Invalid hart index {hart} (total: {})", self.msip.len()),
            })
        }
    }

    /// Hart 0 software interrupt pending bit `msip`.
    pub fn msip(&self) -> u32 {
        self.msip_hart(0).unwrap_or(0)
    }

    /// Set Hart 0 software interrupt pending bit `msip`.
    pub fn set_msip(&mut self, val: u32) {
        let _ = self.set_msip_hart(0, val);
    }

    /// Read `msip` for a specific hart index.
    pub fn msip_hart(&self, hart: usize) -> Option<u32> {
        self.msip.get(hart).map(|v| *v & 1)
    }

    /// Set `msip` for a specific hart index.
    pub fn set_msip_hart(&mut self, hart: usize, val: u32) -> Result<(), MelError> {
        if let Some(target) = self.msip.get_mut(hart) {
            *target = val & 1;
            Ok(())
        } else {
            Err(MelError::Device {
                device: self.name.clone(),
                message: format!("Invalid hart index {hart} (total: {})", self.msip.len()),
            })
        }
    }

    /// Check if machine timer interrupt (MTIP) is pending for Hart 0 (`mtime >= mtimecmp`).
    pub fn is_timer_interrupt_pending(&self) -> bool {
        self.is_timer_interrupt_pending_hart(0)
    }

    /// Check if machine timer interrupt (MTIP) is pending for a specific hart.
    pub fn is_timer_interrupt_pending_hart(&self, hart: usize) -> bool {
        if let Some(cmp) = self.mtimecmp_hart(hart) {
            self.mtime >= cmp
        } else {
            false
        }
    }

    /// Check if machine software interrupt (MSIP) is pending for Hart 0.
    pub fn is_software_interrupt_pending(&self) -> bool {
        self.is_software_interrupt_pending_hart(0)
    }

    /// Check if machine software interrupt (MSIP) is pending for a specific hart.
    pub fn is_software_interrupt_pending_hart(&self, hart: usize) -> bool {
        self.msip_hart(hart).map(|v| v != 0).unwrap_or(false)
    }

    /// Returns true if any interrupt (timer or software) is asserted for Hart 0.
    pub fn is_interrupting(&self) -> bool {
        self.is_timer_interrupt_pending() || self.is_software_interrupt_pending()
    }

    /// Returns true if any interrupt is asserted for any configured hart.
    pub fn has_pending_interrupts(&self) -> bool {
        for hart in 0..self.num_harts() {
            if self.is_timer_interrupt_pending_hart(hart)
                || self.is_software_interrupt_pending_hart(hart)
            {
                return true;
            }
        }
        false
    }
}

impl MmioDevice for Clint {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, MelError> {
        if size == 0 || size > 8 || offset.saturating_add(size as u64) > self.size {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset,
                size: self.size as usize,
            }));
        }

        // 1. MSIP Register Region (0x0000 - 0x3FFF, 4 bytes per hart)
        if offset < CLINT_MTIMECMP_BASE {
            let hart = (offset / 4) as usize;
            let reg_offset = (offset % 4) as usize;
            if hart < self.msip.len() {
                let msip_val = self.msip[hart] & 1;
                let val_bytes = msip_val.to_le_bytes();
                let mut buf = [0u8; 8];
                let copy_len = size.min(4usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 4 {
                    buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
                }
                return Endianness::Little
                    .read_u64(&buf)
                    .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
            }
            return Ok(0);
        }

        // 2. MTIMECMP Register Region (0x4000 - 0xBFF7, 8 bytes per hart)
        if (CLINT_MTIMECMP_BASE..CLINT_MTIME).contains(&offset) {
            let relative = offset - CLINT_MTIMECMP_BASE;
            let hart = (relative / 8) as usize;
            let reg_offset = (relative % 8) as usize;
            if hart < self.mtimecmp.len() {
                let cmp_val = self.mtimecmp[hart];
                let val_bytes = cmp_val.to_le_bytes();
                let mut buf = [0u8; 8];
                let copy_len = size.min(8usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 8 {
                    buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
                }
                return Endianness::Little
                    .read_u64(&buf)
                    .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
            }
            return Ok(0);
        }

        // 3. MTIME Register Region (0xBFF8 - 0xBFFF, 8 bytes)
        if (CLINT_MTIME..CLINT_MTIME + 8).contains(&offset) {
            let reg_offset = (offset - CLINT_MTIME) as usize;
            let val_bytes = self.mtime.to_le_bytes();
            let mut buf = [0u8; 8];
            let copy_len = size.min(8usize.saturating_sub(reg_offset));
            if copy_len > 0 && reg_offset < 8 {
                buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
            }
            return Endianness::Little
                .read_u64(&buf)
                .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
        }

        // Unmapped offsets in the 64 KB window return 0
        Ok(0)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError> {
        if size == 0 || size > 8 || offset.saturating_add(size as u64) > self.size {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset,
                size: self.size as usize,
            }));
        }

        // 1. MSIP Register Region (0x0000 - 0x3FFF, 4 bytes per hart)
        if offset < CLINT_MTIMECMP_BASE {
            let hart = (offset / 4) as usize;
            let reg_offset = (offset % 4) as usize;
            if hart < self.msip.len() {
                let mut val_bytes = self.msip[hart].to_le_bytes();
                let in_bytes = (value as u32).to_le_bytes();
                let copy_len = size.min(4usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 4 {
                    val_bytes[reg_offset..reg_offset + copy_len]
                        .copy_from_slice(&in_bytes[..copy_len]);
                    self.msip[hart] = u32::from_le_bytes(val_bytes) & 1;
                }
            }
            return Ok(());
        }

        // 2. MTIMECMP Register Region (0x4000 - 0xBFF7, 8 bytes per hart)
        if (CLINT_MTIMECMP_BASE..CLINT_MTIME).contains(&offset) {
            let relative = offset - CLINT_MTIMECMP_BASE;
            let hart = (relative / 8) as usize;
            let reg_offset = (relative % 8) as usize;
            if hart < self.mtimecmp.len() {
                let mut val_bytes = self.mtimecmp[hart].to_le_bytes();
                let in_bytes = value.to_le_bytes();
                let copy_len = size.min(8usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 8 {
                    val_bytes[reg_offset..reg_offset + copy_len]
                        .copy_from_slice(&in_bytes[..copy_len]);
                    self.mtimecmp[hart] = u64::from_le_bytes(val_bytes);
                }
            }
            return Ok(());
        }

        // 3. MTIME Register Region (0xBFF8 - 0xBFFF, 8 bytes)
        if (CLINT_MTIME..CLINT_MTIME + 8).contains(&offset) {
            let reg_offset = (offset - CLINT_MTIME) as usize;
            let mut val_bytes = self.mtime.to_le_bytes();
            let in_bytes = value.to_le_bytes();
            let copy_len = size.min(8usize.saturating_sub(reg_offset));
            if copy_len > 0 && reg_offset < 8 {
                val_bytes[reg_offset..reg_offset + copy_len].copy_from_slice(&in_bytes[..copy_len]);
                self.mtime = u64::from_le_bytes(val_bytes);
            }
            return Ok(());
        }

        // Unmapped offsets within 64 KB window ignore writes
        Ok(())
    }

    fn reset(&mut self) -> Result<(), MelError> {
        for msip in &mut self.msip {
            *msip = 0;
        }
        for mtimecmp in &mut self.mtimecmp {
            *mtimecmp = 0;
        }
        self.mtime = 0;
        Ok(())
    }

    fn tick(&mut self, cycles: u64) -> Result<(), MelError> {
        self.mtime = self.mtime.wrapping_add(cycles);
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        serde_json::to_vec(self).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let restored: Self = serde_json::from_slice(data).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })?;
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clint_initial_state() {
        let clint = Clint::new("clint0");
        assert_eq!(clint.name(), "clint0");
        assert_eq!(clint.size(), CLINT_SIZE);
        assert_eq!(clint.mtime(), 0);
        assert_eq!(clint.mtimecmp(), 0);
        assert_eq!(clint.msip(), 0);
        assert_eq!(clint.num_harts(), 1);
        // At power-on with 0 and 0, mtime >= mtimecmp is true
        assert!(clint.is_timer_interrupt_pending());
        assert!(!clint.is_software_interrupt_pending());
        assert!(clint.is_interrupting());
    }

    #[test]
    fn test_clint_timer_tick_and_comparison() {
        let mut clint = Clint::new("clint0");
        clint.set_mtimecmp(100);
        clint.set_mtime(0);

        assert!(!clint.is_timer_interrupt_pending());
        assert!(!clint.is_interrupting());

        clint.tick(50).unwrap();
        assert_eq!(clint.mtime(), 50);
        assert!(!clint.is_timer_interrupt_pending());

        clint.tick(50).unwrap();
        assert_eq!(clint.mtime(), 100);
        assert!(clint.is_timer_interrupt_pending());
        assert!(clint.is_interrupting());

        clint.tick(10).unwrap();
        assert_eq!(clint.mtime(), 110);
        assert!(clint.is_timer_interrupt_pending());
    }

    #[test]
    fn test_clint_msip_software_interrupt() {
        let mut clint = Clint::new("clint0");
        clint.set_mtimecmp(u64::MAX);
        clint.set_mtime(0);

        assert!(!clint.is_software_interrupt_pending());
        assert!(!clint.is_interrupting());

        // Write msip via MMIO (offset 0x0000, 4 bytes)
        clint.write(CLINT_MSIP_BASE, 1, 4).unwrap();
        assert_eq!(clint.msip(), 1);
        assert!(clint.is_software_interrupt_pending());
        assert!(clint.is_interrupting());
        assert_eq!(clint.read(CLINT_MSIP_BASE, 4).unwrap(), 1);

        // Clear msip
        clint.write(CLINT_MSIP_BASE, 0, 4).unwrap();
        assert_eq!(clint.msip(), 0);
        assert!(!clint.is_software_interrupt_pending());
        assert!(!clint.is_interrupting());
    }

    #[test]
    fn test_clint_mmio_mtime_mtimecmp_64bit_access() {
        let mut clint = Clint::new("clint0");

        // 64-bit write to mtimecmp (0x4000)
        clint
            .write(CLINT_MTIMECMP_BASE, 0x0123_4567_89AB_CDEF, 8)
            .unwrap();
        assert_eq!(clint.mtimecmp(), 0x0123_4567_89AB_CDEF);
        assert_eq!(
            clint.read(CLINT_MTIMECMP_BASE, 8).unwrap(),
            0x0123_4567_89AB_CDEF
        );

        // 64-bit write to mtime (0xBFF8)
        clint
            .write(CLINT_MTIME, 0xFEDC_BA98_7654_3210, 8)
            .unwrap();
        assert_eq!(clint.mtime(), 0xFEDC_BA98_7654_3210);
        assert_eq!(clint.read(CLINT_MTIME, 8).unwrap(), 0xFEDC_BA98_7654_3210);
    }

    #[test]
    fn test_clint_mmio_32bit_subword_access() {
        let mut clint = Clint::new("clint0");

        // Write lower 32 bits of mtimecmp
        clint.write(CLINT_MTIMECMP_BASE, 0x1122_3344, 4).unwrap();
        // Write upper 32 bits of mtimecmp
        clint
            .write(CLINT_MTIMECMP_BASE + 4, 0x5566_7788, 4)
            .unwrap();

        assert_eq!(clint.mtimecmp(), 0x5566_7788_1122_3344);
        assert_eq!(clint.read(CLINT_MTIMECMP_BASE, 4).unwrap(), 0x1122_3344);
        assert_eq!(clint.read(CLINT_MTIMECMP_BASE + 4, 4).unwrap(), 0x5566_7788);

        // Write lower 32 bits of mtime
        clint.write(CLINT_MTIME, 0xAABB_CCDD, 4).unwrap();
        // Write upper 32 bits of mtime
        clint.write(CLINT_MTIME + 4, 0xEEFF_0011, 4).unwrap();

        assert_eq!(clint.mtime(), 0xEEFF_0011_AABB_CCDD);
        assert_eq!(clint.read(CLINT_MTIME, 4).unwrap(), 0xAABB_CCDD);
        assert_eq!(clint.read(CLINT_MTIME + 4, 4).unwrap(), 0xEEFF_0011);
    }

    #[test]
    fn test_clint_mmio_byte_level_access() {
        let mut clint = Clint::new("clint0");
        clint.write(CLINT_MTIME, 0x0102_0304_0506_0708, 8).unwrap();

        assert_eq!(clint.read(CLINT_MTIME, 1).unwrap(), 0x08);
        assert_eq!(clint.read(CLINT_MTIME + 1, 1).unwrap(), 0x07);
        assert_eq!(clint.read(CLINT_MTIME + 7, 1).unwrap(), 0x01);

        clint.write(CLINT_MTIME + 3, 0xFF, 1).unwrap();
        assert_eq!(clint.read(CLINT_MTIME, 8).unwrap(), 0x0102_0304_FF06_0708);
    }

    #[test]
    fn test_clint_multi_hart() {
        let mut clint = Clint::with_harts("clint_dual", 2);
        assert_eq!(clint.num_harts(), 2);

        // Hart 0
        clint.set_mtimecmp_hart(0, 500).unwrap();
        clint.set_msip_hart(0, 1).unwrap();

        // Hart 1
        clint.set_mtimecmp_hart(1, 1000).unwrap();
        clint.set_msip_hart(1, 0).unwrap();

        clint.set_mtime(750);

        assert!(clint.is_timer_interrupt_pending_hart(0));
        assert!(!clint.is_timer_interrupt_pending_hart(1));

        assert!(clint.is_software_interrupt_pending_hart(0));
        assert!(!clint.is_software_interrupt_pending_hart(1));

        assert!(clint.has_pending_interrupts());

        // Access Hart 1 mtimecmp via MMIO (offset 0x4008)
        assert_eq!(clint.read(CLINT_MTIMECMP_BASE + 8, 8).unwrap(), 1000);
        // Access Hart 1 msip via MMIO (offset 0x0004)
        assert_eq!(clint.read(CLINT_MSIP_BASE + 4, 4).unwrap(), 0);
    }

    #[test]
    fn test_clint_reset_and_save_restore() {
        let mut clint = Clint::new("clint0");
        clint.set_mtime(0x1234_5678);
        clint.set_mtimecmp(0x9ABC_DEF0);
        clint.set_msip(1);

        let snapshot = clint.save_state().unwrap();

        let mut restored = Clint::new("clint0");
        restored.load_state(&snapshot).unwrap();

        assert_eq!(restored.mtime(), 0x1234_5678);
        assert_eq!(restored.mtimecmp(), 0x9ABC_DEF0);
        assert_eq!(restored.msip(), 1);

        restored.reset().unwrap();
        assert_eq!(restored.mtime(), 0);
        assert_eq!(restored.mtimecmp(), 0);
        assert_eq!(restored.msip(), 0);
    }
}
