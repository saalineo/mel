//! # MEL Loader
//!
//! Binary and executable loaders (flat binary, ELF64).

pub mod elf;
pub mod raw;
pub mod symbols;

pub use elf::{
    ElfInfo, ElfLoadSummary, ElfProgramHeader, ElfSectionHeader, ElfType, SectionType,
    SegmentFlags, SegmentType, load_elf, load_elf_from_file, parse_elf, parse_elf_file,
};
pub use mel_core::{Address, MelError, MelResult};
pub use raw::{load_raw_binary, load_raw_binary_from_file};
pub use symbols::{ElfSymbol, SymbolBinding, SymbolKind, SymbolTable};
