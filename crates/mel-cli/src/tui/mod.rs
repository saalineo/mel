//! Terminal User Interface (TUI) components for interactive debugging and simulation monitoring.

pub mod layout;
pub mod registers;

pub use layout::{render_layout_skeleton, TuiLayout};
pub use registers::{DecodedFlag, RegisterEntry, RegisterSnapshot, RegisterTracker, RegisterWidget};
