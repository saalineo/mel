//! Terminal User Interface (TUI) components for interactive debugging and simulation monitoring.

pub mod app;
pub mod disasm;
pub mod layout;
pub mod memory;
pub mod registers;
pub mod terminal;

pub use app::{App, FocusedPane};
pub use disasm::{DisasmLine, DisasmState, DisasmWidget};
pub use layout::{TuiLayout, render_layout_skeleton};
pub use memory::{BYTES_PER_ROW, MemoryViewerState, MemoryViewerWidget};
pub use registers::{
    DecodedFlag, RegisterEntry, RegisterSnapshot, RegisterTracker, RegisterWidget,
};
pub use terminal::{TerminalCell, TerminalState, TerminalWidget, key_event_to_uart_bytes};
