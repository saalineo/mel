//! 16550-compatible UART (Universal Asynchronous Receiver-Transmitter) peripheral.

use std::collections::VecDeque;
use mel_core::MelError;
use crate::device_trait::MmioDevice;
use serde::{Deserialize, Serialize};

pub const UART_RBR: u64 = 0x00;
pub const UART_THR: u64 = 0x00;
pub const UART_DLL: u64 = 0x00;
pub const UART_IER: u64 = 0x01;
pub const UART_DLM: u64 = 0x01;
pub const UART_IIR: u64 = 0x02;
pub const UART_FCR: u64 = 0x02;
pub const UART_LCR: u64 = 0x03;
pub const UART_MCR: u64 = 0x04;
pub const UART_LSR: u64 = 0x05;
pub const UART_MSR: u64 = 0x06;
pub const UART_SCR: u64 = 0x07;

pub const LSR_DATA_READY: u8 = 0x01;
pub const LSR_THRE: u8 = 0x20;
pub const LSR_TEMT: u8 = 0x40;

pub const LCR_DLAB: u8 = 0x80;

/// 16550-compatible Serial UART peripheral.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uart16550 {
    name: String,
    size: u64,
    ier: u8,
    lcr: u8,
    mcr: u8,
    scr: u8,
    dll: u8,
    dlm: u8,
    fcr: u8,
    rx_queue: VecDeque<u8>,
    tx_buffer: Vec<u8>,
    interrupt_line: bool,
}

impl Uart16550 {
    /// Create a new UART16550 peripheral with default 256-byte MMIO window.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 0x100,
            ier: 0x00,
            lcr: 0x03,
            mcr: 0x00,
            scr: 0x00,
            dll: 0x01,
            dlm: 0x00,
            fcr: 0x00,
            rx_queue: VecDeque::new(),
            tx_buffer: Vec::new(),
            interrupt_line: false,
        }
    }

    /// Feed input bytes into the UART receiver FIFO.
    pub fn push_input(&mut self, byte: u8) {
        self.rx_queue.push_back(byte);
        self.update_interrupts();
    }

    /// Feed multiple input bytes into the UART receiver FIFO.
    pub fn push_input_bytes(&mut self, bytes: &[u8]) {
        self.rx_queue.extend(bytes.iter().copied());
        self.update_interrupts();
    }

    /// Read and drain all bytes written to the transmitter.
    pub fn drain_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx_buffer)
    }

    /// Peek at transmitter output without draining.
    pub fn peek_output(&self) -> &[u8] {
        &self.tx_buffer
    }

    /// Returns whether the UART interrupt line is currently active.
    pub fn is_interrupting(&self) -> bool {
        self.interrupt_line
    }

    fn update_interrupts(&mut self) {
        let rx_ready = !self.rx_queue.is_empty() && (self.ier & 0x01) != 0;
        let tx_ready = (self.ier & 0x02) != 0;
        self.interrupt_line = rx_ready || tx_ready;
    }
}

impl MmioDevice for Uart16550 {
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn read(&mut self, offset: u64, _size: usize) -> Result<u64, MelError> {
        let dlab = (self.lcr & LCR_DLAB) != 0;
        let val = match offset {
            UART_RBR => {
                if dlab {
                    self.dll
                } else {
                    let byte = self.rx_queue.pop_front().unwrap_or(0);
                    self.update_interrupts();
                    byte
                }
            }
            UART_IER => {
                if dlab {
                    self.dlm
                } else {
                    self.ier
                }
            }
            UART_IIR => {
                if !self.rx_queue.is_empty() && (self.ier & 0x01) != 0 {
                    0x04
                } else if (self.ier & 0x02) != 0 {
                    0x02
                } else {
                    0x01
                }
            }
            UART_LCR => self.lcr,
            UART_MCR => self.mcr,
            UART_LSR => {
                let mut lsr = LSR_THRE | LSR_TEMT;
                if !self.rx_queue.is_empty() {
                    lsr |= LSR_DATA_READY;
                }
                lsr
            }
            UART_MSR => 0x00,
            UART_SCR => self.scr,
            _ => 0x00,
        };
        Ok(u64::from(val))
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), MelError> {
        let val = value as u8;
        let dlab = (self.lcr & LCR_DLAB) != 0;

        match offset {
            UART_THR => {
                if dlab {
                    self.dll = val;
                } else {
                    self.tx_buffer.push(val);
                }
            }
            UART_IER => {
                if dlab {
                    self.dlm = val;
                } else {
                    self.ier = val;
                    self.update_interrupts();
                }
            }
            UART_FCR => {
                self.fcr = val;
                if (val & 0x02) != 0 {
                    self.rx_queue.clear();
                }
            }
            UART_LCR => self.lcr = val,
            UART_MCR => self.mcr = val,
            UART_SCR => self.scr = val,
            _ => {}
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), MelError> {
        self.ier = 0x00;
        self.lcr = 0x03;
        self.mcr = 0x00;
        self.scr = 0x00;
        self.dll = 0x01;
        self.dlm = 0x00;
        self.fcr = 0x00;
        self.rx_queue.clear();
        self.tx_buffer.clear();
        self.interrupt_line = false;
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, MelError> {
        serde_json::to_vec(self).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), MelError> {
        let restored: Self = serde_json::from_slice(data).map_err(|e| MelError::Device {
            device: self.name.clone(),
            message: e.to_string(),
        })?;
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_tx_string() {
        let mut uart = Uart16550::new("uart0");
        let msg = b"Hello MEL!";

        for &byte in msg {
            let lsr = uart.read(UART_LSR, 1).unwrap();
            assert!((lsr & (LSR_THRE as u64)) != 0);
            uart.write(UART_THR, byte as u64, 1).unwrap();
        }

        assert_eq!(uart.peek_output(), b"Hello MEL!");
        let drained = uart.drain_output();
        assert_eq!(drained, b"Hello MEL!");
        assert!(uart.peek_output().is_empty());
    }

    #[test]
    fn test_uart_rx_fifo() {
        let mut uart = Uart16550::new("uart0");

        let lsr = uart.read(UART_LSR, 1).unwrap();
        assert_eq!(lsr & (LSR_DATA_READY as u64), 0);

        uart.push_input(b'A');

        let lsr = uart.read(UART_LSR, 1).unwrap();
        assert_ne!(lsr & (LSR_DATA_READY as u64), 0);

        let ch = uart.read(UART_RBR, 1).unwrap() as u8;
        assert_eq!(ch, b'A');

        let lsr = uart.read(UART_LSR, 1).unwrap();
        assert_eq!(lsr & (LSR_DATA_READY as u64), 0);
    }

    #[test]
    fn test_uart_dlab_divisor() {
        let mut uart = Uart16550::new("uart0");

        uart.write(UART_LCR, LCR_DLAB as u64, 1).unwrap();
        uart.write(UART_DLL, 0x0C, 1).unwrap();
        uart.write(UART_DLM, 0x00, 1).unwrap();

        assert_eq!(uart.read(UART_DLL, 1).unwrap(), 0x0C);
        assert_eq!(uart.read(UART_DLM, 1).unwrap(), 0x00);

        uart.write(UART_LCR, 0x03, 1).unwrap();
        assert_eq!(uart.read(UART_LCR, 1).unwrap(), 0x03);
    }
}
