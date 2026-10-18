//! FAT filesystem host tooling integration (FAT12, FAT16, FAT32).
//!
//! Provides utilities to format, inspect, inject, and extract files and directories
//! from virtual disk images backed by [`DiskImage`].

use crate::disk::{DiskImage, SECTOR_SIZE};
use fatfs::{FatType, FileSystem, FormatVolumeOptions, FsOptions};
use mel_core::{MelError, MelResult};
use std::io::{Read, Seek, SeekFrom, Write};

/// I/O stream adapter for [`DiskImage`] providing [`Read`], [`Write`], and [`Seek`].
pub struct DiskImageStream<'a> {
    disk: &'a mut DiskImage,
    pos: u64,
}

impl<'a> DiskImageStream<'a> {
    /// Create a new stream wrapper around a mutable reference to a [`DiskImage`].
    pub fn new(disk: &'a mut DiskImage) -> Self {
        Self { disk, pos: 0 }
    }
}

impl<'a> Read for DiskImageStream<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let total_size = self.disk.size_bytes() as u64;
        if self.pos >= total_size {
            return Ok(0);
        }
        let available = (total_size - self.pos) as usize;
        let read_len = buf.len().min(available);
        if read_len == 0 {
            return Ok(0);
        }
        self.disk
            .read_bytes(self.pos, &mut buf[..read_len])
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.pos += read_len as u64;
        Ok(read_len)
    }
}

impl<'a> Write for DiskImageStream<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let total_size = self.disk.size_bytes() as u64;
        if self.pos >= total_size {
            return Ok(0);
        }
        let available = (total_size - self.pos) as usize;
        let write_len = buf.len().min(available);
        if write_len == 0 {
            return Ok(0);
        }
        self.disk
            .write_bytes(self.pos, &buf[..write_len])
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.pos += write_len as u64;
        Ok(write_len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> Seek for DiskImageStream<'a> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let total_size = self.disk.size_bytes() as i64;
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => total_size + offset,
            SeekFrom::Current(offset) => self.pos as i64 + offset,
        };
        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot seek to negative position",
            ));
        }
        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

/// Metadata entry returned when listing a directory in a FAT filesystem image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FatDirEntry {
    /// File or directory name.
    pub name: String,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Whether this entry is a subdirectory.
    pub is_dir: bool,
}

/// Host filesystem utility for interacting with FAT filesystems on a [`DiskImage`].
pub struct FatFs;

impl FatFs {
    /// Format a [`DiskImage`] with a standard FAT filesystem.
    ///
    /// Automatically selects FAT12, FAT16, or FAT32 based on disk capacity:
    /// - < 4 MiB: FAT12
    /// - 4 MiB .. 512 MiB: FAT16
    /// - >= 512 MiB: FAT32
    pub fn format(disk: &mut DiskImage, label: Option<&str>) -> MelResult<()> {
        let size = disk.size_bytes();
        if size < 512 * 1024 {
            return Err(MelError::Config(format!(
                "Disk capacity ({} bytes) too small for FAT filesystem (minimum 512 KiB)",
                size
            )));
        }

        let fat_type = if size < 4 * 1024 * 1024 {
            FatType::Fat12
        } else if size < 512 * 1024 * 1024 {
            FatType::Fat16
        } else {
            FatType::Fat32
        };

        let mut options = FormatVolumeOptions::new().fat_type(fat_type);
        if let Some(lbl) = label {
            let mut label_bytes = [b' '; 11];
            let bytes = lbl.as_bytes();
            let copy_len = bytes.len().min(11);
            label_bytes[..copy_len].copy_from_slice(&bytes[..copy_len]);
            options = options.volume_label(label_bytes);
        }

        let mut stream = DiskImageStream::new(disk);
        fatfs::format_volume(&mut stream, options).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to format FAT volume: {e}"),
        })?;

        Ok(())
    }

    /// Create and format a new in-memory [`DiskImage`] with a FAT filesystem.
    pub fn create_image(capacity_bytes: usize, label: Option<&str>) -> MelResult<DiskImage> {
        let capacity = capacity_bytes.max(1440 * 1024).div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
        let mut disk = DiskImage::new(capacity);
        Self::format(&mut disk, label)?;
        Ok(disk)
    }

    /// Inject a file into a FAT filesystem at the specified guest path.
    ///
    /// Parent directories will be automatically created if they do not exist.
    /// Path separators can be `/` or `\`.
    pub fn inject_file(disk: &mut DiskImage, path: &str, content: &[u8]) -> MelResult<()> {
        let (dir_path, filename) = split_path(path)?;
        let mut stream = DiskImageStream::new(disk);
        let fs = FileSystem::new(&mut stream, FsOptions::new()).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to mount FAT filesystem: {e}"),
        })?;

        let root_dir = fs.root_dir();
        let target_dir = navigate_or_create_dir(&root_dir, &dir_path)?;

        let mut file = target_dir
            .create_file(&filename)
            .map_err(|e| MelError::Device {
                device: "fatfs".into(),
                message: format!("Failed to create file '{path}': {e}"),
            })?;

        file.truncate().map_err(MelError::Io)?;
        file.write_all(content).map_err(MelError::Io)?;
        file.flush().map_err(MelError::Io)?;

        Ok(())
    }

    /// Extract the byte contents of a file at the specified guest path.
    pub fn extract_file(disk: &mut DiskImage, path: &str) -> MelResult<Vec<u8>> {
        let (dir_path, filename) = split_path(path)?;
        let mut stream = DiskImageStream::new(disk);
        let fs = FileSystem::new(&mut stream, FsOptions::new()).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to mount FAT filesystem: {e}"),
        })?;

        let root_dir = fs.root_dir();
        let target_dir = navigate_dir(&root_dir, &dir_path)?;

        let mut file = target_dir
            .open_file(&filename)
            .map_err(|e| MelError::Device {
                device: "fatfs".into(),
                message: format!("Failed to open file '{path}': {e}"),
            })?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(MelError::Io)?;
        Ok(buf)
    }

    /// Check whether a file or directory exists at `path`.
    pub fn exists(disk: &mut DiskImage, path: &str) -> MelResult<bool> {
        let (dir_path, filename) = split_path(path)?;
        let mut stream = DiskImageStream::new(disk);
        let fs = FileSystem::new(&mut stream, FsOptions::new()).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to mount FAT filesystem: {e}"),
        })?;

        let root_dir = fs.root_dir();
        let target_dir = match navigate_dir(&root_dir, &dir_path) {
            Ok(d) => d,
            Err(_) => return Ok(false),
        };

        for entry in target_dir.iter() {
            let entry = entry.map_err(MelError::Io)?;
            if entry.file_name().eq_ignore_ascii_case(&filename) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// List entries in a directory inside the FAT filesystem.
    pub fn list_dir(disk: &mut DiskImage, path: &str) -> MelResult<Vec<FatDirEntry>> {
        let mut stream = DiskImageStream::new(disk);
        let fs = FileSystem::new(&mut stream, FsOptions::new()).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to mount FAT filesystem: {e}"),
        })?;

        let root_dir = fs.root_dir();
        let clean = path.trim().replace('\\', "/");
        let clean_path = clean.trim_matches('/');

        let target_dir = if clean_path.is_empty() {
            root_dir
        } else {
            navigate_dir(&root_dir, clean_path)?
        };

        let mut entries = Vec::new();
        for entry_res in target_dir.iter() {
            let entry = entry_res.map_err(MelError::Io)?;
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            entries.push(FatDirEntry {
                name,
                size: entry.len(),
                is_dir: entry.is_dir(),
            });
        }

        Ok(entries)
    }

    /// Delete a file from the FAT filesystem.
    pub fn remove_file(disk: &mut DiskImage, path: &str) -> MelResult<()> {
        let (dir_path, filename) = split_path(path)?;
        let mut stream = DiskImageStream::new(disk);
        let fs = FileSystem::new(&mut stream, FsOptions::new()).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to mount FAT filesystem: {e}"),
        })?;

        let root_dir = fs.root_dir();
        let target_dir = navigate_dir(&root_dir, &dir_path)?;

        target_dir.remove(&filename).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to remove '{path}': {e}"),
        })?;

        Ok(())
    }
}

/// Helper function to split a path into parent directory components and leaf file/dir name.
fn split_path(path: &str) -> MelResult<(String, String)> {
    let clean = path.trim().replace('\\', "/");
    let clean_str = clean.trim_matches('/');
    if clean_str.is_empty() {
        return Err(MelError::Config(
            "Invalid empty path for FAT operation".into(),
        ));
    }

    if let Some(pos) = clean_str.rfind('/') {
        let parent = clean_str[..pos].to_string();
        let filename = clean_str[pos + 1..].to_string();
        Ok((parent, filename))
    } else {
        Ok((String::new(), clean_str.to_string()))
    }
}

/// Traverse directories starting from `root_dir`, creating missing directories as needed.
fn navigate_or_create_dir<'a, IO: Read + Write + Seek>(
    root_dir: &'a fatfs::Dir<'a, IO>,
    dir_path: &str,
) -> MelResult<fatfs::Dir<'a, IO>> {
    if dir_path.is_empty() {
        return Ok(root_dir.clone());
    }

    let mut current = root_dir.clone();
    for comp in dir_path.split('/').filter(|s| !s.is_empty()) {
        current = match current.open_dir(comp) {
            Ok(dir) => dir,
            Err(_) => current.create_dir(comp).map_err(|e| MelError::Device {
                device: "fatfs".into(),
                message: format!("Failed to create directory '{comp}': {e}"),
            })?,
        };
    }

    Ok(current)
}

/// Traverse directories starting from `root_dir` without creating missing ones.
fn navigate_dir<'a, IO: Read + Write + Seek>(
    root_dir: &'a fatfs::Dir<'a, IO>,
    dir_path: &str,
) -> MelResult<fatfs::Dir<'a, IO>> {
    if dir_path.is_empty() {
        return Ok(root_dir.clone());
    }

    let mut current = root_dir.clone();
    for comp in dir_path.split('/').filter(|s| !s.is_empty()) {
        current = current.open_dir(comp).map_err(|e| MelError::Device {
            device: "fatfs".into(),
            message: format!("Failed to open directory '{comp}': {e}"),
        })?;
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fat_create_image_and_file_injection() {
        let mut disk = FatFs::create_image(1440 * 1024, Some("MELBOOT")).expect("create fat image");

        let hello_bytes = b"Hello from MEL guest FAT filesystem!\n";
        FatFs::inject_file(&mut disk, "boot.txt", hello_bytes).expect("inject file");

        assert!(FatFs::exists(&mut disk, "boot.txt").unwrap());
        let extracted = FatFs::extract_file(&mut disk, "boot.txt").expect("extract file");
        assert_eq!(extracted, hello_bytes);
    }

    #[test]
    fn test_fat_nested_directories_and_listing() {
        // 16 MB disk -> FAT16
        let mut disk = FatFs::create_image(16 * 1024 * 1024, Some("MELOS")).expect("create image");

        let kernel_elf = vec![0x7f, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00];
        let init_script = b"echo 'Initializing system...'\n";

        FatFs::inject_file(&mut disk, "/boot/kernel.elf", &kernel_elf).expect("inject kernel");
        FatFs::inject_file(&mut disk, "/etc/init.sh", init_script).expect("inject init");

        assert!(FatFs::exists(&mut disk, "/boot/kernel.elf").unwrap());
        assert!(FatFs::exists(&mut disk, "/etc/init.sh").unwrap());
        assert!(!FatFs::exists(&mut disk, "/boot/nonexistent.bin").unwrap());

        let extracted_kernel = FatFs::extract_file(&mut disk, "/boot/kernel.elf").unwrap();
        assert_eq!(extracted_kernel, kernel_elf);

        let root_entries = FatFs::list_dir(&mut disk, "/").unwrap();
        let dir_names: Vec<String> = root_entries.iter().map(|e| e.name.clone()).collect();
        assert!(dir_names.iter().any(|n| n.eq_ignore_ascii_case("BOOT")));
        assert!(dir_names.iter().any(|n| n.eq_ignore_ascii_case("ETC")));

        let boot_entries = FatFs::list_dir(&mut disk, "/boot").unwrap();
        assert_eq!(boot_entries.len(), 1);
        assert!(boot_entries[0].name.eq_ignore_ascii_case("kernel.elf"));
        assert_eq!(boot_entries[0].size, kernel_elf.len() as u64);
        assert!(!boot_entries[0].is_dir);
    }

    #[test]
    fn test_fat_file_removal() {
        let mut disk = FatFs::create_image(2 * 1024 * 1024, None).expect("create image");

        FatFs::inject_file(&mut disk, "temp.log", b"temp data").unwrap();
        assert!(FatFs::exists(&mut disk, "temp.log").unwrap());

        FatFs::remove_file(&mut disk, "temp.log").unwrap();
        assert!(!FatFs::exists(&mut disk, "temp.log").unwrap());
    }

    #[test]
    fn test_fat_cow_overlay_isolation() {
        let mut disk = FatFs::create_image(4 * 1024 * 1024, Some("BASE")).unwrap();
        disk.commit().unwrap(); // base clean state
        assert!(!disk.is_dirty());

        // Inject file into COW overlay
        FatFs::inject_file(&mut disk, "new_file.dat", b"overlay data").unwrap();
        assert!(disk.is_dirty());
        assert!(FatFs::exists(&mut disk, "new_file.dat").unwrap());

        // Rollback -> should revert to clean base state where new_file.dat does not exist
        disk.rollback();
        assert!(!disk.is_dirty());
        assert!(!FatFs::exists(&mut disk, "new_file.dat").unwrap());
    }
}
