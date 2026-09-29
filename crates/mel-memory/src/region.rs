//! Memory region representation with access permissions and metadata.

use mel_core::{Address, Permissions, Size};
use serde::{Deserialize, Serialize};

/// Metadata and bounding box for a physical memory region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRegion {
    pub name: String,
    pub base: Address,
    pub size: Size,
    pub permissions: Permissions,
}

impl MemoryRegion {
    pub fn new(name: impl Into<String>, base: Address, size: Size, permissions: Permissions) -> Self {
        Self {
            name: name.into(),
            base,
            size,
            permissions,
        }
    }

    /// Check if the given address falls within this region.
    #[inline]
    pub fn contains(&self, addr: Address) -> bool {
        addr >= self.base && addr < self.base.saturating_add(self.size)
    }

    /// Convert a physical address to a local offset within this region.
    #[inline]
    pub fn offset_of(&self, addr: Address) -> Option<usize> {
        self.contains(addr).then_some((addr - self.base) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_region_bounds() {
        let region = MemoryRegion::new("RAM", 0x8000_0000, 0x1000, Permissions::ALL);
        assert!(region.contains(0x8000_0000));
        assert!(region.contains(0x8000_0FFF));
        assert!(!region.contains(0x8000_1000));
        assert!(!region.contains(0x7FFF_FFFF));
        assert_eq!(region.offset_of(0x8000_0020), Some(0x20));
        assert_eq!(region.offset_of(0x9000_0000), None);
    }
}
