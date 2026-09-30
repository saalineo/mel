//! # MEL Debugger
//!
//! Interactive REPL, disassembly, breakpoints, memory inspection, and register inspection.

pub mod breakpoints;
pub mod debugger;
pub mod disasm;

pub use breakpoints::{
    Breakpoint, BreakpointCondition, BreakpointHit, BreakpointManager, DebugStopReason, Watchpoint,
    WatchpointKind,
};
pub use debugger::Debugger;
pub use disasm::{create_disassembler, disassemble, DisassembledInstruction};
pub use mel_core::{Address, Architecture, MelError, MelResult, RegisterId};
