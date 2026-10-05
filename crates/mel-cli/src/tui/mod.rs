//! Terminal User Interface (TUI) components for interactive debugging and simulation monitoring.

pub mod disasm;
pub mod layout;
pub mod memory;
pub mod registers;
pub mod terminal;

pub use disasm::{DisasmLine, DisasmState, DisasmWidget};
pub use layout::{render_layout_skeleton, TuiLayout};
pub use memory::{MemoryViewerState, MemoryViewerWidget, BYTES_PER_ROW};
pub use registers::{DecodedFlag, RegisterEntry, RegisterSnapshot, RegisterTracker, RegisterWidget};
pub use terminal::{key_event_to_uart_bytes, TerminalCell, TerminalState, TerminalWidget};
