//! # MEL CLI Library
//!
//! Exposes command handlers and execution logic for the MEL CLI interface.

pub mod args;

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use mel_core::Architecture;
use mel_debugger::{disassemble, format_hex_dump, Debugger, ReplSession};
use mel_loader::load_raw_binary;
use mel_vm::VirtualMachine;

pub use args::*;

/// Build a virtual machine instance matching the requested architecture.
pub fn build_vm_for_arch(arch: Architecture) -> Result<VirtualMachine> {
    mel_vm::build_vm_for_arch(arch)
        .with_context(|| format!("Failed to build virtual machine for {:?}", arch))
}

/// Execute the `mel run` subcommand.
pub fn execute_run<W: Write>(args: &RunArgs, stdout: &mut W) -> Result<()> {
    let arch: Architecture = args.arch.into();
    let entry_addr = args.entry.unwrap_or_else(|| arch.default_reset_vector());

    let payload = std::fs::read(&args.file)
        .with_context(|| format!("Failed to read binary payload from '{}'", args.file))?;

    let mut vm = build_vm_for_arch(arch)?;
    load_raw_binary(&mut vm, &payload, entry_addr)
        .with_context(|| format!("Failed to load raw binary at entry {entry_addr:#x}"))?;

    if !args.quiet {
        writeln!(
            stdout,
            "[MEL] Running '{}' on [{:?}] at PC {:#010x} (budget: {} cycles)...",
            args.file, arch, entry_addr, args.cycles
        )?;
    }

    let step_res = vm
        .run(args.cycles)
        .with_context(|| "Execution failure during VM run")?;

    // Drain and display UART output if available
    if let Some(uart_arc) = vm.uart() {
        if let Ok(mut uart) = uart_arc.lock() {
            let output = uart.drain_output();
            if !output.is_empty() {
                if !args.quiet {
                    writeln!(stdout, "[UART Console Output]:")?;
                }
                stdout.write_all(&output)?;
                if !output.ends_with(b"\n") {
                    writeln!(stdout)?;
                }
            }
        }
    }

    if !args.quiet {
        let final_pc = vm.pc().unwrap_or(0);
        writeln!(
            stdout,
            "[MEL] Execution stopped: {:?} (final PC: {:#010x})",
            step_res, final_pc
        )?;
    }

    Ok(())
}

/// Execute the `mel debug` subcommand.
pub fn execute_debug<R: BufRead, W: Write>(
    args: &DebugArgs,
    reader: R,
    mut writer: W,
) -> Result<()> {
    let arch: Architecture = args.arch.into();
    let entry_addr = args.entry.unwrap_or_else(|| arch.default_reset_vector());

    let payload = std::fs::read(&args.file)
        .with_context(|| format!("Failed to read binary payload from '{}'", args.file))?;

    let mut vm = build_vm_for_arch(arch)?;
    load_raw_binary(&mut vm, &payload, entry_addr)
        .with_context(|| format!("Failed to load raw binary at entry {entry_addr:#x}"))?;

    let debugger = Debugger::new(vm);
    let mut session = ReplSession::new(debugger);

    session
        .run_interactive(reader, &mut writer)
        .with_context(|| "Interactive debug session terminated with error")?;

    Ok(())
}

/// Execute the `mel inspect` subcommand.
pub fn execute_inspect<W: Write>(args: &InspectArgs, stdout: &mut W) -> Result<()> {
    let arch: Architecture = args.arch.into();
    let entry_addr = args.entry.unwrap_or_else(|| arch.default_reset_vector());

    let payload = std::fs::read(&args.file)
        .with_context(|| format!("Failed to read binary payload from '{}'", args.file))?;

    writeln!(stdout, "=== Payload Inspection: {} ===", args.file)?;
    writeln!(stdout, "Architecture : {:?}", arch)?;
    writeln!(
        stdout,
        "Payload Size : {} bytes ({:#x})",
        payload.len(),
        payload.len()
    )?;
    writeln!(stdout, "Base Address : {:#010x}", entry_addr)?;
    writeln!(stdout)?;

    // Disassembly preview
    writeln!(
        stdout,
        "--- Disassembly Preview (up to {} instructions) ---",
        args.count
    )?;
    match disassemble(arch, &payload, entry_addr) {
        Ok(insns) => {
            if insns.is_empty() {
                writeln!(stdout, "(no instructions disassembled)")?;
            } else {
                for insn in insns.into_iter().take(args.count) {
                    writeln!(stdout, "  {}", insn.format_line())?;
                }
            }
        }
        Err(e) => {
            writeln!(stdout, "Disassembly error: {e}")?;
        }
    }
    writeln!(stdout)?;

    // Hex dump preview
    let dump_bytes = payload.len().min(64);
    writeln!(stdout, "--- Raw Hex Dump (first {} bytes) ---", dump_bytes)?;
    let dump_str = format_hex_dump(entry_addr, &payload[..dump_bytes]);
    write!(stdout, "{dump_str}")?;

    Ok(())
}

/// Execute the `mel machine` subcommand.
pub fn execute_machine<W: Write>(args: &MachineArgs, stdout: &mut W) -> Result<()> {
    match &args.command {
        None | Some(MachineSubcommand::List) => {
            writeln!(stdout, "=== Supported CPU Architectures ===")?;
            writeln!(
                stdout,
                "  {:<12} {:<14} {:<14} {:<10}",
                "Architecture", "Reset Vector", "RAM Base", "Endianness"
            )?;
            writeln!(
                stdout,
                "  --------------------------------------------------------"
            )?;
            let archs = [
                Architecture::Riscv64,
                Architecture::Arm64,
                Architecture::X86_64,
                Architecture::PowerIso64,
            ];
            for a in archs {
                writeln!(
                    stdout,
                    "  {:<12} {:#014x} {:#014x} {:?}",
                    a.name(),
                    a.default_reset_vector(),
                    a.default_ram_base(),
                    a.default_endianness()
                )?;
            }
        }
        Some(MachineSubcommand::Info { arch }) => {
            let a: Architecture = (*arch).into();
            writeln!(stdout, "=== Architecture Details: {} ===", a.name())?;
            writeln!(stdout, "  Name         : {:?}", a)?;
            writeln!(
                stdout,
                "  Reset Vector : {:#014x}",
                a.default_reset_vector()
            )?;
            writeln!(stdout, "  Default RAM  : {:#014x}", a.default_ram_base())?;
            writeln!(stdout, "  Endianness   : {:?}", a.default_endianness())?;
            match a {
                Architecture::Riscv64 => {
                    writeln!(stdout, "\n  Memory Map (RISC-V 64):")?;
                    writeln!(stdout, "    0x0000_1000 - 0x0000_1FFF (4 KB)   : Boot ROM")?;
                    writeln!(
                        stdout,
                        "    0x1000_0000 - 0x1000_00FF (256 B)  : UART 16550"
                    )?;
                    writeln!(stdout, "    0x8000_0000 - 0x83FF_FFFF (64 MB)  : Main RAM")?;
                }
                Architecture::Arm64 => {
                    writeln!(stdout, "\n  Memory Map (ARM64 / AArch64):")?;
                    writeln!(stdout, "    0x0000_0000 - 0x0000_FFFF (64 KB)  : Boot ROM")?;
                    writeln!(
                        stdout,
                        "    0x0900_0000 - 0x0900_00FF (256 B)  : UART PL011"
                    )?;
                    writeln!(stdout, "    0x4000_0000 - 0x43FF_FFFF (64 MB)  : Main RAM")?;
                }
                Architecture::X86_64 => {
                    writeln!(stdout, "\n  Memory Map (x86-64):")?;
                    writeln!(stdout, "    0x000F_0000 - 0x000F_FFFF (64 KB)  : BIOS ROM")?;
                    writeln!(stdout, "    0x0010_0000 - 0x040F_FFFF (64 MB)  : Main RAM")?;
                }
                Architecture::PowerIso64 => {
                    writeln!(stdout, "\n  Memory Map (Power ISA 64):")?;
                    writeln!(stdout, "    0x0000_0000 - 0x03FF_FFFF (64 MB)  : Main RAM")?;
                }
            }
        }
    }
    Ok(())
}
