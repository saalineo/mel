//! ELF64 binary header parser and metadata extraction.

use mel_core::{Address, Architecture, Endianness, MelError, Permissions};
use object::elf;
use object::read::elf::{ElfFile64, FileHeader, ProgramHeader, SectionHeader};
use object::Endianness as ObjEndianness;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Range;
use std::path::Path;

/// High-level ELF binary type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElfType {
    None,
    Relocatable,
    Executable,
    SharedObject,
    Core,
    Other(u16),
}

impl ElfType {
    /// Convert raw ELF `e_type` to `ElfType`.
    pub const fn from_raw(raw: u16) -> Self {
        match raw {
            elf::ET_NONE => Self::None,
            elf::ET_REL => Self::Relocatable,
            elf::ET_EXEC => Self::Executable,
            elf::ET_DYN => Self::SharedObject,
            elf::ET_CORE => Self::Core,
            other => Self::Other(other),
        }
    }

    /// Return raw `e_type` value.
    pub const fn raw(&self) -> u16 {
        match self {
            Self::None => elf::ET_NONE,
            Self::Relocatable => elf::ET_REL,
            Self::Executable => elf::ET_EXEC,
            Self::SharedObject => elf::ET_DYN,
            Self::Core => elf::ET_CORE,
            Self::Other(val) => *val,
        }
    }
}

impl fmt::Display for ElfType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "ET_NONE (No file type)"),
            Self::Relocatable => write!(f, "ET_REL (Relocatable object)"),
            Self::Executable => write!(f, "ET_EXEC (Executable file)"),
            Self::SharedObject => write!(f, "ET_DYN (Shared object / PIE)"),
            Self::Core => write!(f, "ET_CORE (Core file)"),
            Self::Other(val) => write!(f, "ET_OTHER ({:#x})", val),
        }
    }
}

/// ELF segment / program header type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SegmentType {
    Null,
    Load,
    Dynamic,
    Interp,
    Note,
    Shlib,
    Phdr,
    Tls,
    GnuEhFrame,
    GnuStack,
    GnuRelro,
    GnuProperty,
    Other(u32),
}

impl SegmentType {
    /// Convert raw `p_type` to `SegmentType`.
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            elf::PT_NULL => Self::Null,
            elf::PT_LOAD => Self::Load,
            elf::PT_DYNAMIC => Self::Dynamic,
            elf::PT_INTERP => Self::Interp,
            elf::PT_NOTE => Self::Note,
            elf::PT_SHLIB => Self::Shlib,
            elf::PT_PHDR => Self::Phdr,
            elf::PT_TLS => Self::Tls,
            elf::PT_GNU_EH_FRAME => Self::GnuEhFrame,
            elf::PT_GNU_STACK => Self::GnuStack,
            elf::PT_GNU_RELRO => Self::GnuRelro,
            elf::PT_GNU_PROPERTY => Self::GnuProperty,
            other => Self::Other(other),
        }
    }

    /// Return raw `p_type` value.
    pub const fn raw(&self) -> u32 {
        match self {
            Self::Null => elf::PT_NULL,
            Self::Load => elf::PT_LOAD,
            Self::Dynamic => elf::PT_DYNAMIC,
            Self::Interp => elf::PT_INTERP,
            Self::Note => elf::PT_NOTE,
            Self::Shlib => elf::PT_SHLIB,
            Self::Phdr => elf::PT_PHDR,
            Self::Tls => elf::PT_TLS,
            Self::GnuEhFrame => elf::PT_GNU_EH_FRAME,
            Self::GnuStack => elf::PT_GNU_STACK,
            Self::GnuRelro => elf::PT_GNU_RELRO,
            Self::GnuProperty => elf::PT_GNU_PROPERTY,
            Self::Other(val) => *val,
        }
    }
}

/// ELF segment flags (permissions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct SegmentFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub raw: u32,
}

impl SegmentFlags {
    /// Parse flags from raw `p_flags` bitmask.
    pub const fn from_raw(raw: u32) -> Self {
        Self {
            read: (raw & elf::PF_R) != 0,
            write: (raw & elf::PF_W) != 0,
            execute: (raw & elf::PF_X) != 0,
            raw,
        }
    }

    /// Convert to `mel_core::Permissions`.
    pub fn permissions(&self) -> Permissions {
        Permissions {
            read: self.read,
            write: self.write,
            execute: self.execute,
        }
    }
}

/// Parsed ELF program header (segment descriptor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfProgramHeader {
    pub segment_type: SegmentType,
    pub flags: SegmentFlags,
    pub offset: u64,
    pub virtual_addr: Address,
    pub physical_addr: Address,
    pub file_size: u64,
    pub mem_size: u64,
    pub align: u64,
}

impl ElfProgramHeader {
    /// Returns true if this segment is loadable (`PT_LOAD`).
    pub const fn is_load(&self) -> bool {
        matches!(self.segment_type, SegmentType::Load)
    }

    /// Returns true if segment is readable.
    pub const fn is_readable(&self) -> bool {
        self.flags.read
    }

    /// Returns true if segment is writable.
    pub const fn is_writable(&self) -> bool {
        self.flags.write
    }

    /// Returns true if segment is executable.
    pub const fn is_executable(&self) -> bool {
        self.flags.execute
    }

    /// Returns permissions struct.
    pub fn permissions(&self) -> Permissions {
        self.flags.permissions()
    }

    /// Returns byte size of uninitialized BSS region in memory.
    pub const fn bss_size(&self) -> u64 {
        self.mem_size.saturating_sub(self.file_size)
    }

    /// Returns byte range in file if within bounds.
    pub fn file_range(&self) -> Option<Range<usize>> {
        let start = usize::try_from(self.offset).ok()?;
        let size = usize::try_from(self.file_size).ok()?;
        let end = start.checked_add(size)?;
        Some(start..end)
    }

    /// Extract segment data slice from ELF buffer.
    pub fn data<'a>(&self, elf_bytes: &'a [u8]) -> Result<&'a [u8], MelError> {
        let range = self.file_range().ok_or_else(|| {
            MelError::Loader(format!(
                "Segment file range offset {:#x} size {:#x} overflows usize",
                self.offset, self.file_size
            ))
        })?;

        if range.end > elf_bytes.len() {
            return Err(MelError::Loader(format!(
                "Segment range {:#x}..{:#x} exceeds ELF file size {:#x}",
                range.start,
                range.end,
                elf_bytes.len()
            )));
        }

        Ok(&elf_bytes[range])
    }
}

/// ELF section type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SectionType {
    Null,
    ProgBits,
    SymTab,
    StrTab,
    Rela,
    Hash,
    Dynamic,
    Note,
    NoBits,
    Rel,
    Shlib,
    DynSym,
    InitArray,
    FiniArray,
    PreInitArray,
    Group,
    SymTabShndx,
    Other(u32),
}

impl SectionType {
    /// Convert raw `sh_type` to `SectionType`.
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            elf::SHT_NULL => Self::Null,
            elf::SHT_PROGBITS => Self::ProgBits,
            elf::SHT_SYMTAB => Self::SymTab,
            elf::SHT_STRTAB => Self::StrTab,
            elf::SHT_RELA => Self::Rela,
            elf::SHT_HASH => Self::Hash,
            elf::SHT_DYNAMIC => Self::Dynamic,
            elf::SHT_NOTE => Self::Note,
            elf::SHT_NOBITS => Self::NoBits,
            elf::SHT_REL => Self::Rel,
            elf::SHT_SHLIB => Self::Shlib,
            elf::SHT_DYNSYM => Self::DynSym,
            elf::SHT_INIT_ARRAY => Self::InitArray,
            elf::SHT_FINI_ARRAY => Self::FiniArray,
            elf::SHT_PREINIT_ARRAY => Self::PreInitArray,
            elf::SHT_GROUP => Self::Group,
            elf::SHT_SYMTAB_SHNDX => Self::SymTabShndx,
            other => Self::Other(other),
        }
    }

    /// Return raw `sh_type` value.
    pub const fn raw(&self) -> u32 {
        match self {
            Self::Null => elf::SHT_NULL,
            Self::ProgBits => elf::SHT_PROGBITS,
            Self::SymTab => elf::SHT_SYMTAB,
            Self::StrTab => elf::SHT_STRTAB,
            Self::Rela => elf::SHT_RELA,
            Self::Hash => elf::SHT_HASH,
            Self::Dynamic => elf::SHT_DYNAMIC,
            Self::Note => elf::SHT_NOTE,
            Self::NoBits => elf::SHT_NOBITS,
            Self::Rel => elf::SHT_REL,
            Self::Shlib => elf::SHT_SHLIB,
            Self::DynSym => elf::SHT_DYNSYM,
            Self::InitArray => elf::SHT_INIT_ARRAY,
            Self::FiniArray => elf::SHT_FINI_ARRAY,
            Self::PreInitArray => elf::SHT_PREINIT_ARRAY,
            Self::Group => elf::SHT_GROUP,
            Self::SymTabShndx => elf::SHT_SYMTAB_SHNDX,
            Self::Other(val) => *val,
        }
    }
}

/// Parsed ELF section header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfSectionHeader {
    pub name: String,
    pub section_type: SectionType,
    pub flags: u64,
    pub virtual_addr: Address,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub align: u64,
    pub entry_size: u64,
}

impl ElfSectionHeader {
    /// Returns true if section occupies memory during process execution (`SHF_ALLOC`).
    pub const fn is_allocated(&self) -> bool {
        (self.flags & (elf::SHF_ALLOC as u64)) != 0
    }

    /// Returns true if section is writable (`SHF_WRITE`).
    pub const fn is_writable(&self) -> bool {
        (self.flags & (elf::SHF_WRITE as u64)) != 0
    }

    /// Returns true if section contains executable instructions (`SHF_EXECINSTR`).
    pub const fn is_executable(&self) -> bool {
        (self.flags & (elf::SHF_EXECINSTR as u64)) != 0
    }

    /// Returns byte range in file if within bounds.
    pub fn file_range(&self) -> Option<Range<usize>> {
        if self.section_type == SectionType::NoBits {
            return None;
        }
        let start = usize::try_from(self.offset).ok()?;
        let size = usize::try_from(self.size).ok()?;
        let end = start.checked_add(size)?;
        Some(start..end)
    }

    /// Extract section data slice from ELF buffer.
    pub fn data<'a>(&self, elf_bytes: &'a [u8]) -> Result<&'a [u8], MelError> {
        if self.section_type == SectionType::NoBits {
            return Ok(&[]);
        }

        let range = self.file_range().ok_or_else(|| {
            MelError::Loader(format!(
                "Section '{}' file range offset {:#x} size {:#x} overflows usize",
                self.name, self.offset, self.size
            ))
        })?;

        if range.end > elf_bytes.len() {
            return Err(MelError::Loader(format!(
                "Section '{}' range {:#x}..{:#x} exceeds ELF file size {:#x}",
                self.name,
                range.start,
                range.end,
                elf_bytes.len()
            )));
        }

        Ok(&elf_bytes[range])
    }
}

/// Comprehensive ELF64 metadata extracted from binary headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfInfo {
    pub architecture: Architecture,
    pub endianness: Endianness,
    pub is_64bit: bool,
    pub elf_type: ElfType,
    pub entry_point: Address,
    pub program_headers: Vec<ElfProgramHeader>,
    pub section_headers: Vec<ElfSectionHeader>,
}

impl ElfInfo {
    /// Parse an ELF64 executable or object from a byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self, MelError> {
        if bytes.len() < 16 {
            return Err(MelError::Loader(
                "Binary is too small to contain an ELF header (minimum 16 bytes)".into(),
            ));
        }

        if bytes[0..4] != elf::ELFMAG {
            return Err(MelError::Loader(format!(
                "Invalid ELF magic: expected {:?}, found {:?}",
                elf::ELFMAG,
                &bytes[0..4]
            )));
        }

        let class = bytes[4];
        if class != elf::ELFCLASS64 {
            return Err(MelError::Loader(format!(
                "Unsupported ELF class {}: MEL requires 64-bit ELF (ELFCLASS64)",
                class
            )));
        }

        let data_encoding = bytes[5];
        let (endianness, obj_endian) = match data_encoding {
            elf::ELFDATA2LSB => (Endianness::Little, ObjEndianness::Little),
            elf::ELFDATA2MSB => (Endianness::Big, ObjEndianness::Big),
            other => {
                return Err(MelError::Loader(format!(
                    "Invalid ELF data encoding: {other}"
                )))
            }
        };

        let elf = ElfFile64::<ObjEndianness>::parse(bytes)
            .map_err(|e| MelError::Loader(format!("Failed to parse ELF64: {e}")))?;

        let raw_header = elf.elf_header();
        let e_machine = raw_header.e_machine(obj_endian);
        let architecture = match e_machine {
            elf::EM_RISCV => Architecture::Riscv64,
            elf::EM_AARCH64 => Architecture::Arm64,
            elf::EM_X86_64 => Architecture::X86_64,
            elf::EM_PPC64 => Architecture::PowerIso64,
            other => {
                return Err(MelError::Loader(format!(
                    "Unsupported ELF e_machine architecture: {other:#x} (supported: EM_RISCV, EM_AARCH64, EM_X86_64, EM_PPC64)"
                )))
            }
        };

        let elf_type = ElfType::from_raw(raw_header.e_type(obj_endian));
        let entry_point = raw_header.e_entry(obj_endian);

        let raw_segments = elf.elf_program_headers();
        let mut program_headers = Vec::with_capacity(raw_segments.len());
        for ph in raw_segments {
            program_headers.push(ElfProgramHeader {
                segment_type: SegmentType::from_raw(ph.p_type(obj_endian)),
                flags: SegmentFlags::from_raw(ph.p_flags(obj_endian)),
                offset: ph.p_offset(obj_endian),
                virtual_addr: ph.p_vaddr(obj_endian),
                physical_addr: ph.p_paddr(obj_endian),
                file_size: ph.p_filesz(obj_endian),
                mem_size: ph.p_memsz(obj_endian),
                align: ph.p_align(obj_endian),
            });
        }

        let section_table = elf.elf_section_table();
        let mut section_headers = Vec::with_capacity(section_table.len());

        for sh in section_table.iter() {
            let name_bytes = section_table.section_name(obj_endian, sh).unwrap_or(b"");
            let name = String::from_utf8_lossy(name_bytes).to_string();

            section_headers.push(ElfSectionHeader {
                name,
                section_type: SectionType::from_raw(sh.sh_type(obj_endian)),
                flags: sh.sh_flags(obj_endian),
                virtual_addr: sh.sh_addr(obj_endian),
                offset: sh.sh_offset(obj_endian),
                size: sh.sh_size(obj_endian),
                link: sh.sh_link(obj_endian),
                info: sh.sh_info(obj_endian),
                align: sh.sh_addralign(obj_endian),
                entry_size: sh.sh_entsize(obj_endian),
            });
        }

        Ok(ElfInfo {
            architecture,
            endianness,
            is_64bit: true,
            elf_type,
            entry_point,
            program_headers,
            section_headers,
        })
    }

    /// Read and parse an ELF file from the filesystem.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, MelError> {
        let bytes = std::fs::read(path.as_ref())?;
        Self::parse(&bytes)
    }

    /// Iterate over loadable segments (`PT_LOAD`).
    pub fn load_segments(&self) -> impl Iterator<Item = &ElfProgramHeader> {
        self.program_headers.iter().filter(|ph| ph.is_load())
    }

    /// Look up a section by name.
    pub fn find_section(&self, name: &str) -> Option<&ElfSectionHeader> {
        self.section_headers.iter().find(|sh| sh.name == name)
    }

    /// Return slice of program headers.
    pub fn segments(&self) -> &[ElfProgramHeader] {
        &self.program_headers
    }

    /// Return slice of section headers.
    pub fn sections(&self) -> &[ElfSectionHeader] {
        &self.section_headers
    }
}

/// Summary of ELF binary loading into virtual machine memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfLoadSummary {
    pub architecture: Architecture,
    pub entry_point: Address,
    pub segments_loaded: usize,
    pub total_bytes_loaded: u64,
    pub total_bss_zeroed: u64,
    pub lowest_addr: Address,
    pub highest_addr: Address,
}

/// Load an ELF64 binary into virtual machine memory, initialize BSS, and set the PC.
pub fn load_elf(vm: &mut mel_vm::VirtualMachine, elf_bytes: &[u8]) -> Result<ElfLoadSummary, MelError> {
    let info = ElfInfo::parse(elf_bytes)?;

    if info.architecture != vm.arch() {
        return Err(MelError::Loader(format!(
            "Architecture mismatch: ELF binary is {}, VM is {}",
            info.architecture,
            vm.arch()
        )));
    }

    let load_segments: Vec<_> = info.load_segments().collect();
    if load_segments.is_empty() {
        return Err(MelError::Loader(
            "ELF binary contains no loadable (PT_LOAD) segments".into(),
        ));
    }

    let mut segments_loaded = 0usize;
    let mut total_bytes_loaded = 0u64;
    let mut total_bss_zeroed = 0u64;
    let mut lowest_addr = u64::MAX;
    let mut highest_addr = 0u64;

    for ph in &load_segments {
        let target_addr = if ph.physical_addr != 0 {
            ph.physical_addr
        } else {
            ph.virtual_addr
        };

        if ph.file_size > 0 {
            let segment_bytes = ph.data(elf_bytes)?;
            vm.write_memory(target_addr, segment_bytes)?;
            total_bytes_loaded = total_bytes_loaded.checked_add(ph.file_size).ok_or_else(|| {
                MelError::Loader("Overflow in total loaded bytes calculation".into())
            })?;
        }

        let bss_size = ph.bss_size();
        if bss_size > 0 {
            let bss_addr = target_addr.checked_add(ph.file_size).ok_or_else(|| {
                MelError::Loader(format!(
                    "BSS address overflow at target address {target_addr:#x} file size {:#x}",
                    ph.file_size
                ))
            })?;

            let zero_buffer = vec![0u8; (bss_size.min(65536)) as usize];
            let mut remaining = bss_size;
            let mut cur_addr = bss_addr;

            while remaining > 0 {
                let chunk_size = (remaining.min(zero_buffer.len() as u64)) as usize;
                vm.write_memory(cur_addr, &zero_buffer[..chunk_size])?;
                cur_addr = cur_addr.checked_add(chunk_size as u64).ok_or_else(|| {
                    MelError::Loader(format!("Address overflow while zeroing BSS at {cur_addr:#x}"))
                })?;
                remaining -= chunk_size as u64;
            }

            total_bss_zeroed = total_bss_zeroed.checked_add(bss_size).ok_or_else(|| {
                MelError::Loader("Overflow in total zeroed BSS calculation".into())
            })?;
        }

        let segment_mem_end = target_addr.checked_add(ph.mem_size).ok_or_else(|| {
            MelError::Loader(format!(
                "Segment memory range overflow at {target_addr:#x} size {:#x}",
                ph.mem_size
            ))
        })?;

        lowest_addr = lowest_addr.min(target_addr);
        highest_addr = highest_addr.max(segment_mem_end);
        segments_loaded += 1;
    }

    vm.set_pc(info.entry_point)?;

    tracing::debug!(
        "Loaded ELF64 ({}) into VM: {} segments, {} bytes code/data, {} bytes BSS, entry PC {:#x}",
        info.architecture,
        segments_loaded,
        total_bytes_loaded,
        total_bss_zeroed,
        info.entry_point
    );

    Ok(ElfLoadSummary {
        architecture: info.architecture,
        entry_point: info.entry_point,
        segments_loaded,
        total_bytes_loaded,
        total_bss_zeroed,
        lowest_addr,
        highest_addr,
    })
}

/// Read and load an ELF64 binary file from disk into virtual machine memory.
pub fn load_elf_from_file<P: AsRef<Path>>(
    vm: &mut mel_vm::VirtualMachine,
    path: P,
) -> Result<ElfLoadSummary, MelError> {
    let bytes = std::fs::read(path.as_ref())?;
    load_elf(vm, &bytes)
}

/// Convenience function to parse ELF bytes into `ElfInfo`.
pub fn parse_elf(bytes: &[u8]) -> Result<ElfInfo, MelError> {
    ElfInfo::parse(bytes)
}

/// Convenience function to read and parse an ELF file into `ElfInfo`.
pub fn parse_elf_file<P: AsRef<Path>>(path: P) -> Result<ElfInfo, MelError> {
    ElfInfo::from_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestElfBuilder {
        machine: u16,
        is_big_endian: bool,
        entry_point: u64,
        code: Vec<u8>,
        text_vaddr: u64,
        bss_vaddr: u64,
        bss_size: u64,
    }

    impl TestElfBuilder {
        fn new(machine: u16, is_big_endian: bool, entry_point: u64, code: &[u8]) -> Self {
            Self {
                machine,
                is_big_endian,
                entry_point,
                code: code.to_vec(),
                text_vaddr: entry_point,
                bss_vaddr: entry_point + 0x1000,
                bss_size: 0x800,
            }
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
            // 0x0040: Program Headers (2 * 56 bytes = 112 bytes) -> ends at 0xB0
            // 0x0100: .text section data (code.len())
            // 0x0200: .shstrtab section data
            // 0x0300: Section Headers (4 * 64 bytes = 256 bytes)

            let ph_offset = 64u64;
            let ph_count = 2u16;

            let text_offset = 0x100u64;
            let text_size = self.code.len() as u64;

            let shstrtab_offset = 0x200u64;
            let mut shstrtab = Vec::new();
            shstrtab.push(0); // null byte index 0
            let text_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".text\0");
            let bss_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".bss\0");
            let shstrtab_name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".shstrtab\0");

            let sh_offset = 0x300u64;
            let sh_count = 4u16;
            let shstrndx = 3u16;

            // 1. ELF Header (64 bytes)
            buf.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // e_ident[0..4]
            buf.push(2); // e_ident[EI_CLASS] = ELFCLASS64
            buf.push(if self.is_big_endian { 2 } else { 1 }); // e_ident[EI_DATA]
            buf.push(1); // e_ident[EI_VERSION] = EV_CURRENT
            buf.push(0); // e_ident[EI_OSABI] = System V
            buf.extend_from_slice(&[0u8; 8]); // padding

            write_u16(&mut buf, elf::ET_EXEC); // e_type
            write_u16(&mut buf, self.machine); // e_machine
            write_u32(&mut buf, 1); // e_version
            write_u64(&mut buf, self.entry_point); // e_entry
            write_u64(&mut buf, ph_offset); // e_phoff
            write_u64(&mut buf, sh_offset); // e_shoff
            write_u32(&mut buf, 0); // e_flags
            write_u16(&mut buf, 64); // e_ehsize
            write_u16(&mut buf, 56); // e_phentsize
            write_u16(&mut buf, ph_count); // e_phnum
            write_u16(&mut buf, 64); // e_shentsize
            write_u16(&mut buf, sh_count); // e_shnum
            write_u16(&mut buf, shstrndx); // e_shstrndx

            // 2. Program Headers (2 * 56 bytes)
            // Segment 1: .text (RX)
            write_u32(&mut buf, elf::PT_LOAD);
            write_u32(&mut buf, elf::PF_R | elf::PF_X);
            write_u64(&mut buf, text_offset);
            write_u64(&mut buf, self.text_vaddr);
            write_u64(&mut buf, self.text_vaddr);
            write_u64(&mut buf, text_size);
            write_u64(&mut buf, text_size);
            write_u64(&mut buf, 0x1000);

            // Segment 2: .bss (RW)
            write_u32(&mut buf, elf::PT_LOAD);
            write_u32(&mut buf, elf::PF_R | elf::PF_W);
            write_u64(&mut buf, text_offset + text_size);
            write_u64(&mut buf, self.bss_vaddr);
            write_u64(&mut buf, self.bss_vaddr);
            write_u64(&mut buf, 0); // filesz = 0 for BSS
            write_u64(&mut buf, self.bss_size); // memsz
            write_u64(&mut buf, 0x1000);

            // Pad to text_offset
            if buf.len() < text_offset as usize {
                buf.resize(text_offset as usize, 0);
            }
            buf.extend_from_slice(&self.code);

            // Pad to shstrtab_offset
            if buf.len() < shstrtab_offset as usize {
                buf.resize(shstrtab_offset as usize, 0);
            }
            buf.extend_from_slice(&shstrtab);

            // Pad to sh_offset
            if buf.len() < sh_offset as usize {
                buf.resize(sh_offset as usize, 0);
            }

            // Section 0: NULL section
            for _ in 0..8 {
                write_u64(&mut buf, 0);
            }

            // Section 1: .text
            write_u32(&mut buf, text_name_off);
            write_u32(&mut buf, elf::SHT_PROGBITS);
            write_u64(&mut buf, (elf::SHF_ALLOC | elf::SHF_EXECINSTR) as u64);
            write_u64(&mut buf, self.text_vaddr);
            write_u64(&mut buf, text_offset);
            write_u64(&mut buf, text_size);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u64(&mut buf, 4);
            write_u64(&mut buf, 0);

            // Section 2: .bss
            write_u32(&mut buf, bss_name_off);
            write_u32(&mut buf, elf::SHT_NOBITS);
            write_u64(&mut buf, (elf::SHF_ALLOC | elf::SHF_WRITE) as u64);
            write_u64(&mut buf, self.bss_vaddr);
            write_u64(&mut buf, text_offset + text_size);
            write_u64(&mut buf, self.bss_size);
            write_u32(&mut buf, 0);
            write_u32(&mut buf, 0);
            write_u64(&mut buf, 8);
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
    fn test_parse_riscv64_elf() {
        let code = [0x13, 0x05, 0xa0, 0x02]; // addi a0, zero, 42
        let elf_bytes = TestElfBuilder::new(elf::EM_RISCV, false, 0x8000_0000, &code).build();

        let info = parse_elf(&elf_bytes).unwrap();
        assert_eq!(info.architecture, Architecture::Riscv64);
        assert_eq!(info.endianness, Endianness::Little);
        assert!(info.is_64bit);
        assert_eq!(info.elf_type, ElfType::Executable);
        assert_eq!(info.entry_point, 0x8000_0000);

        let load_segs: Vec<_> = info.load_segments().collect();
        assert_eq!(load_segs.len(), 2);

        let text_seg = load_segs[0];
        assert_eq!(text_seg.virtual_addr, 0x8000_0000);
        assert_eq!(text_seg.file_size, 4);
        assert_eq!(text_seg.mem_size, 4);
        assert!(text_seg.is_readable());
        assert!(!text_seg.is_writable());
        assert!(text_seg.is_executable());
        assert_eq!(text_seg.permissions(), Permissions::READ_EXECUTE);
        assert_eq!(text_seg.data(&elf_bytes).unwrap(), &code);

        let bss_seg = load_segs[1];
        assert_eq!(bss_seg.virtual_addr, 0x8000_1000);
        assert_eq!(bss_seg.file_size, 0);
        assert_eq!(bss_seg.mem_size, 0x800);
        assert_eq!(bss_seg.bss_size(), 0x800);
        assert!(bss_seg.is_readable());
        assert!(bss_seg.is_writable());
        assert!(!bss_seg.is_executable());
        assert_eq!(bss_seg.permissions(), Permissions::READ_WRITE);

        let text_sec = info.find_section(".text").unwrap();
        assert_eq!(text_sec.section_type, SectionType::ProgBits);
        assert!(text_sec.is_allocated());
        assert!(text_sec.is_executable());
        assert_eq!(text_sec.data(&elf_bytes).unwrap(), &code);

        let bss_sec = info.find_section(".bss").unwrap();
        assert_eq!(bss_sec.section_type, SectionType::NoBits);
        assert!(bss_sec.is_allocated());
        assert!(bss_sec.is_writable());
        let empty: &[u8] = &[];
        assert_eq!(bss_sec.data(&elf_bytes).unwrap(), empty);
    }

    #[test]
    fn test_parse_arm64_elf() {
        let code = [0x00, 0x00, 0x80, 0xd2]; // mov x0, #0
        let elf_bytes = TestElfBuilder::new(elf::EM_AARCH64, false, 0x4000_0000, &code).build();

        let info = parse_elf(&elf_bytes).unwrap();
        assert_eq!(info.architecture, Architecture::Arm64);
        assert_eq!(info.endianness, Endianness::Little);
        assert_eq!(info.entry_point, 0x4000_0000);
        assert_eq!(info.segments().len(), 2);
    }

    #[test]
    fn test_parse_x86_64_elf() {
        let code = [0x48, 0x31, 0xc0]; // xor rax, rax
        let elf_bytes = TestElfBuilder::new(elf::EM_X86_64, false, 0x0010_0000, &code).build();

        let info = parse_elf(&elf_bytes).unwrap();
        assert_eq!(info.architecture, Architecture::X86_64);
        assert_eq!(info.endianness, Endianness::Little);
        assert_eq!(info.entry_point, 0x0010_0000);
        assert_eq!(info.sections().len(), 4);
    }

    #[test]
    fn test_parse_powerpc64_elf() {
        let code = [0x38, 0x60, 0x00, 0x00]; // li r3, 0
        let elf_bytes = TestElfBuilder::new(elf::EM_PPC64, true, 0x0000_0100, &code).build();

        let info = parse_elf(&elf_bytes).unwrap();
        assert_eq!(info.architecture, Architecture::PowerIso64);
        assert_eq!(info.endianness, Endianness::Big);
        assert_eq!(info.entry_point, 0x0000_0100);
    }

    #[test]
    fn test_parse_invalid_magic() {
        let invalid = b"NOT_AN_ELF_FILE_FOR_SURE";
        let err = parse_elf(invalid).unwrap_err();
        assert!(err.to_string().contains("Invalid ELF magic"));
    }

    #[test]
    fn test_parse_too_short() {
        let err = parse_elf(b"short").unwrap_err();
        assert!(err.to_string().contains("too small"));
    }

    #[test]
    fn test_parse_32bit_rejected() {
        let mut bytes = vec![0u8; 64];
        bytes[0..4].copy_from_slice(&elf::ELFMAG);
        bytes[4] = elf::ELFCLASS32;
        bytes[5] = elf::ELFDATA2LSB;

        let err = parse_elf(&bytes).unwrap_err();
        assert!(err.to_string().contains("Unsupported ELF class"));
    }

    #[test]
    fn test_parse_invalid_encoding() {
        let mut bytes = vec![0u8; 64];
        bytes[0..4].copy_from_slice(&elf::ELFMAG);
        bytes[4] = elf::ELFCLASS64;
        bytes[5] = 99; // invalid data encoding

        let err = parse_elf(&bytes).unwrap_err();
        assert!(err.to_string().contains("Invalid ELF data encoding"));
    }

    #[test]
    fn test_parse_unsupported_machine() {
        let code = [0x00, 0x00];
        let elf_bytes = TestElfBuilder::new(0x1234, false, 0x1000, &code).build();

        let err = parse_elf(&elf_bytes).unwrap_err();
        assert!(err.to_string().contains("Unsupported ELF e_machine"));
    }
}
