//! RISC-V PLIC (Platform-Level Interrupt Controller) MMIO peripheral.
//!
//! Provides memory-mapped registers for routing external peripheral interrupts (UART,
//! Block storage, etc.) to CPU harts and privilege contexts following the standard
//! RISC-V PLIC specification (SiFive compatible layout).

use crate::device_trait::MmioDevice;
use mel_core::{Endianness, MelError, MemoryError};
use serde::{Deserialize, Serialize};

/// Standard PLIC base address on RISC-V platforms.
pub const PLIC_BASE: u64 = 0x0C00_0000;

/// Standard PLIC MMIO window size (64 MB).
pub const PLIC_SIZE: u64 = 0x0400_0000;

/// Base offset for Interrupt Source Priorities (4 bytes per source, up to 1024 sources).
pub const PLIC_PRIORITY_BASE: u64 = 0x0000_0000;

/// Base offset for Interrupt Pending bits (32 bits / 4 bytes per 32 sources).
pub const PLIC_PENDING_BASE: u64 = 0x0000_1000;

/// Base offset for Interrupt Enable bits per Context.
pub const PLIC_ENABLE_BASE: u64 = 0x0000_2000;

/// Stride between context enable bit regions (128 bytes = 32 words = 1024 sources).
pub const PLIC_ENABLE_STRIDE: u64 = 0x80;

/// Base offset for Context Priority Threshold & Claim/Complete registers.
pub const PLIC_CONTEXT_BASE: u64 = 0x0020_0000;

/// Stride between context control blocks (4 KB).
pub const PLIC_CONTEXT_STRIDE: u64 = 0x1000;

/// Maximum number of interrupt sources supported by PLIC (Source 0 is reserved).
pub const PLIC_MAX_SOURCES: usize = 1024;

/// Default number of interrupt sources for standard SoC configuration.
pub const PLIC_DEFAULT_SOURCES: usize = 64;

/// Default number of contexts (e.g., Hart 0 M-mode = Context 0, Hart 0 S-mode = Context 1).
pub const PLIC_DEFAULT_CONTEXTS: usize = 4;

/// Standard IRQ line assignment for VirtIO / Block storage device.
pub const BLOCK0_IRQ: u32 = 1;

/// Standard IRQ line assignment for UART0 serial controller (QEMU virt standard).
pub const UART0_IRQ: u32 = 10;

/// RISC-V Platform-Level Interrupt Controller (PLIC).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plic {
    name: String,
    size: u64,
    num_sources: usize,
    num_contexts: usize,
    /// Priority per source ID (source 0 is hardwired to 0).
    priorities: Vec<u32>,
    /// Pending bit per source ID.
    pending: Vec<bool>,
    /// Active physical line state (level-triggered).
    source_levels: Vec<bool>,
    /// Claimed status per source ID.
    claimed: Vec<bool>,
    /// Enable bitmask per context: `enables[context][word]`.
    enables: Vec<Vec<u32>>,
    /// Priority threshold per context.
    thresholds: Vec<u32>,
}

impl Plic {
    /// Create a new PLIC peripheral with default sources (64) and contexts (4).
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_config(name, PLIC_DEFAULT_SOURCES, PLIC_DEFAULT_CONTEXTS)
    }

    /// Create a new PLIC peripheral with custom number of sources and contexts.
    pub fn with_config(name: impl Into<String>, num_sources: usize, num_contexts: usize) -> Self {
        let sources = num_sources.clamp(2, PLIC_MAX_SOURCES);
        let contexts = num_contexts.max(1);
        let words_per_context = sources.div_ceil(32);

        Self {
            name: name.into(),
            size: PLIC_SIZE,
            num_sources: sources,
            num_contexts: contexts,
            priorities: vec![0; sources],
            pending: vec![false; sources],
            source_levels: vec![false; sources],
            claimed: vec![false; sources],
            enables: vec![vec![0; words_per_context]; contexts],
            thresholds: vec![0; contexts],
        }
    }

    /// Total number of configured interrupt sources.
    pub fn num_sources(&self) -> usize {
        self.num_sources
    }

    /// Total number of configured execution contexts.
    pub fn num_contexts(&self) -> usize {
        self.num_contexts
    }

    /// Set priority for an interrupt source (Source 0 is always 0).
    pub fn set_priority(&mut self, source: u32, priority: u32) {
        let s = source as usize;
        if s > 0 && s < self.num_sources {
            self.priorities[s] = priority;
        }
    }

    /// Read priority for an interrupt source.
    pub fn priority(&self, source: u32) -> u32 {
        let s = source as usize;
        if s > 0 && s < self.num_sources {
            self.priorities[s]
        } else {
            0
        }
    }

    /// Assert or deassert an external interrupt line (e.g. from UART or Block device).
    pub fn set_irq_source(&mut self, source: u32, level: bool) {
        let s = source as usize;
        if s > 0 && s < self.num_sources {
            self.source_levels[s] = level;
            if level && !self.claimed[s] {
                self.pending[s] = true;
            }
        }
    }

    /// Pulse an edge-triggered interrupt.
    pub fn trigger_irq(&mut self, source: u32) {
        let s = source as usize;
        if s > 0 && s < self.num_sources && !self.claimed[s] {
            self.pending[s] = true;
        }
    }

    /// Check if an interrupt source is pending.
    pub fn is_pending(&self, source: u32) -> bool {
        let s = source as usize;
        if s > 0 && s < self.num_sources {
            self.pending[s]
        } else {
            false
        }
    }

    /// Set interrupt enable bit for a given context and source.
    pub fn set_enabled(&mut self, context: usize, source: u32, enabled: bool) {
        let s = source as usize;
        if context < self.num_contexts && s > 0 && s < self.num_sources {
            let word = s / 32;
            let bit = s % 32;
            if word < self.enables[context].len() {
                if enabled {
                    self.enables[context][word] |= 1 << bit;
                } else {
                    self.enables[context][word] &= !(1 << bit);
                }
            }
        }
    }

    /// Check if an interrupt source is enabled for a given context.
    pub fn is_enabled(&self, context: usize, source: u32) -> bool {
        let s = source as usize;
        if context < self.num_contexts && s > 0 && s < self.num_sources {
            let word = s / 32;
            let bit = s % 32;
            if let Some(w) = self.enables[context].get(word) {
                return (w & (1 << bit)) != 0;
            }
        }
        false
    }

    /// Set priority threshold for a context.
    pub fn set_threshold(&mut self, context: usize, threshold: u32) {
        if context < self.num_contexts {
            self.thresholds[context] = threshold;
        }
    }

    /// Read priority threshold for a context.
    pub fn threshold(&self, context: usize) -> u32 {
        self.thresholds.get(context).copied().unwrap_or(0)
    }

    /// Find the highest priority pending, enabled interrupt exceeding the context threshold.
    /// Returns `(source_id, priority)` or `None` if no interrupt qualifies.
    pub fn get_best_interrupt(&self, context: usize) -> Option<(u32, u32)> {
        if context >= self.num_contexts {
            return None;
        }

        let threshold = self.thresholds[context];
        let mut best_source = 0u32;
        let mut best_priority = threshold;

        for s in 1..self.num_sources {
            if self.pending[s] && self.is_enabled(context, s as u32) {
                let prio = self.priorities[s];
                if prio > best_priority {
                    best_priority = prio;
                    best_source = s as u32;
                }
            }
        }

        if best_source > 0 {
            Some((best_source, best_priority))
        } else {
            None
        }
    }

    /// Check whether an external interrupt (MEIP/SEIP) is asserted for a context.
    pub fn is_external_interrupt_pending(&self, context: usize) -> bool {
        self.get_best_interrupt(context).is_some()
    }

    /// Returns whether any context has a pending external interrupt.
    pub fn is_interrupting(&self) -> bool {
        for c in 0..self.num_contexts {
            if self.is_external_interrupt_pending(c) {
                return true;
            }
        }
        false
    }

    /// Claim the highest-priority pending interrupt for `context`.
    /// Returns source ID (0 if none available).
    pub fn claim(&mut self, context: usize) -> u32 {
        if let Some((source, _)) = self.get_best_interrupt(context) {
            let s = source as usize;
            self.pending[s] = false;
            self.claimed[s] = true;
            source
        } else {
            0
        }
    }

    /// Complete an in-flight interrupt for `context`.
    pub fn complete(&mut self, _context: usize, source: u32) {
        let s = source as usize;
        if s > 0 && s < self.num_sources {
            self.claimed[s] = false;
            // If the physical line is still asserted high, re-pend it
            if self.source_levels[s] {
                self.pending[s] = true;
            }
        }
    }
}

impl MmioDevice for Plic {
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

        let mut buf = [0u8; 8];

        // 1. Source Priorities (0x0000_0000 - 0x0000_0FFC)
        if offset < PLIC_PENDING_BASE {
            let source = (offset / 4) as usize;
            let reg_offset = (offset % 4) as usize;
            let val = if source > 0 && source < self.num_sources {
                self.priorities[source]
            } else {
                0
            };
            let val_bytes = val.to_le_bytes();
            let copy_len = size.min(4usize.saturating_sub(reg_offset));
            if copy_len > 0 && reg_offset < 4 {
                buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
            }
            return Endianness::Little
                .read_u64(&buf)
                .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
        }

        // 2. Pending Bits (0x0000_1000 - 0x0000_1FFF)
        if (PLIC_PENDING_BASE..PLIC_ENABLE_BASE).contains(&offset) {
            let rel = offset - PLIC_PENDING_BASE;
            let word_idx = (rel / 4) as usize;
            let reg_offset = (rel % 4) as usize;

            let mut word_val = 0u32;
            let start_source = word_idx * 32;
            for bit in 0..32 {
                let s = start_source + bit;
                if s > 0 && s < self.num_sources && self.pending[s] {
                    word_val |= 1 << bit;
                }
            }

            let val_bytes = word_val.to_le_bytes();
            let copy_len = size.min(4usize.saturating_sub(reg_offset));
            if copy_len > 0 && reg_offset < 4 {
                buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
            }
            return Endianness::Little
                .read_u64(&buf)
                .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
        }

        // 3. Enables (0x0000_2000 - 0x001F_FFFF)
        if (PLIC_ENABLE_BASE..PLIC_CONTEXT_BASE).contains(&offset) {
            let rel = offset - PLIC_ENABLE_BASE;
            let context = (rel / PLIC_ENABLE_STRIDE) as usize;
            let word_offset = rel % PLIC_ENABLE_STRIDE;
            let word_idx = (word_offset / 4) as usize;
            let reg_offset = (word_offset % 4) as usize;

            let word_val = if context < self.num_contexts {
                self.enables[context].get(word_idx).copied().unwrap_or(0)
            } else {
                0
            };

            let val_bytes = word_val.to_le_bytes();
            let copy_len = size.min(4usize.saturating_sub(reg_offset));
            if copy_len > 0 && reg_offset < 4 {
                buf[..copy_len].copy_from_slice(&val_bytes[reg_offset..reg_offset + copy_len]);
            }
            return Endianness::Little
                .read_u64(&buf)
                .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
        }

        // 4. Context Threshold & Claim/Complete (0x0020_0000+)
        if offset >= PLIC_CONTEXT_BASE {
            let rel = offset - PLIC_CONTEXT_BASE;
            let context = (rel / PLIC_CONTEXT_STRIDE) as usize;
            let reg_offset = rel % PLIC_CONTEXT_STRIDE;

            if context < self.num_contexts {
                if reg_offset < 4 {
                    // Priority Threshold
                    let val = self.thresholds[context];
                    let val_bytes = val.to_le_bytes();
                    let byte_offset = reg_offset as usize;
                    let copy_len = size.min(4usize.saturating_sub(byte_offset));
                    if copy_len > 0 && byte_offset < 4 {
                        buf[..copy_len]
                            .copy_from_slice(&val_bytes[byte_offset..byte_offset + copy_len]);
                    }
                    return Endianness::Little
                        .read_u64(&buf)
                        .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
                } else if (4..8).contains(&reg_offset) {
                    // Claim Register (Reading triggers claim)
                    let byte_offset = (reg_offset - 4) as usize;
                    let claim_id = self.claim(context);
                    let val_bytes = claim_id.to_le_bytes();
                    let copy_len = size.min(4usize.saturating_sub(byte_offset));
                    if copy_len > 0 && byte_offset < 4 {
                        buf[..copy_len]
                            .copy_from_slice(&val_bytes[byte_offset..byte_offset + copy_len]);
                    }
                    return Endianness::Little
                        .read_u64(&buf)
                        .ok_or(MelError::Memory(MemoryError::OutOfBounds { addr: offset, size }));
                }
            }
            return Ok(0);
        }

        Ok(0)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError> {
        if size == 0 || size > 8 || offset.saturating_add(size as u64) > self.size {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset,
                size: self.size as usize,
            }));
        }

        // 1. Source Priorities (0x0000_0000 - 0x0000_0FFC)
        if offset < PLIC_PENDING_BASE {
            let source = (offset / 4) as usize;
            let reg_offset = (offset % 4) as usize;
            if source > 0 && source < self.num_sources {
                let mut val_bytes = self.priorities[source].to_le_bytes();
                let in_bytes = (value as u32).to_le_bytes();
                let copy_len = size.min(4usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 4 {
                    val_bytes[reg_offset..reg_offset + copy_len]
                        .copy_from_slice(&in_bytes[..copy_len]);
                    self.priorities[source] = u32::from_le_bytes(val_bytes);
                }
            }
            return Ok(());
        }

        // 2. Pending Bits are Read-Only in standard PLIC (writes are ignored)
        if (PLIC_PENDING_BASE..PLIC_ENABLE_BASE).contains(&offset) {
            return Ok(());
        }

        // 3. Enables (0x0000_2000 - 0x001F_FFFF)
        if (PLIC_ENABLE_BASE..PLIC_CONTEXT_BASE).contains(&offset) {
            let rel = offset - PLIC_ENABLE_BASE;
            let context = (rel / PLIC_ENABLE_STRIDE) as usize;
            let word_offset = rel % PLIC_ENABLE_STRIDE;
            let word_idx = (word_offset / 4) as usize;
            let reg_offset = (word_offset % 4) as usize;

            if context < self.num_contexts && word_idx < self.enables[context].len() {
                let mut val_bytes = self.enables[context][word_idx].to_le_bytes();
                let in_bytes = (value as u32).to_le_bytes();
                let copy_len = size.min(4usize.saturating_sub(reg_offset));
                if copy_len > 0 && reg_offset < 4 {
                    val_bytes[reg_offset..reg_offset + copy_len]
                        .copy_from_slice(&in_bytes[..copy_len]);
                    let mut new_word = u32::from_le_bytes(val_bytes);
                    if word_idx == 0 {
                        new_word &= !1; // Source 0 enable is always 0
                    }
                    self.enables[context][word_idx] = new_word;
                }
            }
            return Ok(());
        }

        // 4. Context Threshold & Claim/Complete (0x0020_0000+)
        if offset >= PLIC_CONTEXT_BASE {
            let rel = offset - PLIC_CONTEXT_BASE;
            let context = (rel / PLIC_CONTEXT_STRIDE) as usize;
            let reg_offset = rel % PLIC_CONTEXT_STRIDE;

            if context < self.num_contexts {
                if reg_offset < 4 {
                    // Priority Threshold
                    let byte_offset = reg_offset as usize;
                    let mut val_bytes = self.thresholds[context].to_le_bytes();
                    let in_bytes = (value as u32).to_le_bytes();
                    let copy_len = size.min(4usize.saturating_sub(byte_offset));
                    if copy_len > 0 && byte_offset < 4 {
                        val_bytes[byte_offset..byte_offset + copy_len]
                            .copy_from_slice(&in_bytes[..copy_len]);
                        self.thresholds[context] = u32::from_le_bytes(val_bytes);
                    }
                } else if (4..8).contains(&reg_offset) {
                    // Complete Register (Writing source ID completes the interrupt)
                    let complete_source = value as u32;
                    self.complete(context, complete_source);
                }
            }
            return Ok(());
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<(), MelError> {
        self.priorities.fill(0);
        self.pending.fill(false);
        self.source_levels.fill(false);
        self.claimed.fill(false);
        for ctx_enables in &mut self.enables {
            ctx_enables.fill(0);
        }
        self.thresholds.fill(0);
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
    fn test_plic_initialization() {
        let plic = Plic::new("plic0");
        assert_eq!(plic.name(), "plic0");
        assert_eq!(plic.size(), PLIC_SIZE);
        assert_eq!(plic.num_sources(), PLIC_DEFAULT_SOURCES);
        assert_eq!(plic.num_contexts(), PLIC_DEFAULT_CONTEXTS);
        assert!(!plic.is_interrupting());
        assert_eq!(plic.priority(0), 0);
        assert_eq!(plic.priority(10), 0);
        assert_eq!(plic.threshold(0), 0);
    }

    #[test]
    fn test_plic_priority_configuration() {
        let mut plic = Plic::new("plic0");

        // Source 0 priority is hardwired to 0
        plic.set_priority(0, 7);
        assert_eq!(plic.priority(0), 0);

        // Configure UART0 priority (source 10)
        plic.set_priority(UART0_IRQ, 5);
        assert_eq!(plic.priority(UART0_IRQ), 5);

        // Configure Block0 priority (source 1)
        plic.set_priority(BLOCK0_IRQ, 3);
        assert_eq!(plic.priority(BLOCK0_IRQ), 3);
    }

    #[test]
    fn test_plic_enable_and_threshold() {
        let mut plic = Plic::new("plic0");

        // Context 0 (Hart 0 M-mode)
        plic.set_enabled(0, UART0_IRQ, true);
        assert!(plic.is_enabled(0, UART0_IRQ));
        assert!(!plic.is_enabled(0, BLOCK0_IRQ));
        assert!(!plic.is_enabled(1, UART0_IRQ));

        // Source 0 cannot be enabled
        plic.set_enabled(0, 0, true);
        assert!(!plic.is_enabled(0, 0));

        // Priority threshold
        plic.set_threshold(0, 4);
        assert_eq!(plic.threshold(0), 4);
    }

    #[test]
    fn test_plic_interrupt_flow_and_threshold_filtering() {
        let mut plic = Plic::new("plic0");

        // Priority = 3, Threshold = 4 -> Should NOT trigger
        plic.set_priority(UART0_IRQ, 3);
        plic.set_enabled(0, UART0_IRQ, true);
        plic.set_threshold(0, 4);

        plic.set_irq_source(UART0_IRQ, true);
        assert!(plic.is_pending(UART0_IRQ));
        assert!(!plic.is_external_interrupt_pending(0));
        assert_eq!(plic.claim(0), 0);

        // Raise priority = 5 -> Exceeds threshold 4 -> Triggers
        plic.set_priority(UART0_IRQ, 5);
        assert!(plic.is_external_interrupt_pending(0));
        assert!(plic.is_interrupting());

        // Claim interrupt
        let claimed_source = plic.claim(0);
        assert_eq!(claimed_source, UART0_IRQ);
        assert!(!plic.is_pending(UART0_IRQ));
        assert!(!plic.is_external_interrupt_pending(0));

        // Complete interrupt while UART line is still asserted high (level-triggered)
        plic.complete(0, UART0_IRQ);
        // Should re-pend since source level is still true
        assert!(plic.is_pending(UART0_IRQ));
        assert!(plic.is_external_interrupt_pending(0));

        // Deassert UART line and complete again
        plic.set_irq_source(UART0_IRQ, false);
        let _ = plic.claim(0);
        plic.complete(0, UART0_IRQ);
        assert!(!plic.is_pending(UART0_IRQ));
        assert!(!plic.is_external_interrupt_pending(0));
    }

    #[test]
    fn test_plic_priority_arbitration() {
        let mut plic = Plic::new("plic0");

        // Enable UART (source 10, prio 2) and Block (source 1, prio 6)
        plic.set_priority(UART0_IRQ, 2);
        plic.set_priority(BLOCK0_IRQ, 6);
        plic.set_enabled(0, UART0_IRQ, true);
        plic.set_enabled(0, BLOCK0_IRQ, true);

        // Assert both interrupt lines
        plic.set_irq_source(UART0_IRQ, true);
        plic.set_irq_source(BLOCK0_IRQ, true);

        // Higher priority (Block, 6 > 2) claimed first
        assert_eq!(plic.claim(0), BLOCK0_IRQ);

        // Next claim gets UART (source 10)
        assert_eq!(plic.claim(0), UART0_IRQ);

        // No more pending interrupts
        assert_eq!(plic.claim(0), 0);
    }

    #[test]
    fn test_plic_mmio_read_write_registers() {
        let mut plic = Plic::new("plic0");

        // Write priority for UART0 (source 10) at offset 10 * 4 = 0x28
        plic.write(0x28, 7, 4).unwrap();
        assert_eq!(plic.priority(UART0_IRQ), 7);
        assert_eq!(plic.read(0x28, 4).unwrap(), 7);

        // Enable source 10 in Context 0 (offset 0x2000, bit 10 = 0x400)
        plic.write(PLIC_ENABLE_BASE, 1 << UART0_IRQ, 4).unwrap();
        assert!(plic.is_enabled(0, UART0_IRQ));
        assert_eq!(plic.read(PLIC_ENABLE_BASE, 4).unwrap(), 1 << UART0_IRQ);

        // Trigger IRQ 10
        plic.set_irq_source(UART0_IRQ, true);

        // Read pending bits at 0x1000
        let pending_word = plic.read(PLIC_PENDING_BASE, 4).unwrap();
        assert_eq!(pending_word & (1 << UART0_IRQ), 1 << UART0_IRQ);

        // Set Context 0 Threshold at 0x0020_0000
        plic.write(PLIC_CONTEXT_BASE, 3, 4).unwrap();
        assert_eq!(plic.threshold(0), 3);
        assert_eq!(plic.read(PLIC_CONTEXT_BASE, 4).unwrap(), 3);

        // Read Context 0 Claim register at 0x0020_0004
        let claim_val = plic.read(PLIC_CONTEXT_BASE + 4, 4).unwrap();
        assert_eq!(claim_val, UART0_IRQ as u64);

        // Write Complete to 0x0020_0004
        plic.set_irq_source(UART0_IRQ, false);
        plic.write(PLIC_CONTEXT_BASE + 4, UART0_IRQ as u64, 4)
            .unwrap();
        assert!(!plic.is_external_interrupt_pending(0));
    }

    #[test]
    fn test_plic_subword_mmio_access() {
        let mut plic = Plic::new("plic0");

        // Byte writes to priority register (source 10 at 0x28)
        plic.write(0x28, 0x12, 1).unwrap();
        plic.write(0x29, 0x34, 1).unwrap();
        assert_eq!(plic.priority(UART0_IRQ), 0x3412);
        assert_eq!(plic.read(0x28, 1).unwrap(), 0x12);
        assert_eq!(plic.read(0x29, 1).unwrap(), 0x34);
    }

    #[test]
    fn test_plic_reset_and_snapshot() {
        let mut plic = Plic::new("plic0");
        plic.set_priority(UART0_IRQ, 5);
        plic.set_enabled(0, UART0_IRQ, true);
        plic.set_threshold(0, 2);
        plic.set_irq_source(UART0_IRQ, true);

        let snap = plic.save_state().unwrap();

        let mut restored = Plic::new("plic0");
        restored.load_state(&snap).unwrap();

        assert_eq!(restored.priority(UART0_IRQ), 5);
        assert!(restored.is_enabled(0, UART0_IRQ));
        assert_eq!(restored.threshold(0), 2);
        assert!(restored.is_pending(UART0_IRQ));
        assert!(restored.is_external_interrupt_pending(0));

        restored.reset().unwrap();
        assert_eq!(restored.priority(UART0_IRQ), 0);
        assert!(!restored.is_enabled(0, UART0_IRQ));
        assert!(!restored.is_interrupting());
    }
}
