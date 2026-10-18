//! # MEL Bus
//!
//! System bus, address decoding, and memory/MMIO routing.

pub mod bus;
pub mod device;

pub use bus::{MappedTarget, RegionInfo, SystemBus};
pub use device::BusDevice;
pub use mel_core::{Address, BusError, Endianness, MelError, MelResult, Size};
