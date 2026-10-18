//! # MEL Memory
//!
//! Physical RAM/ROM allocation, memory bounds checking, and memory region metadata.

pub mod ram;
pub mod region;

pub use mel_core::{Address, Endianness, MelError, MelResult, MemoryError, Permissions, Size};
pub use ram::Ram;
pub use region::MemoryRegion;
