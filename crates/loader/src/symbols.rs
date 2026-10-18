//! ELF symbol table extraction and address/symbol resolution.

use mel_core::{Address, MelError};
use object::elf;
use object::read::elf::{ElfFile64, SectionHeader, Sym};
use object::Endianness as ObjEndianness;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;

/// ELF Symbol type / category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    NoType,
    Object,
    Function,
    Section,
    File,
    Common,
    Tls,
    Other(u8),
}

impl SymbolKind {
    /// Convert raw ELF `st_type` to `SymbolKind`.
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0xf {
            elf::STT_NOTYPE => Self::NoType,
            elf::STT_OBJECT => Self::Object,
            elf::STT_FUNC => Self::Function,
            elf::STT_SECTION => Self::Section,
            elf::STT_FILE => Self::File,
            elf::STT_COMMON => Self::Common,
            elf::STT_TLS => Self::Tls,
            other => Self::Other(other),
        }
    }

    /// Return raw `st_type` value.
    pub const fn raw(&self) -> u8 {
        match self {
            Self::NoType => elf::STT_NOTYPE,
            Self::Object => elf::STT_OBJECT,
            Self::Function => elf::STT_FUNC,
            Self::Section => elf::STT_SECTION,
            Self::File => elf::STT_FILE,
            Self::Common => elf::STT_COMMON,
            Self::Tls => elf::STT_TLS,
            Self::Other(val) => *val,
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoType => write!(f, "NOTYPE"),
            Self::Object => write!(f, "OBJECT"),
            Self::Function => write!(f, "FUNC"),
            Self::Section => write!(f, "SECTION"),
            Self::File => write!(f, "FILE"),
            Self::Common => write!(f, "COMMON"),
            Self::Tls => write!(f, "TLS"),
            Self::Other(val) => write!(f, "OTHER({:#x})", val),
        }
    }
}

/// ELF Symbol binding / visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolBinding {
    Local,
    Global,
    Weak,
    Other(u8),
}

impl SymbolBinding {
    /// Convert raw ELF `st_bind` to `SymbolBinding`.
    pub const fn from_raw(raw: u8) -> Self {
        match raw >> 4 {
            elf::STB_LOCAL => Self::Local,
            elf::STB_GLOBAL => Self::Global,
            elf::STB_WEAK => Self::Weak,
            other => Self::Other(other),
        }
    }

    /// Return raw `st_bind` value.
    pub const fn raw(&self) -> u8 {
        match self {
            Self::Local => elf::STB_LOCAL,
            Self::Global => elf::STB_GLOBAL,
            Self::Weak => elf::STB_WEAK,
            Self::Other(val) => *val,
        }
    }
}

impl fmt::Display for SymbolBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "LOCAL"),
            Self::Global => write!(f, "GLOBAL"),
            Self::Weak => write!(f, "WEAK"),
            Self::Other(val) => write!(f, "OTHER({:#x})", val),
        }
    }
}

/// Extracted ELF symbol entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfSymbol {
    pub name: String,
    pub address: Address,
    pub size: u64,
    pub kind: SymbolKind,
    pub binding: SymbolBinding,
    pub section_index: Option<u16>,
}

impl ElfSymbol {
    /// Returns true if symbol represents a function / executable code.
    pub const fn is_function(&self) -> bool {
        matches!(self.kind, SymbolKind::Function)
    }

    /// Returns true if symbol represents a data object / variable.
    pub const fn is_variable(&self) -> bool {
        matches!(self.kind, SymbolKind::Object)
    }

    /// Returns true if symbol is globally visible.
    pub const fn is_global(&self) -> bool {
        matches!(self.binding, SymbolBinding::Global)
    }

    /// Check if this symbol range covers a given address.
    pub fn contains_address(&self, addr: Address) -> bool {
        if addr < self.address {
            return false;
        }
        if self.size == 0 {
            return addr == self.address;
        }
        addr < self.address.saturating_add(self.size)
    }
}

/// Symbol table providing bidirectional name <-> address resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolTable {
    symbols: Vec<ElfSymbol>,
    by_name: HashMap<String, usize>,
    by_addr: BTreeMap<Address, Vec<usize>>,
}

impl SymbolTable {
    /// Create an empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse symbol table from an ELF64 binary byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self, MelError> {
        if bytes.len() < 16 {
            return Err(MelError::Loader(
                "Binary is too small to contain an ELF header".into(),
            ));
        }

        if bytes[0..4] != elf::ELFMAG {
            return Err(MelError::Loader("Invalid ELF magic".into()));
        }

        let class = bytes[4];
        if class != elf::ELFCLASS64 {
            return Err(MelError::Loader(format!(
                "Unsupported ELF class {class}: requires 64-bit ELF"
            )));
        }

        let obj_endian = match bytes[5] {
            elf::ELFDATA2LSB => ObjEndianness::Little,
            elf::ELFDATA2MSB => ObjEndianness::Big,
            other => {
                return Err(MelError::Loader(format!(
                    "Invalid ELF data encoding: {other}"
                )))
            }
        };

        let elf = ElfFile64::<ObjEndianness>::parse(bytes)
            .map_err(|e| MelError::Loader(format!("Failed to parse ELF64: {e}")))?;

        let mut table = Self::new();

        // 1. Try static symbol table (.symtab)
        let sections = elf.elf_section_table();
        let mut symtab_section = None;

        for sh in sections.iter() {
            if sh.sh_type(obj_endian) == elf::SHT_SYMTAB {
                symtab_section = Some(sh);
                break;
            }
        }

        // 2. Fallback to dynamic symbol table (.dynsym) if .symtab is stripped
        if symtab_section.is_none() {
            for sh in sections.iter() {
                if sh.sh_type(obj_endian) == elf::SHT_DYNSYM {
                    symtab_section = Some(sh);
                    break;
                }
            }
        }

        if let Some(sh) = symtab_section {
            let strtab_index = sh.sh_link(obj_endian) as usize;
            let strtab_header = sections
                .section(object::SectionIndex(strtab_index))
                .map_err(|e| {
                    MelError::Loader(format!("Failed to read symbol string table: {e}"))
                })?;
            let strtab_bytes = strtab_header
                .data(obj_endian, bytes)
                .map_err(|e| MelError::Loader(format!("Failed to read string table data: {e}")))?;

            let sym_bytes = sh
                .data(obj_endian, bytes)
                .map_err(|e| MelError::Loader(format!("Failed to read symbol table data: {e}")))?;

            let entsize = std::mem::size_of::<elf::Sym64<ObjEndianness>>();
            if entsize > 0 && !sym_bytes.is_empty() {
                let count = sym_bytes.len() / entsize;
                for i in 0..count {
                    let offset = i * entsize;
                    let sym_slice = &sym_bytes[offset..offset + entsize];
                    // SAFETY: `sym_slice` is guaranteed to have at least `entsize` bytes,
                    // which is exactly `size_of::<elf::Sym64<ObjEndianness>>()`. We use
                    // `read_unaligned` to prevent undefined behavior from potentially unaligned section buffers.
                    let raw_sym: elf::Sym64<ObjEndianness> = unsafe {
                        std::ptr::read_unaligned(
                            sym_slice.as_ptr().cast::<elf::Sym64<ObjEndianness>>(),
                        )
                    };

                    let name_offset = raw_sym.st_name(obj_endian) as usize;
                    let name = if name_offset < strtab_bytes.len() {
                        let name_slice = &strtab_bytes[name_offset..];
                        let end = name_slice
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(name_slice.len());
                        String::from_utf8_lossy(&name_slice[..end]).to_string()
                    } else {
                        String::new()
                    };

                    if name.is_empty() {
                        continue;
                    }

                    let raw_info = raw_sym.st_info();
                    let kind = SymbolKind::from_raw(raw_info);
                    let binding = SymbolBinding::from_raw(raw_info);
                    let address = raw_sym.st_value(obj_endian);
                    let size = raw_sym.st_size(obj_endian);
                    let shndx = raw_sym.st_shndx(obj_endian);
                    let section_index = if shndx == elf::SHN_UNDEF {
                        None
                    } else {
                        Some(shndx)
                    };

                    table.add_symbol(ElfSymbol {
                        name,
                        address,
                        size,
                        kind,
                        binding,
                        section_index,
                    });
                }
            }
        }

        Ok(table)
    }

    /// Read and parse symbol table from an ELF file on disk.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, MelError> {
        let bytes = std::fs::read(path.as_ref())?;
        Self::parse(&bytes)
    }

    /// Add a symbol to the table, indexing it by name and address.
    pub fn add_symbol(&mut self, symbol: ElfSymbol) {
        let index = self.symbols.len();
        self.by_name.insert(symbol.name.clone(), index);
        self.by_addr.entry(symbol.address).or_default().push(index);
        self.symbols.push(symbol);
    }

    /// Total number of symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true if table contains no symbols.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Slice of all symbols.
    pub fn symbols(&self) -> &[ElfSymbol] {
        &self.symbols
    }

    /// Iterate over symbols.
    pub fn iter(&self) -> impl Iterator<Item = &ElfSymbol> {
        self.symbols.iter()
    }

    /// Look up symbol by exact name.
    pub fn find_by_name(&self, name: &str) -> Option<&ElfSymbol> {
        self.by_name
            .get(name)
            .and_then(|&idx| self.symbols.get(idx))
    }

    /// Look up symbol starting at exact address.
    pub fn lookup_address(&self, addr: Address) -> Option<&ElfSymbol> {
        self.by_addr.get(&addr).and_then(|indices| {
            // Prefer Function symbols if multiple symbols share an address
            indices
                .iter()
                .find_map(|&idx| {
                    let sym = &self.symbols[idx];
                    if sym.is_function() {
                        Some(sym)
                    } else {
                        None
                    }
                })
                .or_else(|| indices.first().map(|&idx| &self.symbols[idx]))
        })
    }

    /// Resolve an address to the containing symbol and offset.
    pub fn resolve_address(&self, addr: Address) -> Option<(&ElfSymbol, u64)> {
        // 1. Direct exact match
        if let Some(sym) = self.lookup_address(addr) {
            return Some((sym, 0));
        }

        // 2. Search preceding symbols in address order
        if let Some((&sym_addr, indices)) = self.by_addr.range(..addr).next_back() {
            for &idx in indices {
                let sym = &self.symbols[idx];
                if sym.size > 0 {
                    if addr < sym_addr.saturating_add(sym.size) {
                        let offset = addr - sym_addr;
                        return Some((sym, offset));
                    }
                } else if sym.is_function() {
                    let offset = addr - sym_addr;
                    if offset <= 4096 {
                        return Some((sym, offset));
                    }
                }
            }
        }

        None
    }

    /// Format an address with symbolic notation (e.g. `main+0x4` or `0x80000040`).
    pub fn format_address(&self, addr: Address) -> String {
        if let Some((sym, offset)) = self.resolve_address(addr) {
            if offset == 0 {
                sym.name.clone()
            } else {
                format!("{}+{:#x}", sym.name, offset)
            }
        } else {
            format!("{:#x}", addr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestElfWithSymbolsBuilder {
        machine: u16,
        is_big_endian: bool,
        symbols: Vec<(String, u64, u64, u8, u8)>, // name, addr, size, type, bind
    }

    impl TestElfWithSymbolsBuilder {
        fn new(machine: u16, is_big_endian: bool) -> Self {
            Self {
                machine,
                is_big_endian,
                symbols: Vec::new(),
            }
        }

        fn add_symbol(
            mut self,
            name: &str,
            addr: u64,
            size: u64,
            kind: SymbolKind,
            binding: SymbolBinding,
        ) -> Self {
            let st_info = (binding.raw() << 4) | (kind.raw() & 0xf);
            self.symbols
                .push((name.to_string(), addr, size, st_info, 0));
            self
        }

        fn build(&self) -> Vec<u8> {
            let mut buf = Vec::new();

            let write_u16 = |buf: &mut Vec<u8>, val: u16| {
                if self.is_big_endian {
                    buf.extend_from_slice(&val.to_be_bytes());
                } else {
                    buf.extend_from_slice(&val.to_le_bytes());
                }
            };

            let write_u32 = |buf: &mut Vec<u8>, val: u32| {
                if self.is_big_endian {
                    buf.extend_from_slice(&val.to_be_bytes());
                } else {
                    buf.extend_from_slice(&val.to_le_bytes());
                }
            };

            let write_u64 = |buf: &mut Vec<u8>, val: u64| {
                if self.is_big_endian {
                    buf.extend_from_slice(&val.to_be_bytes());
                } else {
                    buf.extend_from_slice(&val.to_le_bytes());
                }
            };

            // Layout:
            // 0x0000: ELF Header (64 bytes)
            // 0x0100: .strtab data (symbol string table)
            // 0x0200: .symtab data (ELF symbol table, 24 bytes per entry)
            // 0x0400: .shstrtab data (section string table)
            // 0x0600: Section Headers (4 * 64 bytes = 256 bytes)

            let strtab_offset = 0x100u64;
            let mut strtab = Vec::new();
            strtab.push(0);

            let mut sym_data = Vec::new();
            // Symbol 0: NULL symbol (24 bytes)
            sym_data.extend_from_slice(&[0u8; 24]);

            for (name, addr, size, st_info, _) in &self.symbols {
                let name_offset = strtab.len() as u32;
                strtab.extend_from_slice(name.as_bytes());
                strtab.push(0);

                // Elf64_Sym (24 bytes):
                // st_name: u32 (4 bytes)
                // st_info: u8  (1 byte)
                // st_other: u8 (1 byte)
                // st_shndx: u16 (2 bytes)
                // st_value: u64 (8 bytes)
                // st_size: u64 (8 bytes)
                if self.is_big_endian {
                    sym_data.extend_from_slice(&name_offset.to_be_bytes());
                    sym_data.push(*st_info);
                    sym_data.push(0);
                    sym_data.extend_from_slice(&1u16.to_be_bytes()); // shndx = 1 (.text)
                    sym_data.extend_from_slice(&addr.to_be_bytes());
                    sym_data.extend_from_slice(&size.to_be_bytes());
                } else {
                    sym_data.extend_from_slice(&name_offset.to_le_bytes());
                    sym_data.push(*st_info);
                    sym_data.push(0);
                    sym_data.extend_from_slice(&1u16.to_le_bytes()); // shndx = 1
                    sym_data.extend_from_slice(&addr.to_le_bytes());
                    sym_data.extend_from_slice(&size.to_le_bytes());
                }
            }

            let symtab_offset = 0x200u64;
            let shstrtab_offset = 0x400u64;

            let mut shstrtab = Vec::new();
            shstrtab.push(0);
            let symtab_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".symtab\0");
            let strtab_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".strtab\0");
            let shstrtab_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".shstrtab\0");

            let sh_offset = 0x600u64;
            let sh_count = 4u16;
            let shstrndx = 3u16;

            // 1. ELF Header
            buf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
            buf.push(2); // ELFCLASS64
            buf.push(if self.is_big_endian { 2 } else { 1 });
            buf.push(1); // EV_CURRENT
            buf.push(0);
            buf.extend_from_slice(&[0u8; 8]);

            write_u16(&mut buf, elf::ET_EXEC);
            write_u16(&mut buf, self.machine);
            write_u32(&mut buf, 1);
            write_u64(&mut buf, 0x8000_0000);
            write_u64(&mut buf, 0); // no program headers
            write_u64(&mut buf, sh_offset);
            write_u32(&mut buf, 0);
            write_u16(&mut buf, 64);
            write_u16(&mut buf, 0);
            write_u16(&mut buf, 0);
            write_u16(&mut buf, 64);
            write_u16(&mut buf, sh_count);
            write_u16(&mut buf, shstrndx);

            // Pad to strtab_offset
            if buf.len() < strtab_offset as usize {
                buf.resize(strtab_offset as usize, 0);
            }
            buf.extend_from_slice(&strtab);

            // Pad to symtab_offset
            if buf.len() < symtab_offset as usize {
                buf.resize(symtab_offset as usize, 0);
            }
            buf.extend_from_slice(&sym_data);

            // Pad to shstrtab_offset
            if buf.len() < shstrtab_offset as usize {
                buf.resize(shstrtab_offset as usize, 0);
            }
            buf.extend_from_slice(&shstrtab);

            // Pad to sh_offset
            if buf.len() < sh_offset as usize {
                buf.resize(sh_offset as usize, 0);
            }

            // Section 0: NULL
            for _ in 0..8 {
                write_u64(&mut buf, 0);
            }

            // Section 1: .symtab (sh_link = 2 -> .strtab)
            write_u32(&mut buf, symtab_name_off);
            write_u32(&mut buf, elf::SHT_SYMTAB);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, symtab_offset);
            write_u64(&mut buf, sym_data.len() as u64);
            write_u32(&mut buf, 2); // link to section 2 (.strtab)
            write_u32(&mut buf, 1); // info
            write_u64(&mut buf, 8);
            write_u64(&mut buf, 24); // entsize

            // Section 2: .strtab
            write_u32(&mut buf, strtab_name_off);
            write_u32(&mut buf, elf::SHT_STRTAB);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, strtab_offset);
            write_u64(&mut buf, strtab.len() as u64);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u64(&mut buf, 1);
            write_u64(&mut buf, 0);

            // Section 3: .shstrtab
            write_u32(&mut buf, shstrtab_name_off);
            write_u32(&mut buf, elf::SHT_STRTAB);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, 0);
            write_u64(&mut buf, shstrtab_offset);
            write_u64(&mut buf, shstrtab.len() as u64);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u64(&mut buf, 1);
            write_u64(&mut buf, 0);

            buf
        }
    }

    #[test]
    fn test_symbol_table_resolution() {
        let elf_bytes = TestElfWithSymbolsBuilder::new(elf::EM_RISCV, false)
            .add_symbol(
                "main",
                0x8000_0040,
                0x30,
                SymbolKind::Function,
                SymbolBinding::Global,
            )
            .add_symbol(
                "uart_puts",
                0x8000_0100,
                0x50,
                SymbolKind::Function,
                SymbolBinding::Global,
            )
            .add_symbol(
                "g_buffer",
                0x8000_2000,
                0x100,
                SymbolKind::Object,
                SymbolBinding::Global,
            )
            .build();

        let symtab = SymbolTable::parse(&elf_bytes).unwrap();
        assert_eq!(symtab.len(), 3);

        // 1. Name lookup
        let main_sym = symtab.find_by_name("main").unwrap();
        assert_eq!(main_sym.address, 0x8000_0040);
        assert_eq!(main_sym.size, 0x30);
        assert!(main_sym.is_function());
        assert!(main_sym.is_global());

        let uart_sym = symtab.find_by_name("uart_puts").unwrap();
        assert_eq!(uart_sym.address, 0x8000_0100);

        let buf_sym = symtab.find_by_name("g_buffer").unwrap();
        assert_eq!(buf_sym.address, 0x8000_2000);
        assert!(buf_sym.is_variable());

        // 2. Exact address lookup
        assert_eq!(symtab.lookup_address(0x8000_0040).unwrap().name, "main");
        assert_eq!(
            symtab.lookup_address(0x8000_0100).unwrap().name,
            "uart_puts"
        );
        assert!(symtab.lookup_address(0x8000_0050).is_none());

        // 3. Address resolution with offset
        let (resolved, offset) = symtab.resolve_address(0x8000_0044).unwrap();
        assert_eq!(resolved.name, "main");
        assert_eq!(offset, 0x4);

        let (resolved_uart, offset_uart) = symtab.resolve_address(0x8000_0120).unwrap();
        assert_eq!(resolved_uart.name, "uart_puts");
        assert_eq!(offset_uart, 0x20);

        // 4. Formatted address string
        assert_eq!(symtab.format_address(0x8000_0040), "main");
        assert_eq!(symtab.format_address(0x8000_0044), "main+0x4");
        assert_eq!(symtab.format_address(0x8000_0100), "uart_puts");
        assert_eq!(symtab.format_address(0x8000_0108), "uart_puts+0x8");
        assert_eq!(symtab.format_address(0x9000_0000), "0x90000000");
    }

    #[test]
    fn test_symbol_table_big_endian() {
        let elf_bytes = TestElfWithSymbolsBuilder::new(elf::EM_PPC64, true)
            .add_symbol(
                "_start",
                0x0000_0100,
                0x20,
                SymbolKind::Function,
                SymbolBinding::Global,
            )
            .build();

        let symtab = SymbolTable::parse(&elf_bytes).unwrap();
        assert_eq!(symtab.len(), 1);

        let start_sym = symtab.find_by_name("_start").unwrap();
        assert_eq!(start_sym.address, 0x0000_0100);
        assert_eq!(symtab.format_address(0x0000_010c), "_start+0xc");
    }
}
