//! # MEL Devices
//!
//! MMIO peripheral devices: UART 16550, CLINT timer, PLIC, Block Device, Boot ROM.

pub mod device_trait;
pub mod dummy;
pub mod uart16550;

pub use device_trait::{MmioBusAdapter, MmioDevice, SharedMmioBusAdapter};
pub use dummy::DummyDevice;
pub use mel_core::{Address, MelError, MelResult};
pub use uart16550::{
    Uart16550, LCR_DLAB, LSR_DATA_READY, LSR_TEMT, LSR_THRE, UART_DLL, UART_DLM, UART_FCR,
    UART_IER, UART_IIR, UART_LCR, UART_LSR, UART_MCR, UART_MSR, UART_RBR, UART_SCR, UART_THR,
};
