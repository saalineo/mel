//! # MEL Debugger
//!
//! Interactive REPL, disassembly, breakpoints, memory inspection, and register inspection.

pub mod disasm;

pub use disasm::{create_disassembler, disassemble, DisassembledInstruction};
pub use mel_core::{Address, Architecture, MelError, MelResult};
