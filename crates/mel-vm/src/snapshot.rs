//! Machine state snapshotting and restoration.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use mel_core::{Architecture, MelError, MelResult};
use serde::{Deserialize, Serialize};

/// Magic identifier header for MEL snapshot files (`b"MELS"`).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"MELS";

/// Current snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 1;

/// Complete serializable machine state snapshot.
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
    pub uart_state: Option<Vec<u8>>,
    /// Serialized Boot ROM state, if present.
    pub rom_state: Option<Vec<u8>>,
    /// Serialized auxiliary MMIO device states indexed by device name.
    pub devices: HashMap<String, Vec<u8>>,
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
        }
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
    fn test_snapshot_serialization_roundtrip() {
        let snapshot = Snapshot::new(
            Architecture::Riscv64,
            0x8000_0000,
            vec![1, 2, 3, 4],
            vec![0xAA; 128],
            Some(vec![5, 6]),
            None,
            HashMap::new(),
        );

        let bytes = snapshot.to_bytes().unwrap();
        let restored = Snapshot::from_bytes(&bytes).unwrap();

        assert_eq!(restored.magic, SNAPSHOT_MAGIC);
        assert_eq!(restored.version, SNAPSHOT_VERSION);
        assert_eq!(restored.arch, Architecture::Riscv64);
        assert_eq!(restored.ram_base, 0x8000_0000);
        assert_eq!(restored.cpu_state, vec![1, 2, 3, 4]);
        assert_eq!(restored.ram_data, vec![0xAA; 128]);
        assert_eq!(restored.uart_state, Some(vec![5, 6]));
        assert_eq!(restored.rom_state, None);
    }
}
