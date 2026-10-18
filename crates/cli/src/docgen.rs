//! Hardware Documentation Markdown Exporter.

use mel_core::Architecture;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct DocGenerator;

impl DocGenerator {
    /// Generate complete markdown documentation for a given CPU architecture.
    pub fn generate_arch_markdown(arch: Architecture) -> String {
        let mut md = String::new();
        md.push_str(&format!("# MEL Hardware Reference — {}\n\n", arch.name()));
        md.push_str(&format!("Target Architecture: `{:?}`\n\n", arch));
        md.push_str(&format!(
            "- Reset Vector: `{:#010x}`\n",
            arch.default_reset_vector()
        ));
        md.push_str(&format!(
            "- Default RAM Base: `{:#010x}`\n",
            arch.default_ram_base()
        ));
        md.push_str(&format!(
            "- Default Endianness: `{:?}`\n\n",
            arch.default_endianness()
        ));

        md.push_str("## Memory Map Layout\n\n");
        match arch {
            Architecture::Riscv64 => {
                md.push_str("| Address Range | Size | Component | Access | Description |\n");
                md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
                md.push_str("| `0x0000_1000 - 0x0000_1FFF` | 4 KB | Boot ROM | R/X | Reset vector & jump payload |\n");
                md.push_str("| `0x0200_0000 - 0x0200_FFFF` | 64 KB | CLINT | MMIO R/W | Timer & software interrupts |\n");
                md.push_str("| `0x0C00_0000 - 0x0FFF_FFFF` | 64 MB | PLIC | MMIO R/W | Platform-level interrupt controller |\n");
                md.push_str("| `0x1000_0000 - 0x1000_00FF` | 256 B | UART 16550 | MMIO R/W | Serial terminal console |\n");
                md.push_str("| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual disk storage controller |\n");
                md.push_str("| `0x8000_0000 - 0x83FF_FFFF` | 64 MB | Main RAM | R/W/X | System physical RAM |\n\n");

                md.push_str("## CLINT Register Offsets\n\n");
                md.push_str("| Offset | Register | Description |\n");
                md.push_str("| :--- | :--- | :--- |\n");
                md.push_str("| `0x0000` | `msip[0]` | Machine Software Interrupt Pending |\n");
                md.push_str(
                    "| `0x4000` | `mtimecmp[0]` | Machine Timer Compare (triggers MTIP) |\n",
                );
                md.push_str("| `0xBFF8` | `mtime` | Machine Monotonic Counter Clock |\n\n");

                md.push_str("## ABI Register Conventions\n\n");
                md.push_str("| Register | ABI Name | Role | Saver |\n");
                md.push_str("| :--- | :--- | :--- | :--- |\n");
                md.push_str("| `x0` | `zero` | Hardwired Zero | N/A |\n");
                md.push_str("| `x1` | `ra` | Return Address | Caller |\n");
                md.push_str("| `x2` | `sp` | Stack Pointer | Callee |\n");
                md.push_str("| `x8` | `s0`/`fp` | Saved Register / Frame Pointer | Callee |\n");
                md.push_str(
                    "| `x10..x17` | `a0..a7` | Function Arguments & Return Values | Caller |\n",
                );
            }
            Architecture::Arm64 => {
                md.push_str("| Address Range | Size | Component | Access | Description |\n");
                md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
                md.push_str("| `0x0000_0000 - 0x0000_FFFF` | 64 KB | Boot ROM | R/X | Reset vector (`0x0000_0000`) |\n");
                md.push_str("| `0x0800_0000 - 0x0800_FFFF` | 64 KB | GIC | MMIO R/W | Generic Interrupt Controller |\n");
                md.push_str("| `0x0900_0000 - 0x0900_00FF` | 256 B | UART | MMIO R/W | Serial Console (PL011) |\n");
                md.push_str("| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |\n");
                md.push_str("| `0x4000_0000 - 0x43FF_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM |\n\n");

                md.push_str("## ABI Register Conventions\n\n");
                md.push_str("| Register | Role |\n");
                md.push_str("| :--- | :--- |\n");
                md.push_str("| `x0..x7` | Parameter / Result registers |\n");
                md.push_str("| `x8` | Indirect result / Syscall ID |\n");
                md.push_str("| `x19..x28` | Callee-saved registers |\n");
                md.push_str("| `x29` (`fp`) | Frame pointer |\n");
                md.push_str("| `x30` (`lr`) | Link register |\n");
                md.push_str("| `sp` | Stack pointer |\n");
            }
            Architecture::X86_64 => {
                md.push_str("| Address Range | Size | Component | Access | Description |\n");
                md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
                md.push_str("| `0x000F_0000 - 0x000F_FFFF` | 64 KB | BIOS ROM | R/X | Reset Vector (`0x000F_FFF0`) |\n");
                md.push_str("| `0x0010_0000 - 0x040F_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM (1MB Base) |\n");
                md.push_str("| `0x1000_0000 - 0x1000_00FF` | 256 B | COM1 UART | MMIO R/W | Serial Console |\n");
                md.push_str("| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |\n");
                md.push_str("| `0xFEE0_0000 - 0xFEE0_0FFF` | 4 KB | Local APIC | MMIO R/W | Interrupt Controller |\n\n");

                md.push_str("## ABI Register Conventions\n\n");
                md.push_str("| Register | Role |\n");
                md.push_str("| :--- | :--- |\n");
                md.push_str("| `rax` | Return Value / Syscall ID |\n");
                md.push_str("| `rdi, rsi, rdx, rcx, r8, r9` | Function Arguments |\n");
                md.push_str("| `rbx, rsp, rbp, r12..r15` | Callee-saved registers |\n");
                md.push_str("| `rsp` | Stack pointer |\n");
            }
            Architecture::PowerIso64 => {
                md.push_str("| Address Range | Size | Component | Access | Description |\n");
                md.push_str("| :--- | :--- | :--- | :--- | :--- |\n");
                md.push_str("| `0x0000_0000 - 0x03FF_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM (Vectors at 0x100) |\n");
                md.push_str("| `0x1000_0000 - 0x1000_00FF` | 256 B | UART 16550 | MMIO R/W | Serial Console |\n");
                md.push_str("| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |\n");
                md.push_str("| `0x3000_0000 - 0x3000_FFFF` | 64 KB | OpenPIC | MMIO R/W | Interrupt Controller |\n\n");

                md.push_str("## ABI Register Conventions\n\n");
                md.push_str("| Register | Role |\n");
                md.push_str("| :--- | :--- |\n");
                md.push_str("| `r0` | Language specific / Syscall ID |\n");
                md.push_str("| `r1` | Stack pointer |\n");
                md.push_str("| `r3..r10` | Function parameter / return values |\n");
                md.push_str("| `r14..r31` | Callee-saved nonvolatile registers |\n");
            }
        }

        md
    }

    /// Export documentation files to target directory.
    pub fn export_docs(
        arch_filter: Option<Architecture>,
        out_dir: &Path,
    ) -> std::io::Result<Vec<PathBuf>> {
        fs::create_dir_all(out_dir)?;
        let mut created = Vec::new();

        let archs: Vec<Architecture> = match arch_filter {
            Some(a) => vec![a],
            None => vec![
                Architecture::Riscv64,
                Architecture::Arm64,
                Architecture::X86_64,
                Architecture::PowerIso64,
            ],
        };

        for arch in archs {
            let content = Self::generate_arch_markdown(arch);
            let filename = format!("{}_hardware.md", arch.name().to_lowercase());
            let out_path = out_dir.join(filename);
            let mut file = fs::File::create(&out_path)?;
            file.write_all(content.as_bytes())?;
            created.push(out_path);
        }

        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_markdown_all_archs() {
        for arch in [
            Architecture::Riscv64,
            Architecture::Arm64,
            Architecture::X86_64,
            Architecture::PowerIso64,
        ] {
            let md = DocGenerator::generate_arch_markdown(arch);
            assert!(md.contains("Memory Map Layout"));
            assert!(md.contains("ABI Register Conventions"));
            assert!(md.contains(arch.name()));
        }
    }

    #[test]
    fn test_export_docs_to_temp() {
        let temp_dir = std::env::temp_dir().join("mel_doc_test");
        let exported = DocGenerator::export_docs(None, &temp_dir).unwrap();
        assert_eq!(exported.len(), 4);
        for p in exported {
            assert!(p.exists());
            let _ = fs::remove_file(p);
        }
        let _ = fs::remove_dir(temp_dir);
    }
}
