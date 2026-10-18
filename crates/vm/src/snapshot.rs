//! Machine state snapshotting and restoration.

use crate::clock::VirtualClock;
use mel_core::{Architecture, MelError, MelResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Magic identifier header for MEL snapshot files (`b"MELS"`).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"MELS";

/// Current snapshot format version (Version 2 with multi-arch and VirtualClock support).
pub const SNAPSHOT_VERSION: u32 = 2;

/// Complete serializable machine state snapshot supporting all CPU architectures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Format identifier header.
    pub magic: [u8; 4],
    /// Snapshot schema format version.
    pub version: u32,
    /// Target CPU architecture.
    pub arch: Architecture,
    /// Base address of RAM in guest address space.
    pub ram_base: u64,
    /// Serialized CPU backend state (registers, PC, cycles).
    pub cpu_state: Vec<u8>,
    /// Raw RAM bytes.
    pub ram_data: Vec<u8>,
    /// Serialized primary UART state, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uart_state: Option<Vec<u8>>,
    /// Serialized Boot ROM state, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rom_state: Option<Vec<u8>>,
    /// Serialized auxiliary MMIO device states indexed by device name.
    #[serde(default)]
    pub devices: HashMap<String, Vec<u8>>,
    /// Virtual clock state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<VirtualClock>,
}

impl Snapshot {
    /// Construct a new machine snapshot.
    pub fn new(
        arch: Architecture,
        ram_base: u64,
        cpu_state: Vec<u8>,
        ram_data: Vec<u8>,
        uart_state: Option<Vec<u8>>,
        rom_state: Option<Vec<u8>>,
        devices: HashMap<String, Vec<u8>>,
    ) -> Self {
        Self {
            magic: SNAPSHOT_MAGIC,
            version: SNAPSHOT_VERSION,
            arch,
            ram_base,
            cpu_state,
            ram_data,
            uart_state,
            rom_state,
            devices,
            clock: None,
        }
    }

    /// Attach virtual clock state to the snapshot.
    pub fn with_clock(mut self, clock: VirtualClock) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Serialize snapshot to binary JSON byte vector.
    pub fn to_bytes(&self) -> MelResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| MelError::Config(format!("Failed to serialize snapshot: {e}")))
    }

    /// Deserialize snapshot from a byte slice.
    pub fn from_bytes(data: &[u8]) -> MelResult<Self> {
        let snapshot: Self = serde_json::from_slice(data)
            .map_err(|e| MelError::Config(format!("Failed to parse snapshot: {e}")))?;

        if snapshot.magic != SNAPSHOT_MAGIC {
            return Err(MelError::Config(format!(
                "Invalid snapshot magic header: expected {:?}, got {:?}",
                SNAPSHOT_MAGIC, snapshot.magic
            )));
        }

        if snapshot.version > SNAPSHOT_VERSION {
            return Err(MelError::Config(format!(
                "Unsupported snapshot version {} (maximum supported: {})",
                snapshot.version, SNAPSHOT_VERSION
            )));
        }

        Ok(snapshot)
    }

    /// Save snapshot to a file path on the host filesystem.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> MelResult<()> {
        let bytes = self.to_bytes()?;
        let mut file = File::create(path.as_ref())?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }

    /// Load snapshot from a file path on the host filesystem.
    pub fn load_from_file(path: impl AsRef<Path>) -> MelResult<Self> {
        let mut file = File::open(path.as_ref())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_serialization_roundtrip_v2() {
        let mut clock = VirtualClock::new(10_000_000).unwrap();
        clock.advance(500);

        let snapshot = Snapshot::new(
            Architecture::Arm64,
            0x4000_0000,
            vec![1, 2, 3, 4],
            vec![0xBB; 256],
            Some(vec![7, 8]),
            None,
            HashMap::new(),
        )
        .with_clock(clock.clone());

        let bytes = snapshot.to_bytes().unwrap();
        let restored = Snapshot::from_bytes(&bytes).unwrap();

        assert_eq!(restored.magic, SNAPSHOT_MAGIC);
        assert_eq!(restored.version, SNAPSHOT_VERSION);
        assert_eq!(restored.arch, Architecture::Arm64);
        assert_eq!(restored.ram_base, 0x4000_0000);
        assert_eq!(restored.cpu_state, vec![1, 2, 3, 4]);
        assert_eq!(restored.ram_data, vec![0xBB; 256]);
        assert_eq!(restored.uart_state, Some(vec![7, 8]));
        assert_eq!(restored.rom_state, None);
        assert_eq!(restored.clock, Some(clock));
    }

    #[test]
    fn test_snapshot_backward_compatibility_v1() {
        // v1 json without clock field and with version = 1
        let v1_json = r#"{
            "magic": [77, 69, 76, 83],
            "version": 1,
            "arch": "riscv64",
            "ram_base": 2147483648,
            "cpu_state": [1, 2, 3],
            "ram_data": [4, 5, 6],
            "uart_state": null,
            "rom_state": null,
            "devices": {}
        }"#;

        let restored = Snapshot::from_bytes(v1_json.as_bytes()).unwrap();
        assert_eq!(restored.version, 1);
        assert_eq!(restored.arch, Architecture::Riscv64);
        assert_eq!(restored.clock, None);
    }
}
