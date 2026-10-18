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
pub use disasm::{create_disassembler, disassemble, DisassembledInstruction};
pub use mel_core::{Address, Architecture, MelError, MelResult, RegisterId};
pub use mel_loader::{ElfSymbol, SymbolBinding, SymbolKind, SymbolTable};
pub use mmu::{MmuInspector, TranslationResult};
pub use repl::{format_hex_dump, parse_number, ReplAction, ReplCommand, ReplOutput, ReplSession};
pub use sv39::{
    PageSize, PageTableEntry, PteFlags, Satp, Sv39Fault, Sv39Translation, Sv39Validator,
};
