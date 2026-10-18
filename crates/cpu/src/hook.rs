//! CPU-Bus memory and MMIO hook bridge.

use mel_bus::SystemBus;
use mel_core::{Endianness, MelError};
use std::sync::{Arc, Mutex};
use unicorn_engine::Unicorn;
use unicorn_engine::unicorn_const::{HookType, Prot};

/// Context managing CPU-to-Bus routing and hook registrations.
#[derive(Clone)]
pub struct CpuBusBridge {
    bus: Arc<Mutex<SystemBus>>,
}

impl CpuBusBridge {
    pub fn new(bus: Arc<Mutex<SystemBus>>) -> Self {
        Self { bus }
    }

    pub fn bus(&self) -> &Arc<Mutex<SystemBus>> {
        &self.bus
    }

    pub fn register<'a, D: 'static>(&self, uc: &mut Unicorn<'a, D>) -> Result<(), MelError> {
        register_bus_hooks(uc, self.bus.clone())
    }
}

/// Connects a `SystemBus` to a Unicorn instance via memory hooks.
pub fn register_bus_hooks<'a, D: 'static>(
    uc: &mut Unicorn<'a, D>,
    bus: Arc<Mutex<SystemBus>>,
) -> Result<(), MelError> {
    // 1. Unmapped hooks: on-demand map 4KB pages into Unicorn
    uc.add_mem_hook(
        HookType::MEM_WRITE_UNMAPPED | HookType::MEM_READ_UNMAPPED,
        0,
        u64::MAX,
        move |uc, _mem_type, addr, _size, _val| -> bool {
            let page_base = addr & !0xFFF;
            uc.mem_map(page_base, 0x1000, Prot::ALL).is_ok()
        },
    )
    .map_err(|e| MelError::Cpu(mel_core::CpuError::Internal(e.to_string())))?;

    // 2. Mapped write hook: dispatches all memory writes to SystemBus
    let bus_write = bus.clone();
    uc.add_mem_hook(
        HookType::MEM_WRITE,
        0,
        u64::MAX,
        move |_uc, _mem_type, addr, size, val| -> bool {
            if let Ok(mut b) = bus_write.lock() {
                let _ = b.write(addr, val as u64, size, Endianness::Little);
            }
            true
        },
    )
    .map_err(|e| MelError::Cpu(mel_core::CpuError::Internal(e.to_string())))?;

    // 3. Mapped read hook: refreshes device memory from SystemBus before CPU loads it
    let bus_read = bus;
    uc.add_mem_hook(
        HookType::MEM_READ,
        0,
        u64::MAX,
        move |uc, _mem_type, addr, size, _val| -> bool {
            let b = match bus_read.lock() {
                Ok(guard) => guard,
                Err(_) => return true,
            };
            if let Ok(val) = b.read(addr, size, Endianness::Little) {
                let bytes = match size {
                    1 => (val as u8).to_le_bytes().to_vec(),
                    2 => (val as u16).to_le_bytes().to_vec(),
                    4 => (val as u32).to_le_bytes().to_vec(),
                    8 => val.to_le_bytes().to_vec(),
                    _ => return true,
                };
                let _ = uc.mem_write(addr, &bytes);
            }
            true
        },
    )
    .map_err(|e| MelError::Cpu(mel_core::CpuError::Internal(e.to_string())))?;

    Ok(())
}
