//! Physical System Bus implementation with address decoding and MMIO routing.

use std::sync::{Arc, Mutex};
use mel_core::{Address, BusError, Endianness, MelError, MelResult, Size};
use mel_memory::Ram;
use crate::device::BusDevice;

/// Represents an active mapped region on the system bus.
pub enum MappedTarget {
    Ram(Arc<Mutex<Ram>>),
    Device(Arc<Mutex<dyn BusDevice>>),
}

/// Metadata describing a mapped address region.
#[derive(Clone, Debug)]
pub struct RegionInfo {
    pub name: String,
    pub base: Address,
    pub size: Size,
    pub is_ram: bool,
}

struct BusEntry {
    base: Address,
    size: Size,
    name: String,
    target: MappedTarget,
}

impl BusEntry {
    #[inline]
    fn contains(&self, addr: Address) -> bool {
        addr >= self.base && addr < self.base.saturating_add(self.size)
    }

    #[inline]
    fn overlaps(&self, other_base: Address, other_size: Size) -> bool {
        let self_end = self.base.saturating_add(self.size);
        let other_end = other_base.saturating_add(other_size);
        self.base < other_end && other_base < self_end
    }
}

/// The system bus dispatching read/write transactions to RAM and MMIO peripherals.
#[derive(Default)]
pub struct SystemBus {
    entries: Vec<BusEntry>,
}

impl SystemBus {
    /// Create a new, empty system bus.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Map a RAM buffer at the specified base address.
    pub fn map_ram(&mut self, name: impl Into<String>, base: Address, ram: Arc<Mutex<Ram>>) -> MelResult<()> {
        let size = ram.lock().expect("bus RAM lock poisoned").size() as u64;
        self.check_overlap(base, size)?;
        self.entries.push(BusEntry {
            base,
            size,
            name: name.into(),
            target: MappedTarget::Ram(ram),
        });
        Ok(())
    }

    /// Map an MMIO device at the specified base address and size.
    pub fn map_device(
        &mut self,
        name: impl Into<String>,
        base: Address,
        size: Size,
        device: Arc<Mutex<dyn BusDevice>>,
    ) -> MelResult<()> {
        self.check_overlap(base, size)?;
        self.entries.push(BusEntry {
            base,
            size,
            name: name.into(),
            target: MappedTarget::Device(device),
        });
        Ok(())
    }

    fn find_entry(&self, addr: Address) -> Option<&BusEntry> {
        self.entries.iter().find(|e| e.contains(addr))
    }

    fn check_overlap(&self, base: Address, size: Size) -> MelResult<()> {
        for entry in &self.entries {
            if entry.overlaps(base, size) {
                return Err(MelError::Bus(BusError::OverlappingRegion {
                    start: base,
                    end: base.saturating_add(size),
                }));
            }
        }
        Ok(())
    }

    /// Returns a list of all mapped regions currently on the bus.
    pub fn list_regions(&self) -> Vec<RegionInfo> {
        self.entries
            .iter()
            .map(|e| RegionInfo {
                name: e.name.clone(),
                base: e.base,
                size: e.size,
                is_ram: matches!(e.target, MappedTarget::Ram(_)),
            })
            .collect()
    }

    /// Read an integer of 1, 2, 4, or 8 bytes from the system bus.
    pub fn read(&self, addr: Address, size: usize, endianness: Endianness) -> MelResult<u64> {
        let entry = self.find_entry(addr).ok_or(BusError::UnmappedAddress { addr, size })?;
        let offset = (addr - entry.base) as usize;
        match &entry.target {
            MappedTarget::Ram(ram_lock) => {
                let ram = ram_lock.lock().expect("bus RAM lock poisoned");
                match size {
                    1 => ram.read_u8(offset).map(u64::from).map_err(MelError::Memory),
                    2 => ram.read_u16(offset, endianness).map(u64::from).map_err(MelError::Memory),
                    4 => ram.read_u32(offset, endianness).map(u64::from).map_err(MelError::Memory),
                    8 => ram.read_u64(offset, endianness).map_err(MelError::Memory),
                    _ => Err(MelError::Bus(BusError::UnmappedAddress { addr, size })),
                }
            }
            MappedTarget::Device(dev_lock) => {
                let mut dev = dev_lock.lock().expect("bus device lock poisoned");
                dev.read(offset as u64, size)
            }
        }
    }

    /// Write an integer of 1, 2, 4, or 8 bytes to the system bus.
    pub fn write(&mut self, addr: Address, val: u64, size: usize, endianness: Endianness) -> MelResult<()> {
        let entry = self.find_entry(addr).ok_or(BusError::UnmappedAddress { addr, size })?;
        let offset = (addr - entry.base) as usize;
        match &entry.target {
            MappedTarget::Ram(ram_lock) => {
                let mut ram = ram_lock.lock().expect("bus RAM lock poisoned");
                match size {
                    1 => ram.write_u8(offset, val as u8).map_err(MelError::Memory),
                    2 => ram.write_u16(offset, val as u16, endianness).map_err(MelError::Memory),
                    4 => ram.write_u32(offset, val as u32, endianness).map_err(MelError::Memory),
                    8 => ram.write_u64(offset, val, endianness).map_err(MelError::Memory),
                    _ => Err(MelError::Bus(BusError::UnmappedAddress { addr, size })),
                }
            }
            MappedTarget::Device(dev_lock) => {
                let mut dev = dev_lock.lock().expect("bus device lock poisoned");
                dev.write(offset as u64, val, size)
            }
        }
    }

    /// Read an 8-bit unsigned integer.
    #[inline]
    pub fn read_u8(&self, addr: Address) -> MelResult<u8> {
        self.read(addr, 1, Endianness::Little).map(|v| v as u8)
    }

    /// Write an 8-bit unsigned integer.
    #[inline]
    pub fn write_u8(&mut self, addr: Address, val: u8) -> MelResult<()> {
        self.write(addr, val as u64, 1, Endianness::Little)
    }

    /// Read raw bytes in bulk.
    pub fn read_bytes(&self, addr: Address, buf: &mut [u8]) -> MelResult<()> {
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = self.read_u8(addr.wrapping_add(i as u64))?;
        }
        Ok(())
    }

    /// Write raw bytes in bulk.
    pub fn write_bytes(&mut self, addr: Address, bytes: &[u8]) -> MelResult<()> {
        for (i, &byte) in bytes.iter().enumerate() {
            self.write_u8(addr.wrapping_add(i as u64), byte)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockUart {
        last_val: u8,
    }

    impl BusDevice for MockUart {
        fn name(&self) -> &str {
            "mock_uart"
        }

        fn read(&mut self, _offset: Address, _size: usize) -> Result<u64, MelError> {
            Ok(self.last_val as u64)
        }

        fn write(&mut self, _offset: Address, value: u64, _size: usize) -> Result<(), MelError> {
            self.last_val = value as u8;
            Ok(())
        }
    }

    #[test]
    fn test_bus_routing_and_unmapped() {
        let mut bus = SystemBus::new();
        let ram = Arc::new(Mutex::new(Ram::new(0x1000)));
        bus.map_ram("RAM", 0x8000_0000, ram).unwrap();

        let uart = Arc::new(Mutex::new(MockUart { last_val: 0 }));
        bus.map_device("UART", 0x1000_0000, 0x100, uart).unwrap();

        bus.write(0x8000_0000, 0x12345678, 4, Endianness::Little).unwrap();
        assert_eq!(bus.read(0x8000_0000, 4, Endianness::Little).unwrap(), 0x12345678);

        bus.write_u8(0x1000_0000, 0x42).unwrap();
        assert_eq!(bus.read_u8(0x1000_0000).unwrap(), 0x42);

        assert!(bus.read_u8(0x5000_0000).is_err());
    }

    #[test]
    fn test_overlapping_region_error() {
        let mut bus = SystemBus::new();
        let ram1 = Arc::new(Mutex::new(Ram::new(0x1000)));
        let ram2 = Arc::new(Mutex::new(Ram::new(0x1000)));

        bus.map_ram("RAM1", 0x8000_0000, ram1).unwrap();
        let err = bus.map_ram("RAM2", 0x8000_0800, ram2);
        assert!(err.is_err());
    }
}
