//! Breakpoint and watchpoint management for the MEL debugger.

use mel_core::MelError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Kind of memory access monitored by a watchpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchpointKind {
    /// Triggers on memory read operations.
    Read,
    /// Triggers on memory write operations.
    Write,
    /// Triggers on both memory read and write operations.
    ReadWrite,
}

impl WatchpointKind {
    /// Returns true if this watchpoint kind matches the specified memory access type.
    pub fn matches(self, is_write: bool) -> bool {
        match self {
            WatchpointKind::Read => !is_write,
            WatchpointKind::Write => is_write,
            WatchpointKind::ReadWrite => true,
        }
    }
}

/// Trigger condition for a breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakpointCondition {
    /// Always breaks whenever reached.
    Always,
    /// Breaks when the hit count reaches or exceeds a specific threshold.
    HitCount(u64),
    /// Breaks every N hits (e.g. hit_count % N == 0).
    EveryNHits(u64),
}

impl BreakpointCondition {
    /// Evaluates whether execution should pause given the updated hit count.
    pub fn should_break(&self, hit_count: u64) -> bool {
        match self {
            BreakpointCondition::Always => true,
            BreakpointCondition::HitCount(threshold) => hit_count >= *threshold,
            BreakpointCondition::EveryNHits(n) => *n > 0 && hit_count.is_multiple_of(*n),
        }
    }
}

/// Software execution breakpoint set on a program counter (PC) address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Unique breakpoint identifier.
    pub id: u32,
    /// Target PC memory address.
    pub address: u64,
    /// Whether this breakpoint is actively evaluated.
    pub enabled: bool,
    /// Total number of times this address was evaluated.
    pub hit_count: u64,
    /// Whether this breakpoint is automatically removed after firing once.
    pub temporary: bool,
    /// Condition governing whether to halt on hit.
    pub condition: BreakpointCondition,
    /// Optional user label or symbol description.
    pub description: Option<String>,
}

/// Memory watchpoint monitoring access to an address range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchpoint {
    /// Unique watchpoint identifier.
    pub id: u32,
    /// Base memory address being monitored.
    pub address: u64,
    /// Length of the monitored memory region in bytes.
    pub size: usize,
    /// Type of access that triggers this watchpoint.
    pub kind: WatchpointKind,
    /// Whether this watchpoint is actively evaluated.
    pub enabled: bool,
    /// Total number of times this watchpoint was triggered.
    pub hit_count: u64,
    /// Whether this watchpoint is automatically removed after firing once.
    pub temporary: bool,
    /// Optional user label or description.
    pub description: Option<String>,
}

/// Reason for a debugger execution pause or trap event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebugStopReason {
    /// Single instruction step completed.
    Step,
    /// Execution breakpoint hit on program counter.
    Breakpoint {
        id: u32,
        address: u64,
        hit_count: u64,
    },
    /// Memory watchpoint hit on read/write access.
    Watchpoint {
        id: u32,
        address: u64,
        size: usize,
        kind: WatchpointKind,
        accessed_address: u64,
        access_size: usize,
        hit_count: u64,
    },
    /// Target reached specified maximum cycle or step budget.
    CycleLimitReached { cycles: u64 },
    /// Target CPU halted.
    Halted { reason: String },
    /// Target CPU encountered an architectural trap / exception.
    Trap { cause: u64, epc: u64 },
}

/// Information returned when a breakpoint or watchpoint fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakpointHit {
    /// Execution breakpoint fired.
    Breakpoint(Breakpoint),
    /// Memory watchpoint fired.
    Watchpoint {
        watchpoint: Watchpoint,
        accessed_address: u64,
        access_size: usize,
        kind: WatchpointKind,
    },
}

/// Central breakpoint and watchpoint manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakpointManager {
    next_id: u32,
    breakpoints: HashMap<u32, Breakpoint>,
    watchpoints: HashMap<u32, Watchpoint>,
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BreakpointManager {
    /// Create a new, empty breakpoint manager.
    pub fn new() -> Self {
        Self {
            next_id: 1,
            breakpoints: HashMap::new(),
            watchpoints: HashMap::new(),
        }
    }

    /// Add a permanent software execution breakpoint on a PC address.
    pub fn add_breakpoint(&mut self, address: u64) -> u32 {
        self.add_breakpoint_with_desc(address, false, BreakpointCondition::Always, None)
    }

    /// Add a one-shot temporary software breakpoint on a PC address.
    pub fn add_temporary_breakpoint(&mut self, address: u64) -> u32 {
        self.add_breakpoint_with_desc(address, true, BreakpointCondition::Always, None)
    }

    /// Add a conditional software breakpoint.
    pub fn add_conditional_breakpoint(
        &mut self,
        address: u64,
        condition: BreakpointCondition,
    ) -> u32 {
        self.add_breakpoint_with_desc(address, false, condition, None)
    }

    /// Add a configurable software breakpoint.
    pub fn add_breakpoint_with_desc(
        &mut self,
        address: u64,
        temporary: bool,
        condition: BreakpointCondition,
        description: Option<String>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let bp = Breakpoint {
            id,
            address,
            enabled: true,
            hit_count: 0,
            temporary,
            condition,
            description,
        };

        self.breakpoints.insert(id, bp);
        id
    }

    /// Add a permanent memory watchpoint.
    pub fn add_watchpoint(&mut self, address: u64, size: usize, kind: WatchpointKind) -> u32 {
        self.add_watchpoint_with_desc(address, size, kind, false, None)
    }

    /// Add a one-shot temporary memory watchpoint.
    pub fn add_temporary_watchpoint(
        &mut self,
        address: u64,
        size: usize,
        kind: WatchpointKind,
    ) -> u32 {
        self.add_watchpoint_with_desc(address, size, kind, true, None)
    }

    /// Add a configurable memory watchpoint.
    pub fn add_watchpoint_with_desc(
        &mut self,
        address: u64,
        size: usize,
        kind: WatchpointKind,
        temporary: bool,
        description: Option<String>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let wp = Watchpoint {
            id,
            address,
            size: if size == 0 { 1 } else { size },
            kind,
            enabled: true,
            hit_count: 0,
            temporary,
            description,
        };

        self.watchpoints.insert(id, wp);
        id
    }

    /// Remove a breakpoint by its unique ID.
    pub fn remove_breakpoint(&mut self, id: u32) -> Result<Breakpoint, MelError> {
        self.breakpoints
            .remove(&id)
            .ok_or_else(|| MelError::Debugger(format!("Breakpoint #{id} does not exist")))
    }

    /// Remove all breakpoints at the specified address, returning count removed.
    pub fn remove_breakpoint_at(&mut self, address: u64) -> usize {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|_, bp| bp.address != address);
        before - self.breakpoints.len()
    }

    /// Remove a watchpoint by its unique ID.
    pub fn remove_watchpoint(&mut self, id: u32) -> Result<Watchpoint, MelError> {
        self.watchpoints
            .remove(&id)
            .ok_or_else(|| MelError::Debugger(format!("Watchpoint #{id} does not exist")))
    }

    /// Enable or disable a breakpoint by ID.
    pub fn enable_breakpoint(&mut self, id: u32, enabled: bool) -> Result<(), MelError> {
        let bp = self
            .breakpoints
            .get_mut(&id)
            .ok_or_else(|| MelError::Debugger(format!("Breakpoint #{id} does not exist")))?;
        bp.enabled = enabled;
        Ok(())
    }

    /// Enable or disable a watchpoint by ID.
    pub fn enable_watchpoint(&mut self, id: u32, enabled: bool) -> Result<(), MelError> {
        let wp = self
            .watchpoints
            .get_mut(&id)
            .ok_or_else(|| MelError::Debugger(format!("Watchpoint #{id} does not exist")))?;
        wp.enabled = enabled;
        Ok(())
    }

    /// Lookup a breakpoint by ID.
    pub fn get_breakpoint(&self, id: u32) -> Option<&Breakpoint> {
        self.breakpoints.get(&id)
    }

    /// Lookup a mutable breakpoint by ID.
    pub fn get_breakpoint_mut(&mut self, id: u32) -> Option<&mut Breakpoint> {
        self.breakpoints.get_mut(&id)
    }

    /// Lookup an enabled breakpoint at a specific address.
    pub fn get_breakpoint_at(&self, address: u64) -> Option<&Breakpoint> {
        self.breakpoints
            .values()
            .find(|bp| bp.enabled && bp.address == address)
    }

    /// Lookup a watchpoint by ID.
    pub fn get_watchpoint(&self, id: u32) -> Option<&Watchpoint> {
        self.watchpoints.get(&id)
    }

    /// Lookup a mutable watchpoint by ID.
    pub fn get_watchpoint_mut(&mut self, id: u32) -> Option<&mut Watchpoint> {
        self.watchpoints.get_mut(&id)
    }

    /// Returns all registered breakpoints sorted by ID.
    pub fn breakpoints(&self) -> Vec<&Breakpoint> {
        let mut list: Vec<_> = self.breakpoints.values().collect();
        list.sort_by_key(|bp| bp.id);
        list
    }

    /// Returns all registered watchpoints sorted by ID.
    pub fn watchpoints(&self) -> Vec<&Watchpoint> {
        let mut list: Vec<_> = self.watchpoints.values().collect();
        list.sort_by_key(|wp| wp.id);
        list
    }

    /// Total count of registered breakpoints.
    pub fn breakpoint_count(&self) -> usize {
        self.breakpoints.len()
    }

    /// Total count of registered watchpoints.
    pub fn watchpoint_count(&self) -> usize {
        self.watchpoints.len()
    }

    /// Check if an enabled breakpoint exists at a given address.
    pub fn has_breakpoint_at(&self, address: u64) -> bool {
        self.breakpoints
            .values()
            .any(|bp| bp.enabled && bp.address == address)
    }

    /// Clear all breakpoints.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    /// Clear all watchpoints.
    pub fn clear_watchpoints(&mut self) {
        self.watchpoints.clear();
    }

    /// Clear all breakpoints and watchpoints.
    pub fn clear_all(&mut self) {
        self.breakpoints.clear();
        self.watchpoints.clear();
    }

    /// Check if the current program counter (PC) triggers an enabled breakpoint.
    ///
    /// If hit, increments the breakpoint's hit count. If temporary, removes the breakpoint.
    pub fn check_pc(&mut self, pc: u64) -> Option<BreakpointHit> {
        let matching_id = self
            .breakpoints
            .values()
            .filter(|bp| bp.enabled && bp.address == pc)
            .map(|bp| bp.id)
            .next();

        if let Some(id) = matching_id {
            let bp = self.breakpoints.get_mut(&id)?;
            bp.hit_count = bp.hit_count.saturating_add(1);

            if bp.condition.should_break(bp.hit_count) {
                let hit_bp = bp.clone();
                if bp.temporary {
                    self.breakpoints.remove(&id);
                }
                return Some(BreakpointHit::Breakpoint(hit_bp));
            }
        }

        None
    }

    /// Check if a memory access triggers any enabled watchpoint.
    ///
    /// - `access_addr`: Starting address of the memory access.
    /// - `access_size`: Size of access in bytes.
    /// - `is_write`: True if write, false if read.
    pub fn check_watchpoint(
        &mut self,
        access_addr: u64,
        access_size: usize,
        is_write: bool,
    ) -> Option<BreakpointHit> {
        let effective_access_size = if access_size == 0 { 1 } else { access_size };
        let access_end = access_addr.saturating_add(effective_access_size as u64);

        let matching_id = self
            .watchpoints
            .values()
            .filter(|wp| {
                if !wp.enabled || !wp.kind.matches(is_write) {
                    return false;
                }
                let wp_end = wp.address.saturating_add(wp.size as u64);
                // Overlap test: [access_addr, access_end) intersects [wp.address, wp_end)
                access_addr < wp_end && wp.address < access_end
            })
            .map(|wp| wp.id)
            .next();

        if let Some(id) = matching_id {
            let wp = self.watchpoints.get_mut(&id)?;
            wp.hit_count = wp.hit_count.saturating_add(1);
            let hit_wp = wp.clone();
            let matched_kind = if is_write {
                WatchpointKind::Write
            } else {
                WatchpointKind::Read
            };

            if wp.temporary {
                self.watchpoints.remove(&id);
            }

            return Some(BreakpointHit::Watchpoint {
                watchpoint: hit_wp,
                accessed_address: access_addr,
                access_size: effective_access_size,
                kind: matched_kind,
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_crud() {
        let mut mgr = BreakpointManager::new();
        assert_eq!(mgr.breakpoint_count(), 0);

        let bp1 = mgr.add_breakpoint(0x8000_0000);
        let bp2 = mgr.add_breakpoint(0x8000_0004);
        assert_eq!(mgr.breakpoint_count(), 2);
        assert!(mgr.has_breakpoint_at(0x8000_0000));
        assert!(mgr.has_breakpoint_at(0x8000_0004));
        assert!(!mgr.has_breakpoint_at(0x8000_0008));

        // Disable bp1
        mgr.enable_breakpoint(bp1, false).unwrap();
        assert!(!mgr.has_breakpoint_at(0x8000_0000));
        assert!(!mgr.get_breakpoint(bp1).unwrap().enabled);

        // Re-enable bp1
        mgr.enable_breakpoint(bp1, true).unwrap();
        assert!(mgr.has_breakpoint_at(0x8000_0000));

        // Remove bp2
        let removed = mgr.remove_breakpoint(bp2).unwrap();
        assert_eq!(removed.id, bp2);
        assert_eq!(removed.address, 0x8000_0004);
        assert_eq!(mgr.breakpoint_count(), 1);

        // Remove nonexistent
        assert!(mgr.remove_breakpoint(999).is_err());
    }

    #[test]
    fn test_breakpoint_hit_and_temporary() {
        let mut mgr = BreakpointManager::new();
        let bp_temp = mgr.add_temporary_breakpoint(0x8000_0010);

        // First hit should trigger and remove temporary breakpoint
        let hit1 = mgr.check_pc(0x8000_0010);
        assert!(hit1.is_some());
        if let Some(BreakpointHit::Breakpoint(bp)) = hit1 {
            assert_eq!(bp.id, bp_temp);
            assert_eq!(bp.address, 0x8000_0010);
            assert_eq!(bp.hit_count, 1);
        } else {
            panic!("Expected breakpoint hit");
        }

        // Second check on same address should no longer find it
        let hit2 = mgr.check_pc(0x8000_0010);
        assert!(hit2.is_none());
        assert_eq!(mgr.breakpoint_count(), 0);
    }

    #[test]
    fn test_conditional_breakpoints() {
        let mut mgr = BreakpointManager::new();

        // Break on 3rd hit
        let bp_cond = mgr.add_conditional_breakpoint(0x1000, BreakpointCondition::HitCount(3));

        // Hit 1: count becomes 1, should not break
        assert!(mgr.check_pc(0x1000).is_none());
        // Hit 2: count becomes 2, should not break
        assert!(mgr.check_pc(0x1000).is_none());
        // Hit 3: count becomes 3, breaks!
        let hit3 = mgr.check_pc(0x1000);
        assert!(hit3.is_some());
        if let Some(BreakpointHit::Breakpoint(bp)) = hit3 {
            assert_eq!(bp.id, bp_cond);
            assert_eq!(bp.hit_count, 3);
        }

        // Test every N hits
        let mut mgr2 = BreakpointManager::new();
        let bp_every2 = mgr2.add_conditional_breakpoint(0x2000, BreakpointCondition::EveryNHits(2));
        assert!(mgr2.check_pc(0x2000).is_none()); // Hit 1 -> false
        assert!(mgr2.check_pc(0x2000).is_some()); // Hit 2 -> true
        assert!(mgr2.check_pc(0x2000).is_none()); // Hit 3 -> false
        assert!(mgr2.check_pc(0x2000).is_some()); // Hit 4 -> true
        assert_eq!(mgr2.get_breakpoint(bp_every2).unwrap().hit_count, 4);
    }

    #[test]
    fn test_watchpoint_crud_and_kinds() {
        let mut mgr = BreakpointManager::new();
        assert_eq!(mgr.watchpoint_count(), 0);

        let wp_r = mgr.add_watchpoint(0x2000_0000, 4, WatchpointKind::Read);
        let _wp_w = mgr.add_watchpoint(0x2000_0010, 8, WatchpointKind::Write);
        let _wp_rw = mgr.add_watchpoint(0x2000_0020, 16, WatchpointKind::ReadWrite);

        assert_eq!(mgr.watchpoint_count(), 3);

        // Test Read Watchpoint (addr 0x2000_0000, size 4)
        // Read access at 0x2000_0000 should hit
        let hit_r = mgr.check_watchpoint(0x2000_0000, 4, false);
        assert!(hit_r.is_some());
        // Write access to same region should NOT hit Read watchpoint
        let miss_w = mgr.check_watchpoint(0x2000_0000, 4, true);
        assert!(miss_w.is_none());

        // Test Write Watchpoint (addr 0x2000_0010, size 8)
        // Read access should NOT hit
        let miss_r = mgr.check_watchpoint(0x2000_0010, 4, false);
        assert!(miss_r.is_none());
        // Write access should hit
        let hit_w = mgr.check_watchpoint(0x2000_0014, 4, true);
        assert!(hit_w.is_some());

        // Test ReadWrite Watchpoint (addr 0x2000_0020, size 16)
        let hit_rw1 = mgr.check_watchpoint(0x2000_0022, 2, false);
        assert!(hit_rw1.is_some());
        let hit_rw2 = mgr.check_watchpoint(0x2000_0028, 4, true);
        assert!(hit_rw2.is_some());

        // Test removal
        mgr.remove_watchpoint(wp_r).unwrap();
        assert_eq!(mgr.watchpoint_count(), 2);
        assert!(mgr.remove_watchpoint(wp_r).is_err());
    }

    #[test]
    fn test_watchpoint_range_overlaps() {
        let mut mgr = BreakpointManager::new();
        // Monitored region: [0x1000 .. 0x1010) (16 bytes)
        let _wp = mgr.add_watchpoint(0x1000, 16, WatchpointKind::Write);

        // Case 1: Partial overlap from left: [0x0ffc .. 0x1004)
        assert!(mgr.check_watchpoint(0x0ffc, 8, true).is_some());

        // Case 2: Partial overlap from right: [0x100c .. 0x1014)
        assert!(mgr.check_watchpoint(0x100c, 8, true).is_some());

        // Case 3: Fully contained: [0x1004 .. 0x1008)
        assert!(mgr.check_watchpoint(0x1004, 4, true).is_some());

        // Case 4: Complete containment of watchpoint: [0x0f00 .. 0x2000)
        assert!(mgr.check_watchpoint(0x0f00, 0x1100, true).is_some());

        // Case 5: Adjacent before [0x0ff8 .. 0x1000) -> No overlap
        assert!(mgr.check_watchpoint(0x0ff8, 8, true).is_none());

        // Case 6: Adjacent after [0x1010 .. 0x1018) -> No overlap
        assert!(mgr.check_watchpoint(0x1010, 8, true).is_none());
    }

    #[test]
    fn test_temporary_watchpoint() {
        let mut mgr = BreakpointManager::new();
        let _wp = mgr.add_temporary_watchpoint(0x4000, 4, WatchpointKind::Write);

        // First write hits and removes temporary watchpoint
        let hit = mgr.check_watchpoint(0x4000, 4, true);
        assert!(hit.is_some());
        assert_eq!(mgr.watchpoint_count(), 0);

        // Second write misses
        let hit2 = mgr.check_watchpoint(0x4000, 4, true);
        assert!(hit2.is_none());
    }

    #[test]
    fn test_clear_all_and_list() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x100);
        mgr.add_breakpoint(0x200);
        mgr.add_watchpoint(0x300, 4, WatchpointKind::Read);

        assert_eq!(mgr.breakpoints().len(), 2);
        assert_eq!(mgr.watchpoints().len(), 1);

        mgr.clear_breakpoints();
        assert_eq!(mgr.breakpoints().len(), 0);
        assert_eq!(mgr.watchpoints().len(), 1);

        mgr.clear_all();
        assert_eq!(mgr.breakpoints().len(), 0);
        assert_eq!(mgr.watchpoints().len(), 0);
    }

    #[test]
    fn test_breakpoint_manager_serialization() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint_with_desc(
            0x8000_0000,
            false,
            BreakpointCondition::HitCount(5),
            Some("entry_point".to_string()),
        );
        mgr.add_watchpoint_with_desc(
            0x8000_1000,
            8,
            WatchpointKind::Write,
            false,
            Some("stack_top".to_string()),
        );

        let json = serde_json::to_string(&mgr).unwrap();
        let restored: BreakpointManager = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.breakpoint_count(), 1);
        assert_eq!(restored.watchpoint_count(), 1);
        assert_eq!(
            restored.get_breakpoint(1).unwrap().description.as_deref(),
            Some("entry_point")
        );
        assert_eq!(
            restored.get_watchpoint(2).unwrap().description.as_deref(),
            Some("stack_top")
        );
    }
}
