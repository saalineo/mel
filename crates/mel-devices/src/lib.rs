//! # MEL Devices
//!
//! MMIO peripheral devices: UART 16550, CLINT timer, PLIC, Block Device, Boot ROM.

pub mod clint;
pub mod device_trait;
pub mod dummy;
pub mod plic;
pub mod rom;
pub mod uart16550;

pub use clint::{
    Clint, CLINT_BASE, CLINT_MSIP_BASE, CLINT_MTIME, CLINT_MTIMECMP_BASE, CLINT_SIZE,
};
pub use device_trait::{MmioBusAdapter, MmioDevice, SharedMmioBusAdapter};
pub use dummy::DummyDevice;
pub use mel_core::{Address, MelError, MelResult};
pub use plic::{
    Plic, BLOCK0_IRQ, PLIC_BASE, PLIC_CONTEXT_BASE, PLIC_CONTEXT_STRIDE, PLIC_DEFAULT_CONTEXTS,
    PLIC_DEFAULT_SOURCES, PLIC_ENABLE_BASE, PLIC_ENABLE_STRIDE, PLIC_MAX_SOURCES,
    PLIC_PENDING_BASE, PLIC_PRIORITY_BASE, PLIC_SIZE, UART0_IRQ,
};
pub use rom::BootRom;
pub use uart16550::{
    Uart16550, LCR_DLAB, LSR_DATA_READY, LSR_TEMT, LSR_THRE, UART_DLL, UART_DLM, UART_FCR,
    UART_IER, UART_IIR, UART_LCR, UART_LSR, UART_MCR, UART_MSR, UART_RBR, UART_SCR, UART_THR,
};

