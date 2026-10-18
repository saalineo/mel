//! Multi-architecture instruction disassembly engine powered by Capstone.

use capstone::prelude::*;
use mel_core::{Architecture, MelError};
use serde::{Deserialize, Serialize};

/// Disassembled instruction representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisassembledInstruction {
    pub address: u64,
    pub size: usize,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operands: String,
}

impl DisassembledInstruction {
    /// Format instruction into standard assembly listing string.
    pub fn format_line(&self) -> String {
        let hex_bytes = self
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");

        if self.operands.is_empty() {
            format!(
                "{:#010x}:  {:<16} {}",
                self.address, hex_bytes, self.mnemonic
            )
        } else {
            format!(
                "{:#010x}:  {:<16} {} {}",
                self.address, hex_bytes, self.mnemonic, self.operands
            )
        }
    }
}

impl std::fmt::Display for DisassembledInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_line())
    }
}

/// Build a configured Capstone disassembler engine for the target architecture.
pub fn create_disassembler(arch: Architecture) -> Result<Capstone, MelError> {
    let result = match arch {
        Architecture::Riscv64 => Capstone::new()
            .riscv()
            .mode(arch::riscv::ArchMode::RiscV64)
            .extra_mode([arch::riscv::ArchExtraMode::RiscVC].iter().copied())
            .detail(true)
            .build(),
        Architecture::Arm64 => Capstone::new()
            .arm64()
            .mode(arch::arm64::ArchMode::Arm)
            .detail(true)
            .build(),
        Architecture::X86_64 => Capstone::new()
            .x86()
            .mode(arch::x86::ArchMode::Mode64)
            .detail(true)
            .build(),
        Architecture::PowerIso64 => Capstone::new()
            .ppc()
            .mode(arch::ppc::ArchMode::Mode64)
            .endian(capstone::Endian::Big)
            .detail(true)
            .build(),
    };

    result.map_err(|e| {
        MelError::Debugger(format!(
            "Failed to initialize Capstone disassembler for {arch:?}: {e}"
        ))
    })
}

/// Disassemble raw instruction bytes into human-readable instructions.
pub fn disassemble(
    arch: Architecture,
    code: &[u8],
    address: u64,
) -> Result<Vec<DisassembledInstruction>, MelError> {
    if code.is_empty() {
        return Ok(Vec::new());
    }

    let cs = create_disassembler(arch)?;
    let insns = cs
        .disasm_all(code, address)
        .map_err(|e| MelError::Debugger(format!("Disassembly failed at {address:#x}: {e}")))?;

    let disasm_list = insns
        .iter()
        .map(|insn| DisassembledInstruction {
            address: insn.address(),
            size: insn.bytes().len(),
            bytes: insn.bytes().to_vec(),
            mnemonic: insn.mnemonic().unwrap_or("???").to_string(),
            operands: insn.op_str().unwrap_or("").to_string(),
        })
        .collect();

    Ok(disasm_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disassemble_riscv64_instructions() {
        // RISC-V 64-bit assembly:
        // lui x1, 0x10000
        // addi x2, x0, 88
        // sb x2, 0(x1)
        let code: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x80, 0x05, 0x23, 0x80, 0x20, 0x00,
        ];

        let instructions = disassemble(Architecture::Riscv64, &code, 0x8000_0000).unwrap();
        assert_eq!(instructions.len(), 3);

        assert_eq!(instructions[0].address, 0x8000_0000);
        assert_eq!(instructions[0].mnemonic, "lui");
        assert_eq!(instructions[0].size, 4);

        assert_eq!(instructions[1].address, 0x8000_0004);
        assert_eq!(instructions[1].mnemonic, "addi");

        assert_eq!(instructions[2].address, 0x8000_0008);
        assert_eq!(instructions[2].mnemonic, "sb");
    }

    #[test]
    fn test_disassemble_riscv_compressed_c_nop() {
        // c.nop is 0x0001 (16-bit compressed instruction)
        let code: [u8; 2] = [0x01, 0x00];
        let instructions = disassemble(Architecture::Riscv64, &code, 0x1000).unwrap();
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].size, 2);
        assert_eq!(instructions[0].mnemonic, "c.nop");
    }

    #[test]
    fn test_disassemble_empty_code() {
        let instructions = disassemble(Architecture::Riscv64, &[], 0x8000_0000).unwrap();
        assert!(instructions.is_empty());
    }

    #[test]
    fn test_disassemble_arm64() {
        // mov x0, #42 (0xd2800540)
        let code: [u8; 4] = [0x40, 0x05, 0x80, 0xd2];
        let instructions = disassemble(Architecture::Arm64, &code, 0x4000_0000).unwrap();
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].mnemonic, "mov");
    }

    #[test]
    fn test_disassemble_x86_64() {
        // nop; ret (0x90, 0xc3)
        let code: [u8; 2] = [0x90, 0xc3];
        let instructions = disassemble(Architecture::X86_64, &code, 0x1000).unwrap();
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0].mnemonic, "nop");
        assert_eq!(instructions[1].mnemonic, "ret");
    }

    #[test]
    fn test_disassemble_power_iso64() {
        // li r3, 42 (0x38, 0x60, 0x00, 0x2a)
        let code: [u8; 4] = [0x38, 0x60, 0x00, 0x2a];
        let instructions = disassemble(Architecture::PowerIso64, &code, 0x100).unwrap();
        assert_eq!(instructions.len(), 1);
        assert!(instructions[0].mnemonic == "li" || instructions[0].mnemonic == "addi");
    }
}
