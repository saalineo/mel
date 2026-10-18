//! Virtual Block Storage Device Controller peripheral.
//!
//! Provides a memory-mapped block device controller mapped at standard address `0x2000_0000`
//! with 512-byte sector read and write commands, status tracking, DMA buffer configuration,
//! internal sector data buffer, and completion interrupts.

use crate::device_trait::MmioDevice;
use mel_core::{Endianness, MelError, MemoryError};
use serde::{Deserialize, Serialize};

/// Standard base address for virtual block storage device.
pub const BLOCK_BASE: u64 = 0x2000_0000;

/// Standard MMIO window size for virtual block device (4 KB).
pub const BLOCK_SIZE: u64 = 0x1000;

/// Standard sector size in bytes.
pub const SECTOR_SIZE: usize = 512;

/// Magic identifier value for Block device: ASCII "BLOC" in little-endian.
pub const BLOCK_MAGIC_VAL: u32 = 0x434F_4C42;

/// Register offsets.
pub const REG_MAGIC: u64 = 0x00;
pub const REG_VERSION: u64 = 0x04;
pub const REG_CAPACITY: u64 = 0x08;
pub const REG_SECTOR_NUM: u64 = 0x10;
pub const REG_BUFFER_ADDR: u64 = 0x18;
pub const REG_COMMAND: u64 = 0x20;
pub const REG_STATUS: u64 = 0x24;
pub const REG_SECTOR_SIZE: u64 = 0x28;
pub const REG_INT_ENABLE: u64 = 0x2C;
pub const REG_INT_STATUS: u64 = 0x30;

/// Internal MMIO 512-byte sector data buffer start offset.
pub const BUFFER_OFFSET: u64 = 0x100;

/// Command opcodes.
pub const CMD_NONE: u32 = 0;
pub const CMD_READ: u32 = 1;
pub const CMD_WRITE: u32 = 2;
pub const CMD_FLUSH: u32 = 3;

/// Status flags.
pub const STATUS_READY: u32 = 0;
pub const STATUS_BUSY: u32 = 1;
pub const STATUS_ERROR: u32 = 2;

/// Virtual Block Device peripheral controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDevice {
    name: String,
    size: u64,
    sector_num: u64,
    buffer_addr: u64,
    status: u32,
    command: u32,
    int_enable: bool,
    int_pending: bool,
    sector_buffer: Vec<u8>,
    storage: Vec<u8>,
}

impl BlockDevice {
    /// Create a new block device with zeroed capacity (must be a multiple of 512 bytes).
    pub fn new(name: impl Into<String>, capacity_bytes: usize) -> Self {
        let aligned_capacity = capacity_bytes.max(SECTOR_SIZE);
        Self {
            name: name.into(),
            size: BLOCK_SIZE,
            sector_num: 0,
            buffer_addr: 0,
            status: STATUS_READY,
            command: CMD_NONE,
            int_enable: false,
            int_pending: false,
            sector_buffer: vec![0u8; SECTOR_SIZE],
            storage: vec![0u8; aligned_capacity],
        }
    }

    /// Create a block device initialized with raw storage bytes.
    pub fn with_data(name: impl Into<String>, data: Vec<u8>) -> Self {
        let mut dev = Self::new(name, data.len());
        dev.storage = data;
        // Pad to sector size if needed
        let rem = dev.storage.len() % SECTOR_SIZE;
        if rem != 0 {
            dev.storage
                .resize(dev.storage.len() + (SECTOR_SIZE - rem), 0);
        }
        dev
    }

    /// Capacity in 512-byte sectors.
    pub fn capacity_sectors(&self) -> u64 {
        (self.storage.len() / SECTOR_SIZE) as u64
    }

    /// Capacity in total bytes.
    pub fn capacity_bytes(&self) -> usize {
        self.storage.len()
    }

    /// Current target sector number (LBA).
    pub fn sector_num(&self) -> u64 {
        self.sector_num
    }

    /// Set target sector number.
    pub fn set_sector_num(&mut self, sector: u64) {
        self.sector_num = sector;
    }

    /// Current guest buffer address.
    pub fn buffer_addr(&self) -> u64 {
        self.buffer_addr
    }

    /// Set guest buffer address.
    pub fn set_buffer_addr(&mut self, addr: u64) {
        self.buffer_addr = addr;
    }

    /// Current controller status.
    pub fn status(&self) -> u32 {
        self.status
    }

    /// Internal 512-byte sector buffer.
    pub fn sector_buffer(&self) -> &[u8] {
        &self.sector_buffer
    }

    /// Mutable reference to internal sector buffer.
    pub fn sector_buffer_mut(&mut self) -> &mut [u8] {
        &mut self.sector_buffer
    }

    /// Immutable access to entire raw storage backing data.
    pub fn raw_data(&self) -> &[u8] {
        &self.storage
    }

    /// Mutable access to raw storage backing data.
    pub fn raw_data_mut(&mut self) -> &mut [u8] {
        &mut self.storage
    }

    /// Check if device interrupt line is currently asserted.
    pub fn is_interrupting(&self) -> bool {
        self.int_enable && self.int_pending
    }

    /// Direct sector read from storage backing.
    pub fn read_sector(&self, sector: u64, out_buf: &mut [u8]) -> Result<(), MelError> {
        let offset = (sector as usize).saturating_mul(SECTOR_SIZE);
        if offset + SECTOR_SIZE <= self.storage.len() && out_buf.len() >= SECTOR_SIZE {
            out_buf[..SECTOR_SIZE].copy_from_slice(&self.storage[offset..offset + SECTOR_SIZE]);
            Ok(())
        } else {
            Err(MelError::Device {
                device: self.name.clone(),
                message: format!(
                    "Sector index {sector} out of bounds (max: {})",
                    self.capacity_sectors()
                ),
            })
        }
    }

    /// Direct sector write into storage backing.
    pub fn write_sector(&mut self, sector: u64, in_buf: &[u8]) -> Result<(), MelError> {
        let offset = (sector as usize).saturating_mul(SECTOR_SIZE);
        if offset + SECTOR_SIZE <= self.storage.len() && in_buf.len() >= SECTOR_SIZE {
            self.storage[offset..offset + SECTOR_SIZE].copy_from_slice(&in_buf[..SECTOR_SIZE]);
            Ok(())
        } else {
            Err(MelError::Device {
                device: self.name.clone(),
                message: format!(
                    "Sector index {sector} out of bounds (max: {})",
                    self.capacity_sectors()
                ),
            })
        }
    }

    /// Execute a command write.
    fn execute_command(&mut self, cmd: u32) {
        self.command = cmd;
        match cmd {
            CMD_READ => {
                let offset = (self.sector_num as usize).saturating_mul(SECTOR_SIZE);
                if offset + SECTOR_SIZE <= self.storage.len() {
                    self.sector_buffer
                        .copy_from_slice(&self.storage[offset..offset + SECTOR_SIZE]);
                    self.status = STATUS_READY;
                } else {
                    self.status = STATUS_ERROR;
                }
                if self.int_enable {
                    self.int_pending = true;
                }
            }
            CMD_WRITE => {
                let offset = (self.sector_num as usize).saturating_mul(SECTOR_SIZE);
                if offset + SECTOR_SIZE <= self.storage.len() {
                    self.storage[offset..offset + SECTOR_SIZE].copy_from_slice(&self.sector_buffer);
                    self.status = STATUS_READY;
                } else {
                    self.status = STATUS_ERROR;
                }
                if self.int_enable {
                    self.int_pending = true;
                }
            }
            CMD_FLUSH => {
                self.status = STATUS_READY;
                if self.int_enable {
                    self.int_pending = true;
                }
            }
            _ => {
                self.status = STATUS_ERROR;
            }
        }
    }
}

impl MmioDevice for BlockDevice {
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

        // 1. MMIO Sector Buffer (0x0100 - 0x02FF)
        if (BUFFER_OFFSET..BUFFER_OFFSET + SECTOR_SIZE as u64).contains(&offset) {
            let buf_offset = (offset - BUFFER_OFFSET) as usize;
            let copy_len = size.min(SECTOR_SIZE.saturating_sub(buf_offset));
            if copy_len > 0 {
                buf[..copy_len]
                    .copy_from_slice(&self.sector_buffer[buf_offset..buf_offset + copy_len]);
            }
            return Endianness::Little.read_u64(&buf).ok_or(MelError::Memory(
                MemoryError::OutOfBounds { addr: offset, size },
            ));
        }

        // 2. Control Registers
        let val: u64 = match offset {
            REG_MAGIC => BLOCK_MAGIC_VAL as u64,
            REG_VERSION => 1,
            REG_CAPACITY => self.capacity_sectors(),
            REG_SECTOR_NUM => self.sector_num,
            REG_BUFFER_ADDR => self.buffer_addr,
            REG_COMMAND => self.command as u64,
            REG_STATUS => self.status as u64,
            REG_SECTOR_SIZE => SECTOR_SIZE as u64,
            REG_INT_ENABLE => u64::from(self.int_enable),
            REG_INT_STATUS => u64::from(self.int_pending),
            _ => 0,
        };

        let val_bytes = val.to_le_bytes();
        let copy_len = size.min(8);
        buf[..copy_len].copy_from_slice(&val_bytes[..copy_len]);

        Endianness::Little
            .read_u64(&buf)
            .ok_or(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset,
                size,
            }))
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), MelError> {
        if size == 0 || size > 8 || offset.saturating_add(size as u64) > self.size {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: offset,
                size: self.size as usize,
            }));
        }

        // 1. MMIO Sector Buffer (0x0100 - 0x02FF)
        if (BUFFER_OFFSET..BUFFER_OFFSET + SECTOR_SIZE as u64).contains(&offset) {
            let buf_offset = (offset - BUFFER_OFFSET) as usize;
            let in_bytes = value.to_le_bytes();
            let copy_len = size.min(SECTOR_SIZE.saturating_sub(buf_offset));
            if copy_len > 0 {
                self.sector_buffer[buf_offset..buf_offset + copy_len]
                    .copy_from_slice(&in_bytes[..copy_len]);
            }
            return Ok(());
        }

        // 2. Control Registers
        match offset {
            REG_SECTOR_NUM => {
                if size == 8 {
                    self.sector_num = value;
                } else if size == 4 {
                    self.sector_num = value as u32 as u64;
                }
            }
            REG_BUFFER_ADDR => {
                if size == 8 {
                    self.buffer_addr = value;
                } else if size == 4 {
                    self.buffer_addr = value as u32 as u64;
                }
            }
            REG_COMMAND => {
                self.execute_command(value as u32);
            }
            REG_STATUS => {
                self.status = value as u32;
            }
            REG_INT_ENABLE => {
                self.int_enable = (value & 1) != 0;
                if !self.int_enable {
                    self.int_pending = false;
                }
            }
            REG_INT_STATUS if (value & 1) != 0 => {
                // Write 1 to clear (W1C)
                self.int_pending = false;
            }
            _ => {}
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<(), MelError> {
        self.sector_num = 0;
        self.buffer_addr = 0;
        self.status = STATUS_READY;
        self.command = CMD_NONE;
        self.int_enable = false;
        self.int_pending = false;
        self.sector_buffer.fill(0);
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
    fn test_block_device_initialization() {
        let dev = BlockDevice::new("blk0", 1024 * 1024); // 1 MB = 2048 sectors
        assert_eq!(dev.name(), "blk0");
        assert_eq!(dev.size(), BLOCK_SIZE);
        assert_eq!(dev.capacity_sectors(), 2048);
        assert_eq!(dev.capacity_bytes(), 1024 * 1024);
        assert_eq!(dev.status(), STATUS_READY);
        assert_eq!(dev.sector_num(), 0);
        assert!(!dev.is_interrupting());
    }

    #[test]
    fn test_block_device_mmio_identification_registers() {
        let mut dev = BlockDevice::new("blk0", 64 * 512);

        assert_eq!(dev.read(REG_MAGIC, 4).unwrap(), BLOCK_MAGIC_VAL as u64);
        assert_eq!(dev.read(REG_VERSION, 4).unwrap(), 1);
        assert_eq!(dev.read(REG_CAPACITY, 8).unwrap(), 64);
        assert_eq!(dev.read(REG_SECTOR_SIZE, 4).unwrap(), 512);
        assert_eq!(dev.read(REG_STATUS, 4).unwrap(), STATUS_READY as u64);
    }

    #[test]
    fn test_block_device_sector_write_and_read_mmio_flow() {
        let mut dev = BlockDevice::new("blk0", 16 * 512);

        // 1. Write custom payload to internal sector buffer via MMIO (offset 0x100)
        let sample_text = b"Hello MEL Virtual Disk Controller!";
        for (i, &b) in sample_text.iter().enumerate() {
            dev.write(BUFFER_OFFSET + i as u64, b as u64, 1).unwrap();
        }

        // 2. Set target sector number to 5
        dev.write(REG_SECTOR_NUM, 5, 8).unwrap();
        assert_eq!(dev.sector_num(), 5);

        // 3. Issue CMD_WRITE (2)
        dev.write(REG_COMMAND, CMD_WRITE as u64, 4).unwrap();
        assert_eq!(dev.status(), STATUS_READY);

        // 4. Clear internal sector buffer
        for i in 0..sample_text.len() {
            dev.write(BUFFER_OFFSET + i as u64, 0, 1).unwrap();
        }
        assert_eq!(dev.read(BUFFER_OFFSET, 1).unwrap(), 0);

        // 5. Issue CMD_READ (1) for sector 5
        dev.write(REG_SECTOR_NUM, 5, 8).unwrap();
        dev.write(REG_COMMAND, CMD_READ as u64, 4).unwrap();
        assert_eq!(dev.status(), STATUS_READY);

        // 6. Verify data restored in internal sector buffer
        let mut read_back = Vec::new();
        for i in 0..sample_text.len() {
            let byte = dev.read(BUFFER_OFFSET + i as u64, 1).unwrap() as u8;
            read_back.push(byte);
        }
        assert_eq!(&read_back, sample_text);
    }

    #[test]
    fn test_block_device_out_of_bounds_error() {
        let mut dev = BlockDevice::new("blk0", 4 * 512); // 4 sectors: 0, 1, 2, 3

        // Target out-of-bounds sector 10
        dev.write(REG_SECTOR_NUM, 10, 8).unwrap();
        dev.write(REG_COMMAND, CMD_READ as u64, 4).unwrap();
        assert_eq!(dev.status(), STATUS_ERROR);

        dev.write(REG_COMMAND, CMD_WRITE as u64, 4).unwrap();
        assert_eq!(dev.status(), STATUS_ERROR);
    }

    #[test]
    fn test_block_device_interrupt_assertion_and_ack() {
        let mut dev = BlockDevice::new("blk0", 8 * 512);

        // Enable interrupts
        dev.write(REG_INT_ENABLE, 1, 4).unwrap();
        assert!(!dev.is_interrupting());

        // Perform read command on sector 0
        dev.write(REG_SECTOR_NUM, 0, 8).unwrap();
        dev.write(REG_COMMAND, CMD_READ as u64, 4).unwrap();

        // Interrupt line must be asserted
        assert!(dev.is_interrupting());
        assert_eq!(dev.read(REG_INT_STATUS, 4).unwrap(), 1);

        // Acknowledge interrupt (W1C)
        dev.write(REG_INT_STATUS, 1, 4).unwrap();
        assert!(!dev.is_interrupting());
        assert_eq!(dev.read(REG_INT_STATUS, 4).unwrap(), 0);
    }

    #[test]
    fn test_block_device_save_restore_snapshot() {
        let mut dev = BlockDevice::new("blk0", 8 * 512);
        dev.set_sector_num(3);
        dev.sector_buffer_mut()[0] = 0xAA;
        dev.sector_buffer_mut()[511] = 0x55;
        dev.raw_data_mut()[3 * 512] = 0x77;

        let snapshot = dev.save_state().unwrap();

        let mut restored = BlockDevice::new("blk0", 8 * 512);
        restored.load_state(&snapshot).unwrap();

        assert_eq!(restored.sector_num(), 3);
        assert_eq!(restored.sector_buffer()[0], 0xAA);
        assert_eq!(restored.sector_buffer()[511], 0x55);
        assert_eq!(restored.raw_data()[3 * 512], 0x77);
    }
}
