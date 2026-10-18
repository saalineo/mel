//! Disk image file backing and Copy-on-Write (COW) overlay storage.
//!
//! Provides support for raw disk image files (`.img`, `.iso`, `.raw`), in-memory virtual
//! disks, sector-level reading/writing, and non-destructive Copy-on-Write (COW) RAM caches.

use mel_core::{MelError, MelResult, MemoryError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Standard disk sector size in bytes (512 B).
pub const SECTOR_SIZE: usize = 512;

/// Disk image manager with optional Copy-on-Write sector cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskImage {
    /// Total capacity in bytes (always a multiple of 512).
    size_bytes: usize,
    /// Whether Copy-on-Write mode is active.
    cow_enabled: bool,
    /// In-memory modified sector overlay: `sector_index -> 512-byte sector`.
    cow_overlay: HashMap<u64, Vec<u8>>,
    /// In-memory base data when not file-backed.
    #[serde(default)]
    memory_base: Vec<u8>,
    /// Optional file backing path on host.
    file_path: Option<PathBuf>,
    /// Read-only status of the base storage.
    read_only: bool,
}

impl DiskImage {
    /// Create a new blank in-memory disk image of `capacity_bytes` with COW enabled.
    pub fn new(capacity_bytes: usize) -> Self {
        let final_size = capacity_bytes.max(SECTOR_SIZE).div_ceil(SECTOR_SIZE) * SECTOR_SIZE;

        Self {
            size_bytes: final_size,
            cow_enabled: true,
            cow_overlay: HashMap::new(),
            memory_base: vec![0u8; final_size],
            file_path: None,
            read_only: false,
        }
    }

    /// Create an in-memory disk image initialized from an existing byte vector.
    pub fn from_bytes(data: Vec<u8>) -> Self {
        let mut disk = Self::new(data.len());
        disk.memory_base = data;
        let aligned_len = disk
            .memory_base
            .len()
            .max(SECTOR_SIZE)
            .div_ceil(SECTOR_SIZE)
            * SECTOR_SIZE;
        if disk.memory_base.len() < aligned_len {
            disk.memory_base.resize(aligned_len, 0);
        }
        disk.size_bytes = disk.memory_base.len();
        disk
    }

    /// Open an existing disk image file (`.img`, `.raw`, `.iso`) from disk.
    pub fn open_file(path: impl AsRef<Path>, cow: bool) -> MelResult<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let metadata = std::fs::metadata(&path_buf).map_err(|e| {
            MelError::Config(format!(
                "Failed to open disk image '{}': {e}",
                path_buf.display()
            ))
        })?;

        let file_len = metadata.len() as usize;
        let aligned_len = file_len.max(SECTOR_SIZE).div_ceil(SECTOR_SIZE) * SECTOR_SIZE;

        // Read base file contents into memory base cache for fast accesses
        let mut file = File::open(&path_buf).map_err(|e| {
            MelError::Config(format!(
                "Failed to open disk image file '{}': {e}",
                path_buf.display()
            ))
        })?;

        let mut data = Vec::with_capacity(aligned_len);
        file.read_to_end(&mut data)?;
        if data.len() < aligned_len {
            data.resize(aligned_len, 0);
        }

        Ok(Self {
            size_bytes: aligned_len,
            cow_enabled: cow,
            cow_overlay: HashMap::new(),
            memory_base: data,
            file_path: Some(path_buf),
            read_only: metadata.permissions().readonly(),
        })
    }

    /// Create a new disk image file on the host filesystem initialized with zeroes.
    pub fn create_file(path: impl AsRef<Path>, size_bytes: usize) -> MelResult<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let final_size = size_bytes.max(SECTOR_SIZE).div_ceil(SECTOR_SIZE) * SECTOR_SIZE;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path_buf)
            .map_err(|e| {
                MelError::Config(format!(
                    "Failed to create disk image '{}': {e}",
                    path_buf.display()
                ))
            })?;

        file.set_len(final_size as u64)?;
        file.flush()?;

        Ok(Self {
            size_bytes: final_size,
            cow_enabled: false,
            cow_overlay: HashMap::new(),
            memory_base: vec![0u8; final_size],
            file_path: Some(path_buf),
            read_only: false,
        })
    }

    /// Total capacity in bytes.
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    /// Total capacity in 512-byte sectors.
    pub fn sector_count(&self) -> u64 {
        (self.size_bytes / SECTOR_SIZE) as u64
    }

    /// Whether Copy-on-Write overlay is enabled.
    pub fn is_cow_enabled(&self) -> bool {
        self.cow_enabled
    }

    /// Enable or disable Copy-on-Write overlay.
    pub fn set_cow(&mut self, enabled: bool) {
        self.cow_enabled = enabled;
    }

    /// Number of modified sectors currently held in the COW overlay cache.
    pub fn dirty_sectors_count(&self) -> usize {
        self.cow_overlay.len()
    }

    /// Check if there are any uncommitted modifications in the COW cache.
    pub fn is_dirty(&self) -> bool {
        !self.cow_overlay.is_empty()
    }

    /// Discard all uncommitted sector modifications, reverting disk to base state.
    pub fn rollback(&mut self) {
        self.cow_overlay.clear();
    }

    /// Read a 512-byte sector from the disk image into `out_buf`.
    pub fn read_sector(&self, sector: u64, out_buf: &mut [u8]) -> MelResult<()> {
        if out_buf.len() < SECTOR_SIZE {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: sector,
                size: out_buf.len(),
            }));
        }

        if sector >= self.sector_count() {
            return Err(MelError::Device {
                device: "disk_image".to_string(),
                message: format!(
                    "Sector {sector} out of bounds (total sectors: {})",
                    self.sector_count()
                ),
            });
        }

        // 1. Check COW overlay first
        if self.cow_enabled {
            if let Some(cow_sector) = self.cow_overlay.get(&sector) {
                out_buf[..SECTOR_SIZE].copy_from_slice(cow_sector);
                return Ok(());
            }
        }

        // 2. Read from base memory buffer
        let offset = (sector as usize) * SECTOR_SIZE;
        out_buf[..SECTOR_SIZE].copy_from_slice(&self.memory_base[offset..offset + SECTOR_SIZE]);
        Ok(())
    }

    /// Write a 512-byte sector to the disk image from `in_buf`.
    pub fn write_sector(&mut self, sector: u64, in_buf: &[u8]) -> MelResult<()> {
        if in_buf.len() < SECTOR_SIZE {
            return Err(MelError::Memory(MemoryError::OutOfBounds {
                addr: sector,
                size: in_buf.len(),
            }));
        }

        if sector >= self.sector_count() {
            return Err(MelError::Device {
                device: "disk_image".to_string(),
                message: format!(
                    "Sector {sector} out of bounds (total sectors: {})",
                    self.sector_count()
                ),
            });
        }

        let mut sector_data = vec![0u8; SECTOR_SIZE];
        sector_data.copy_from_slice(&in_buf[..SECTOR_SIZE]);

        if self.cow_enabled {
            // Write to COW overlay cache
            self.cow_overlay.insert(sector, sector_data);
        } else {
            // Write directly to base memory buffer
            let offset = (sector as usize) * SECTOR_SIZE;
            self.memory_base[offset..offset + SECTOR_SIZE].copy_from_slice(&sector_data);

            // If file-backed and not read-only, write through to file
            if let Some(path) = &self.file_path {
                if !self.read_only {
                    let mut file = OpenOptions::new().write(true).open(path)?;
                    file.seek(SeekFrom::Start(offset as u64))?;
                    file.write_all(&sector_data)?;
                    file.flush()?;
                }
            }
        }

        Ok(())
    }

    /// Read arbitrary byte range from disk image.
    pub fn read_bytes(&self, offset: u64, buf: &mut [u8]) -> MelResult<()> {
        let mut read_len = 0;
        while read_len < buf.len() {
            let cur_offset = offset + read_len as u64;
            let sector = cur_offset / SECTOR_SIZE as u64;
            let sector_offset = (cur_offset % SECTOR_SIZE as u64) as usize;

            let mut sec_buf = [0u8; SECTOR_SIZE];
            self.read_sector(sector, &mut sec_buf)?;

            let available = SECTOR_SIZE - sector_offset;
            let to_copy = (buf.len() - read_len).min(available);
            buf[read_len..read_len + to_copy]
                .copy_from_slice(&sec_buf[sector_offset..sector_offset + to_copy]);
            read_len += to_copy;
        }
        Ok(())
    }

    /// Write arbitrary byte range to disk image.
    pub fn write_bytes(&mut self, offset: u64, buf: &[u8]) -> MelResult<()> {
        let mut written_len = 0;
        while written_len < buf.len() {
            let cur_offset = offset + written_len as u64;
            let sector = cur_offset / SECTOR_SIZE as u64;
            let sector_offset = (cur_offset % SECTOR_SIZE as u64) as usize;

            let mut sec_buf = [0u8; SECTOR_SIZE];
            self.read_sector(sector, &mut sec_buf)?;

            let available = SECTOR_SIZE - sector_offset;
            let to_copy = (buf.len() - written_len).min(available);
            sec_buf[sector_offset..sector_offset + to_copy]
                .copy_from_slice(&buf[written_len..written_len + to_copy]);

            self.write_sector(sector, &sec_buf)?;
            written_len += to_copy;
        }
        Ok(())
    }

    /// Commit all dirty COW sectors into base storage (and backing file on disk if present).
    pub fn commit(&mut self) -> MelResult<()> {
        for (&sector, data) in &self.cow_overlay {
            let offset = (sector as usize) * SECTOR_SIZE;
            self.memory_base[offset..offset + SECTOR_SIZE].copy_from_slice(data);

            if let Some(path) = &self.file_path {
                if !self.read_only {
                    let mut file = OpenOptions::new().write(true).open(path)?;
                    file.seek(SeekFrom::Start(offset as u64))?;
                    file.write_all(data)?;
                    file.flush()?;
                }
            }
        }

        self.cow_overlay.clear();
        Ok(())
    }

    /// Save the complete materialized disk contents to a destination file path.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> MelResult<()> {
        let mut file = File::create(path.as_ref())?;
        let mut full_data = self.memory_base.clone();

        // Apply COW overlay on top of copy
        for (&sector, data) in &self.cow_overlay {
            let offset = (sector as usize) * SECTOR_SIZE;
            full_data[offset..offset + SECTOR_SIZE].copy_from_slice(data);
        }

        file.write_all(&full_data)?;
        file.flush()?;
        Ok(())
    }

    /// Export the materialized full disk data as a byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut full_data = self.memory_base.clone();
        for (&sector, data) in &self.cow_overlay {
            let offset = (sector as usize) * SECTOR_SIZE;
            full_data[offset..offset + SECTOR_SIZE].copy_from_slice(data);
        }
        full_data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_image_in_memory_creation_and_geometry() {
        let disk = DiskImage::new(1024 * 1024); // 1 MB
        assert_eq!(disk.size_bytes(), 1024 * 1024);
        assert_eq!(disk.sector_count(), 2048);
        assert!(disk.is_cow_enabled());
        assert!(!disk.is_dirty());
        assert_eq!(disk.dirty_sectors_count(), 0);
    }

    #[test]
    fn test_disk_image_cow_overlay_and_rollback() {
        let mut disk = DiskImage::new(4 * 512); // 4 sectors: 0, 1, 2, 3

        let initial_payload = [0xAAu8; SECTOR_SIZE];
        let modified_payload = [0xBBu8; SECTOR_SIZE];

        // Write directly to base (with COW off)
        disk.set_cow(false);
        disk.write_sector(1, &initial_payload).unwrap();

        let mut read_buf = [0u8; SECTOR_SIZE];
        disk.read_sector(1, &mut read_buf).unwrap();
        assert_eq!(read_buf, initial_payload);

        // Turn COW on and modify sector 1
        disk.set_cow(true);
        disk.write_sector(1, &modified_payload).unwrap();
        assert!(disk.is_dirty());
        assert_eq!(disk.dirty_sectors_count(), 1);

        // Read back sees modified data
        disk.read_sector(1, &mut read_buf).unwrap();
        assert_eq!(read_buf, modified_payload);

        // Rollback discards overlay
        disk.rollback();
        assert!(!disk.is_dirty());
        assert_eq!(disk.dirty_sectors_count(), 0);

        // Read back reverts to base data (0xAA)
        disk.read_sector(1, &mut read_buf).unwrap();
        assert_eq!(read_buf, initial_payload);
    }

    #[test]
    fn test_disk_image_cow_commit() {
        let mut disk = DiskImage::new(4 * 512);
        disk.set_cow(true);

        let new_data = [0x77u8; SECTOR_SIZE];
        disk.write_sector(2, &new_data).unwrap();
        assert!(disk.is_dirty());

        // Commit to base
        disk.commit().unwrap();
        assert!(!disk.is_dirty());

        // Even after rollback, committed data is in base
        disk.rollback();
        let mut read_buf = [0u8; SECTOR_SIZE];
        disk.read_sector(2, &mut read_buf).unwrap();
        assert_eq!(read_buf, new_data);
    }

    #[test]
    fn test_disk_image_arbitrary_byte_read_write() {
        let mut disk = DiskImage::new(2 * 512);
        let msg = b"Hello MEL File-Backed Disk!";

        disk.write_bytes(100, msg).unwrap();

        let mut out = vec![0u8; msg.len()];
        disk.read_bytes(100, &mut out).unwrap();
        assert_eq!(&out, msg);
    }

    #[test]
    fn test_disk_image_file_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("mel_test_disk.img");

        // Create file
        let mut disk = DiskImage::create_file(&test_file, 4 * 512).unwrap();
        let test_payload = [0x5Au8; SECTOR_SIZE];
        disk.write_sector(0, &test_payload).unwrap();
        drop(disk);

        // Re-open file with COW enabled
        let mut reopened = DiskImage::open_file(&test_file, true).unwrap();
        let mut buf = [0u8; SECTOR_SIZE];
        reopened.read_sector(0, &mut buf).unwrap();
        assert_eq!(buf, test_payload);

        // Modify in COW mode
        let cow_payload = [0x99u8; SECTOR_SIZE];
        reopened.write_sector(0, &cow_payload).unwrap();
        reopened.read_sector(0, &mut buf).unwrap();
        assert_eq!(buf, cow_payload);

        // Commit to file
        reopened.commit().unwrap();
        drop(reopened);

        // Re-open again and verify persistence
        let final_disk = DiskImage::open_file(&test_file, false).unwrap();
        final_disk.read_sector(0, &mut buf).unwrap();
        assert_eq!(buf, cow_payload);

        // Clean up
        let _ = std::fs::remove_file(test_file);
    }
}
