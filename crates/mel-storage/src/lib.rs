//! MEL Storage: Disk image management, Copy-on-Write overlays, and FAT filesystem host tooling.

pub mod disk;
pub mod fat;

pub use disk::{DiskImage, SECTOR_SIZE};
pub use fat::{DiskImageStream, FatDirEntry, FatFs};
pub use mel_core::{MelError, MelResult};
