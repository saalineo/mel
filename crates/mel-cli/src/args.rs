//! Command-line argument definitions using Clap.

use clap::{Args, Parser, Subcommand, ValueEnum};
use mel_core::Architecture;

/// MEL — Bare-metal Systems-Engineering Simulation CLI.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "mel",
    version,
    about = "MEL — Bare-Metal Systems-Engineering Simulation Engine",
    long_about = "MEL is a bare-metal systems-engineering simulation where players start with real CPU hardware\nand progressively build, configure, understand, and replace the software stack."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available MEL CLI subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Execute a binary payload on a virtual machine
    Run(RunArgs),

    /// Launch interactive CLI debugger for a binary payload
    Debug(DebugArgs),

    /// Inspect and disassemble a binary payload
    Inspect(InspectArgs),

    /// Query and list supported CPU architectures and machine configurations
    Machine(MachineArgs),
}

/// Supported target architecture argument.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchArg {
    #[value(name = "riscv64", alias = "riscv")]
    Riscv64,
    #[value(name = "arm64", alias = "aarch64")]
    Arm64,
    #[value(name = "x86_64", alias = "x86-64", alias = "amd64")]
    X86_64,
    #[value(name = "powerpc64", alias = "ppc64", alias = "poweriso64")]
    PowerIso64,
}

impl From<ArchArg> for Architecture {
    fn from(arg: ArchArg) -> Self {
        match arg {
            ArchArg::Riscv64 => Architecture::Riscv64,
            ArchArg::Arm64 => Architecture::Arm64,
            ArchArg::X86_64 => Architecture::X86_64,
            ArchArg::PowerIso64 => Architecture::PowerIso64,
        }
    }
}

/// Arguments for `mel run`.
#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Path to binary executable payload
    pub file: String,

    /// Target CPU architecture
    #[arg(short, long, value_enum, default_value = "riscv64")]
    pub arch: ArchArg,

    /// Maximum execution cycle limit
    #[arg(short, long, default_value = "1000000")]
    pub cycles: u64,

    /// Memory load address / entry point PC (hex or decimal)
    #[arg(short, long, value_parser = parse_hex_or_dec)]
    pub entry: Option<u64>,

    /// Suppress simulation banners and print only UART output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Arguments for `mel debug`.
#[derive(Args, Debug, Clone)]
pub struct DebugArgs {
    /// Path to binary executable payload
    pub file: String,

    /// Target CPU architecture
    #[arg(short, long, value_enum, default_value = "riscv64")]
    pub arch: ArchArg,

    /// Memory load address / entry point PC (hex or decimal)
    #[arg(short, long, value_parser = parse_hex_or_dec)]
    pub entry: Option<u64>,
}

/// Arguments for `mel inspect`.
#[derive(Args, Debug, Clone)]
pub struct InspectArgs {
    /// Path to binary executable payload
    pub file: String,

    /// Target CPU architecture
    #[arg(short, long, value_enum, default_value = "riscv64")]
    pub arch: ArchArg,

    /// Disassembly start address (default: architecture RAM base)
    #[arg(short, long, value_parser = parse_hex_or_dec)]
    pub entry: Option<u64>,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value = "10")]
    pub count: usize,
}

/// Arguments for `mel machine`.
#[derive(Args, Debug, Clone, Default)]
pub struct MachineArgs {
    #[command(subcommand)]
    pub command: Option<MachineSubcommand>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum MachineSubcommand {
    /// List all officially supported CPU architectures and default hardware maps
    List,
    /// Display detailed memory map and registers for an architecture
    Info {
        #[arg(value_enum)]
        arch: ArchArg,
    },
}

/// Parse a string into an integer supporting hex (`0x...`), decimal, and underscores.
pub fn parse_hex_or_dec(s: &str) -> Result<u64, String> {
    let clean = s.trim().replace('_', "");
    if clean.starts_with("0x") || clean.starts_with("0X") {
        u64::from_str_radix(&clean[2..], 16).map_err(|e| format!("Invalid hex number '{s}': {e}"))
    } else {
        clean
            .parse::<u64>()
            .map_err(|e| format!("Invalid decimal or hex number '{s}': {e}"))
    }
}
