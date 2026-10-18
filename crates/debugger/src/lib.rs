//! # MEL Debugger
//!
//! Interactive REPL, disassembly, breakpoints, memory inspection, and register inspection.

pub mod breakpoints;
pub mod debugger;
pub mod disasm;
pub mod mmu;
pub mod repl;
pub mod sv39;

pub use breakpoints::{
    Breakpoint, BreakpointCondition, BreakpointHit, BreakpointManager, DebugStopReason, Watchpoint,
    WatchpointKind,
};
pub use debugger::Debugger;
pub use disasm::{DisassembledInstruction, create_disassembler, disassemble};
pub use mel_core::{Address, Architecture, MelError, MelResult, RegisterId};
pub use mel_loader::{ElfSymbol, SymbolBinding, SymbolKind, SymbolTable};
pub use mmu::{MmuInspector, TranslationResult};
pub use repl::{ReplAction, ReplCommand, ReplOutput, ReplSession, format_hex_dump, parse_number};
pub use sv39::{
    PageSize, PageTableEntry, PteFlags, Satp, Sv39Fault, Sv39Translation, Sv39Validator,
};
