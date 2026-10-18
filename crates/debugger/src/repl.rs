//! Interactive CLI REPL debugger for MEL.
//!
//! Provides command parsing, step/continue execution, register inspection with diffs,
//! memory hex dumping, disassembly viewing, and breakpoint/watchpoint management.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use mel_core::{Address, Architecture, MelError, MelResult, RegisterId};

use crate::breakpoints::{DebugStopReason, WatchpointKind};
use crate::debugger::Debugger;

/// Parsed REPL command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    /// Step `count` instructions (default 1).
    Step(Option<u64>),
    /// Continue execution until breakpoint, watchpoint, trap, halt, or cycle limit.
    Continue,
    /// Inspect CPU registers with diff tracking.
    Registers,
    /// Examine memory hex dump at `addr` for `len` bytes.
    Memory { addr: Address, len: usize },
    /// Disassemble `count` instructions at optional `addr` (default current PC).
    Disassemble { addr: Option<Address>, count: usize },
    /// Set software breakpoint at `addr`.
    Break { addr: Address },
    /// Set temporary software breakpoint at `addr`.
    TempBreak { addr: Address },
    /// Set memory watchpoint at `addr` for `size` bytes.
    Watch {
        addr: Address,
        size: usize,
        kind: WatchpointKind,
    },
    /// Delete breakpoint or watchpoint by ID.
    Delete(u32),
    /// List all registered breakpoints and watchpoints.
    ListBreakpoints,
    /// Reset VM and CPU state.
    Reset,
    /// Show help summary.
    Help,
    /// Terminate REPL session.
    Quit,
    /// Empty line (no-op).
    Empty,
}

/// Action to be taken by the REPL driver after executing a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplAction {
    /// Continue listening for user commands.
    Continue,
    /// Exit the REPL loop.
    Quit,
}

/// Result returned from evaluating a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplOutput {
    /// Formatted output string to display to user.
    pub text: String,
    /// Flow control action.
    pub action: ReplAction,
}

impl ReplOutput {
    /// Create a standard output response continuing REPL loop.
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            action: ReplAction::Continue,
        }
    }

    /// Create a termination response.
    pub fn quit(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            action: ReplAction::Quit,
        }
    }
}

/// Interactive REPL debugger session.
pub struct ReplSession {
    debugger: Debugger,
    prev_registers: HashMap<String, u64>,
}

impl ReplSession {
    /// Create a new REPL session wrapping a debugger.
    pub fn new(debugger: Debugger) -> Self {
        Self {
            debugger,
            prev_registers: HashMap::new(),
        }
    }

    /// Access reference to inner debugger.
    pub fn debugger(&self) -> &Debugger {
        &self.debugger
    }

    /// Access mutable reference to inner debugger.
    pub fn debugger_mut(&mut self) -> &mut Debugger {
        &mut self.debugger
    }

    /// Resolve address or symbol name.
    fn resolve_addr_or_symbol(&self, token: &str) -> MelResult<Address> {
        if let Some(sym) = self.debugger.symbols().find_by_name(token) {
            return Ok(sym.address);
        }
        parse_number(token)
    }

    /// Parse a raw input string into a structured `ReplCommand`.
    pub fn parse_command(&self, input: &str) -> MelResult<ReplCommand> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(ReplCommand::Empty);
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = tokens[0].to_lowercase();

        match cmd.as_str() {
            "s" | "step" => {
                let count = if tokens.len() > 1 {
                    Some(parse_number(tokens[1])?)
                } else {
                    None
                };
                Ok(ReplCommand::Step(count))
            }
            "c" | "cont" | "continue" => Ok(ReplCommand::Continue),
            "r" | "reg" | "regs" | "registers" => Ok(ReplCommand::Registers),
            "m" | "mem" | "memory" | "x" | "examine" => {
                if tokens.len() < 2 {
                    return Err(MelError::Debugger(
                        "Usage: mem <addr|symbol> [len]  (e.g., mem 0x80000000 64 or mem main)"
                            .to_string(),
                    ));
                }
                let addr = self.resolve_addr_or_symbol(tokens[1])?;
                let len = if tokens.len() > 2 {
                    parse_number(tokens[2])? as usize
                } else {
                    32
                };
                Ok(ReplCommand::Memory { addr, len })
            }
            "d" | "dis" | "disasm" | "u" => {
                let (addr, count) = match tokens.len() {
                    1 => (None, 5),
                    2 => {
                        if tokens[1].eq_ignore_ascii_case("pc") {
                            (None, 5)
                        } else {
                            (Some(self.resolve_addr_or_symbol(tokens[1])?), 5)
                        }
                    }
                    _ => {
                        let a = if tokens[1].eq_ignore_ascii_case("pc") {
                            None
                        } else {
                            Some(self.resolve_addr_or_symbol(tokens[1])?)
                        };
                        let c = parse_number(tokens[2])? as usize;
                        (a, c)
                    }
                };
                Ok(ReplCommand::Disassemble { addr, count })
            }
            "b" | "break" | "bp" => {
                if tokens.len() < 2 {
                    return Err(MelError::Debugger(
                        "Usage: break <addr|symbol>  (e.g., break 0x80000000 or break main)"
                            .to_string(),
                    ));
                }
                let addr = self.resolve_addr_or_symbol(tokens[1])?;
                Ok(ReplCommand::Break { addr })
            }
            "tb" | "tbreak" => {
                if tokens.len() < 2 {
                    return Err(MelError::Debugger(
                        "Usage: tbreak <addr|symbol>  (e.g., tbreak 0x80000000 or tbreak main)"
                            .to_string(),
                    ));
                }
                let addr = self.resolve_addr_or_symbol(tokens[1])?;
                Ok(ReplCommand::TempBreak { addr })
            }
            "w" | "watch" => {
                if tokens.len() < 2 {
                    return Err(MelError::Debugger(
                        "Usage: watch <addr> [size] [r|w|rw]  (e.g., watch 0x80001000 4 w)"
                            .to_string(),
                    ));
                }
                let addr = parse_number(tokens[1])?;
                let size = if tokens.len() > 2 {
                    parse_number(tokens[2])? as usize
                } else {
                    4
                };
                let kind = if tokens.len() > 3 {
                    match tokens[3].to_lowercase().as_str() {
                        "r" | "read" => WatchpointKind::Read,
                        "w" | "write" => WatchpointKind::Write,
                        "rw" | "acc" | "access" => WatchpointKind::ReadWrite,
                        other => {
                            return Err(MelError::Debugger(format!(
                                "Unknown watchpoint access kind '{other}'. Use 'r', 'w', or 'rw'"
                            )));
                        }
                    }
                } else {
                    WatchpointKind::Write
                };
                Ok(ReplCommand::Watch { addr, size, kind })
            }
            "del" | "delete" | "rm" => {
                if tokens.len() < 2 {
                    return Err(MelError::Debugger("Usage: del <id>".to_string()));
                }
                let id = parse_number(tokens[1])? as u32;
                Ok(ReplCommand::Delete(id))
            }
            "bl" | "breakpoints" | "info" => {
                if tokens.len() > 1
                    && !tokens[1].eq_ignore_ascii_case("break")
                    && !tokens[1].eq_ignore_ascii_case("b")
                {
                    return Err(MelError::Debugger(
                        "Usage: info break  or  breakpoints".to_string(),
                    ));
                }
                Ok(ReplCommand::ListBreakpoints)
            }
            "reset" => Ok(ReplCommand::Reset),
            "h" | "help" | "?" => Ok(ReplCommand::Help),
            "q" | "quit" | "exit" => Ok(ReplCommand::Quit),
            unknown => Err(MelError::Debugger(format!(
                "Unknown debugger command '{unknown}'. Type 'help' for available commands."
            ))),
        }
    }

    /// Evaluate a single command line string and return formatted output.
    pub fn eval_line(&mut self, line: &str) -> MelResult<ReplOutput> {
        let cmd = self.parse_command(line)?;
        self.execute_command(cmd)
    }

    /// Execute a parsed `ReplCommand`.
    pub fn execute_command(&mut self, cmd: ReplCommand) -> MelResult<ReplOutput> {
        match cmd {
            ReplCommand::Empty => Ok(ReplOutput::ok("")),
            ReplCommand::Quit => Ok(ReplOutput::quit("Exiting MEL debugger.")),
            ReplCommand::Help => Ok(ReplOutput::ok(format_help_text())),
            ReplCommand::Reset => {
                self.debugger.reset()?;
                self.prev_registers.clear();
                let pc = self.debugger.pc()?;
                Ok(ReplOutput::ok(format!(
                    "Virtual machine reset. PC: {pc:#010x}"
                )))
            }
            ReplCommand::Break { addr } => {
                let id = self.debugger.add_breakpoint(addr);
                Ok(ReplOutput::ok(format!(
                    "Breakpoint #{id} set at {addr:#010x}"
                )))
            }
            ReplCommand::TempBreak { addr } => {
                let id = self.debugger.add_temporary_breakpoint(addr);
                Ok(ReplOutput::ok(format!(
                    "Temporary breakpoint #{id} set at {addr:#010x}"
                )))
            }
            ReplCommand::Watch { addr, size, kind } => {
                let id = self.debugger.add_watchpoint(addr, size, kind);
                Ok(ReplOutput::ok(format!(
                    "Watchpoint #{id} set at {addr:#010x} (size: {size} B, kind: {kind:?})"
                )))
            }
            ReplCommand::Delete(id) => {
                if let Ok(bp) = self.debugger.remove_breakpoint(id) {
                    Ok(ReplOutput::ok(format!(
                        "Removed breakpoint #{id} at {:#010x}",
                        bp.address
                    )))
                } else if let Ok(wp) = self.debugger.remove_watchpoint(id) {
                    Ok(ReplOutput::ok(format!(
                        "Removed watchpoint #{id} at {:#010x}",
                        wp.address
                    )))
                } else {
                    Err(MelError::Debugger(format!(
                        "No breakpoint or watchpoint found with ID #{id}"
                    )))
                }
            }
            ReplCommand::ListBreakpoints => {
                let bps = self.debugger.breakpoints().breakpoints();
                let wps = self.debugger.breakpoints().watchpoints();

                let mut out = String::new();
                if bps.is_empty() && wps.is_empty() {
                    out.push_str("No breakpoints or watchpoints set.\n");
                } else {
                    if !bps.is_empty() {
                        out.push_str("Breakpoints:\n");
                        out.push_str("  ID  Address       Enabled  Hits  Type       Description\n");
                        out.push_str("  -------------------------------------------------------\n");
                        for bp in &bps {
                            let bp_type = if bp.temporary { "Temp" } else { "Standard" };
                            let desc = bp.description.as_deref().unwrap_or("-");
                            out.push_str(&format!(
                                "  {:<3} {:#010x}   {:<7}  {:<4}  {:<9}  {}\n",
                                bp.id, bp.address, bp.enabled, bp.hit_count, bp_type, desc
                            ));
                        }
                    }
                    if !wps.is_empty() {
                        if !bps.is_empty() {
                            out.push('\n');
                        }
                        out.push_str("Watchpoints:\n");
                        out.push_str("  ID  Address       Size  Kind        Enabled  Hits  Type\n");
                        out.push_str("  -------------------------------------------------------\n");
                        for wp in &wps {
                            let wp_type = if wp.temporary { "Temp" } else { "Standard" };
                            let kind_str = format!("{:?}", wp.kind);
                            out.push_str(&format!(
                                "  {:<3} {:#010x}   {:<4}  {:<10}  {:<7}  {:<4}  {}\n",
                                wp.id,
                                wp.address,
                                wp.size,
                                kind_str,
                                wp.enabled,
                                wp.hit_count,
                                wp_type
                            ));
                        }
                    }
                }
                Ok(ReplOutput::ok(out))
            }
            ReplCommand::Step(count_opt) => {
                let count = count_opt.unwrap_or(1);
                let mut last_stop = DebugStopReason::Step;

                for _ in 0..count {
                    last_stop = self.debugger.step()?;
                    if last_stop != DebugStopReason::Step {
                        break;
                    }
                }

                let pc = self.debugger.pc()?;
                let mut out = format_stop_reason(&last_stop, pc);

                // Disassemble next instruction for context
                if let Ok(insns) = self.debugger.disassemble_current(1)
                    && let Some(insn) = insns.first()
                {
                    out.push_str(&format!("\n=> {}", insn.format_line()));
                }

                Ok(ReplOutput::ok(out))
            }
            ReplCommand::Continue => {
                let stop = self.debugger.continue_execution(None)?;
                let pc = self.debugger.pc()?;
                let mut out = format_stop_reason(&stop, pc);

                if let Ok(insns) = self.debugger.disassemble_current(1)
                    && let Some(insn) = insns.first()
                {
                    out.push_str(&format!("\n=> {}", insn.format_line()));
                }

                Ok(ReplOutput::ok(out))
            }
            ReplCommand::Registers => {
                let out = self.format_current_registers()?;
                Ok(ReplOutput::ok(out))
            }
            ReplCommand::Memory { addr, len } => {
                let effective_len = if len == 0 { 16 } else { len };
                let mut buf = vec![0u8; effective_len];
                self.debugger.read_memory(addr, &mut buf)?;
                let dump = format_hex_dump(addr, &buf);
                Ok(ReplOutput::ok(dump))
            }
            ReplCommand::Disassemble { addr, count } => {
                let target_addr = match addr {
                    Some(a) => a,
                    None => self.debugger.pc()?,
                };
                let effective_count = if count == 0 { 5 } else { count };
                let insns = self.debugger.disassemble_at(target_addr, effective_count)?;

                let current_pc = self.debugger.pc().unwrap_or(0);
                let mut out = String::new();
                for insn in insns {
                    let arrow = if insn.address == current_pc {
                        "=> "
                    } else {
                        "   "
                    };
                    out.push_str(&format!("{arrow}{}\n", insn.format_line()));
                }
                Ok(ReplOutput::ok(out))
            }
        }
    }

    /// Read registers, format clean table with changed register indicators (*), and update cache.
    fn format_current_registers(&mut self) -> MelResult<String> {
        let arch = self.debugger.vm().arch();
        let pc = self.debugger.pc()?;
        let mut out = String::new();

        out.push_str(&format!("Architecture: {arch:?} | PC: {pc:#010x}\n\n"));

        let mut current_map = HashMap::new();
        current_map.insert("pc".to_string(), pc);

        let reg_list = get_arch_registers(arch);

        for (name, alias) in &reg_list {
            let val = self
                .debugger
                .read_register(RegisterId::named(*name))
                .unwrap_or(0);
            current_map.insert(name.to_string(), val);

            let changed = match self.prev_registers.get(*name) {
                Some(&prev) => prev != val,
                None => false,
            };

            let mark = if changed { "*" } else { " " };
            let alias_fmt = format!("({alias})");
            out.push_str(&format!(
                "  {mark} {:<4} {:<6}: {:#018x} ({})\n",
                name, alias_fmt, val, val
            ));
        }

        self.prev_registers = current_map;
        Ok(out)
    }

    /// Run interactive REPL loop over input and output streams.
    pub fn run_interactive<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> MelResult<()> {
        writeln!(writer, "=== MEL Interactive Debugger ===")?;
        writeln!(writer, "Type 'help' for commands, 'quit' to exit.\n")?;

        let mut line = String::new();
        loop {
            let pc = self.debugger.pc().unwrap_or(0);
            write!(writer, "mel-dbg [{pc:#010x}]> ")?;
            writer.flush()?;

            line.clear();
            if reader.read_line(&mut line)? == 0 {
                // EOF
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match self.eval_line(trimmed) {
                Ok(output) => {
                    if !output.text.is_empty() {
                        writeln!(writer, "{}", output.text)?;
                    }
                    if output.action == ReplAction::Quit {
                        break;
                    }
                }
                Err(err) => {
                    writeln!(writer, "Error: {err}")?;
                }
            }
        }

        Ok(())
    }
}

/// Format a `DebugStopReason` with execution details.
fn format_stop_reason(reason: &DebugStopReason, pc: Address) -> String {
    match reason {
        DebugStopReason::Step => format!("Single step completed at PC: {pc:#010x}"),
        DebugStopReason::Breakpoint {
            id,
            address,
            hit_count,
        } => format!("Hit breakpoint #{id} at {address:#010x} (hit count: {hit_count})"),
        DebugStopReason::Watchpoint {
            id,
            address,
            size,
            kind,
            accessed_address,
            access_size,
            hit_count,
        } => format!(
            "Hit watchpoint #{id} at {address:#010x} (size: {size} B, kind: {kind:?})\n  Accessed: {accessed_address:#010x} (size: {access_size} B, total hits: {hit_count})"
        ),
        DebugStopReason::CycleLimitReached { cycles } => {
            format!("Cycle limit reached ({cycles} steps executed). Paused at PC: {pc:#010x}")
        }
        DebugStopReason::Halted { reason } => {
            format!("CPU halted: {reason} at PC: {pc:#010x}")
        }
        DebugStopReason::Trap { cause, epc } => {
            format!("CPU trap exception: cause {cause:#x}, epc {epc:#010x}")
        }
    }
}

/// Parse a string into an address or number, supporting hex (`0x...`), decimal, and underscores.
pub fn parse_number(token: &str) -> MelResult<u64> {
    let clean = token.trim().replace('_', "");
    if clean.is_empty() {
        return Err(MelError::Debugger("Empty number argument".to_string()));
    }

    if clean.starts_with("0x") || clean.starts_with("0X") {
        u64::from_str_radix(&clean[2..], 16)
            .map_err(|e| MelError::Debugger(format!("Invalid hexadecimal number '{token}': {e}")))
    } else {
        clean.parse::<u64>().map_err(|e| {
            MelError::Debugger(format!(
                "Invalid decimal or hexadecimal number '{token}': {e}"
            ))
        })
    }
}

/// Format a memory slice into standard 16-byte hex dump rows with ASCII representation.
pub fn format_hex_dump(base_addr: Address, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "(empty memory slice)\n".to_string();
    }

    let mut out = String::new();
    for (chunk_idx, chunk) in bytes.chunks(16).enumerate() {
        let line_addr = base_addr.saturating_add((chunk_idx * 16) as u64);
        let mut hex_part = String::new();
        let mut ascii_part = String::new();

        for i in 0..16 {
            if i == 8 {
                hex_part.push(' ');
            }
            if let Some(&b) = chunk.get(i) {
                hex_part.push_str(&format!("{b:02x} "));
                let ch = if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                };
                ascii_part.push(ch);
            } else {
                hex_part.push_str("   ");
            }
        }

        out.push_str(&format!("{line_addr:#010x}:  {hex_part} |{ascii_part}|\n"));
    }
    out
}

/// Returns architecture register list `(name, alias)` for formatting.
fn get_arch_registers(arch: Architecture) -> Vec<(&'static str, &'static str)> {
    match arch {
        Architecture::Riscv64 => vec![
            ("x0", "zero"),
            ("x1", "ra"),
            ("x2", "sp"),
            ("x3", "gp"),
            ("x4", "tp"),
            ("x5", "t0"),
            ("x6", "t1"),
            ("x7", "t2"),
            ("x8", "s0/fp"),
            ("x9", "s1"),
            ("x10", "a0"),
            ("x11", "a1"),
            ("x12", "a2"),
            ("x13", "a3"),
            ("x14", "a4"),
            ("x15", "a5"),
            ("x16", "a6"),
            ("x17", "a7"),
            ("x18", "s2"),
            ("x19", "s3"),
            ("x20", "s4"),
            ("x21", "s5"),
            ("x22", "s6"),
            ("x23", "s7"),
            ("x24", "s8"),
            ("x25", "s9"),
            ("x26", "s10"),
            ("x27", "s11"),
            ("x28", "t3"),
            ("x29", "t4"),
            ("x30", "t5"),
            ("x31", "t6"),
        ],
        Architecture::Arm64 => vec![
            ("x0", "r0"),
            ("x1", "r1"),
            ("x2", "r2"),
            ("x3", "r3"),
            ("x4", "r4"),
            ("x5", "r5"),
            ("x6", "r6"),
            ("x7", "r7"),
            ("x8", "xr"),
            ("x9", "r9"),
            ("x10", "r10"),
            ("x11", "r11"),
            ("x12", "r12"),
            ("x13", "r13"),
            ("x14", "r14"),
            ("x15", "r15"),
            ("x16", "ip0"),
            ("x17", "ip1"),
            ("x18", "pr"),
            ("x19", "r19"),
            ("x20", "r20"),
            ("x21", "r21"),
            ("x22", "r22"),
            ("x23", "r23"),
            ("x24", "r24"),
            ("x25", "r25"),
            ("x26", "r26"),
            ("x27", "r27"),
            ("x28", "r28"),
            ("x29", "fp"),
            ("x30", "lr"),
            ("sp", "sp"),
        ],
        Architecture::X86_64 => vec![
            ("rax", "rax"),
            ("rbx", "rbx"),
            ("rcx", "rcx"),
            ("rdx", "rdx"),
            ("rsi", "rsi"),
            ("rdi", "rdi"),
            ("rbp", "rbp"),
            ("rsp", "rsp"),
            ("r8", "r8"),
            ("r9", "r9"),
            ("r10", "r10"),
            ("r11", "r11"),
            ("r12", "r12"),
            ("r13", "r13"),
            ("r14", "r14"),
            ("r15", "r15"),
        ],
        Architecture::PowerIso64 => vec![
            ("r0", "r0"),
            ("r1", "sp"),
            ("r2", "toc"),
            ("r3", "arg1"),
            ("r4", "arg2"),
            ("r5", "arg3"),
            ("r6", "arg4"),
            ("r7", "arg5"),
            ("r8", "arg6"),
            ("r9", "arg7"),
            ("r10", "arg8"),
            ("r11", "env"),
            ("r12", "entry"),
            ("r13", "tp"),
            ("r14", "r14"),
            ("r15", "r15"),
            ("r16", "r16"),
            ("r17", "r17"),
            ("r18", "r18"),
            ("r19", "r19"),
            ("r20", "r20"),
            ("r21", "r21"),
            ("r22", "r22"),
            ("r23", "r23"),
            ("r24", "r24"),
            ("r25", "r25"),
            ("r26", "r26"),
            ("r27", "r27"),
            ("r28", "r28"),
            ("r29", "r29"),
            ("r30", "r30"),
            ("r31", "r31"),
        ],
    }
}

/// Help documentation string for REPL commands.
fn format_help_text() -> String {
    r#"MEL Debugger Command Reference:
  step [n], s [n]             - Step single or n instructions (default 1)
  continue, c                 - Continue execution until breakpoint/watchpoint/halt
  regs, r                     - Display architectural registers with changed diffs
  mem <addr> [len], m, x      - Dump memory at <addr> for [len] bytes (hex + ASCII)
  disasm [addr] [count], d, u - Disassemble [count] instructions starting at [addr] (or PC)
  break <addr>, b             - Set software execution breakpoint at <addr>
  tbreak <addr>, tb           - Set one-shot temporary breakpoint at <addr>
  watch <addr> [size] [kind]  - Set memory watchpoint at <addr> (kind: r, w, rw; default w)
  del <id>, delete <id>       - Delete breakpoint or watchpoint with <id>
  info break, bl              - List all registered breakpoints and watchpoints
  reset                       - Reset CPU and virtual machine to power-on state
  help, h, ?                  - Display this help summary
  quit, q, exit               - Terminate debugger session
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_vm::{create_basic_riscv64_vm, create_mock_vm};

    #[test]
    fn test_parse_number_formats() {
        assert_eq!(parse_number("0x80000000").unwrap(), 0x8000_0000);
        assert_eq!(parse_number("0X8000_0000").unwrap(), 0x8000_0000);
        assert_eq!(parse_number("1024").unwrap(), 1024);
        assert_eq!(parse_number("1_000_000").unwrap(), 1_000_000);
        assert!(parse_number("invalid").is_err());
        assert!(parse_number("").is_err());
    }

    #[test]
    fn test_parse_repl_commands() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let dbg = Debugger::new(vm);
        let session = ReplSession::new(dbg);

        assert_eq!(session.parse_command("s").unwrap(), ReplCommand::Step(None));
        assert_eq!(
            session.parse_command("step 5").unwrap(),
            ReplCommand::Step(Some(5))
        );
        assert_eq!(session.parse_command("c").unwrap(), ReplCommand::Continue);
        assert_eq!(session.parse_command("r").unwrap(), ReplCommand::Registers);
        assert_eq!(
            session.parse_command("mem 0x80000000 64").unwrap(),
            ReplCommand::Memory {
                addr: 0x8000_0000,
                len: 64
            }
        );
        assert_eq!(
            session.parse_command("disasm 0x1000 10").unwrap(),
            ReplCommand::Disassemble {
                addr: Some(0x1000),
                count: 10
            }
        );
        assert_eq!(
            session.parse_command("dis").unwrap(),
            ReplCommand::Disassemble {
                addr: None,
                count: 5
            }
        );
        assert_eq!(
            session.parse_command("break 0x80000004").unwrap(),
            ReplCommand::Break { addr: 0x8000_0004 }
        );
        assert_eq!(
            session.parse_command("watch 0x80001000 8 rw").unwrap(),
            ReplCommand::Watch {
                addr: 0x8000_1000,
                size: 8,
                kind: WatchpointKind::ReadWrite
            }
        );
        assert_eq!(
            session.parse_command("del 2").unwrap(),
            ReplCommand::Delete(2)
        );
        assert_eq!(
            session.parse_command("info break").unwrap(),
            ReplCommand::ListBreakpoints
        );
        assert_eq!(session.parse_command("reset").unwrap(), ReplCommand::Reset);
        assert_eq!(session.parse_command("quit").unwrap(), ReplCommand::Quit);
        assert_eq!(session.parse_command("").unwrap(), ReplCommand::Empty);
    }

    #[test]
    fn test_repl_eval_step_and_registers() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let dbg = Debugger::new(vm);
        let mut session = ReplSession::new(dbg);

        // Step single instruction
        let out_step = session.eval_line("step").unwrap();
        assert!(out_step.text.contains("Single step completed"));
        assert_eq!(out_step.action, ReplAction::Continue);

        // Examine registers
        let out_regs = session.eval_line("regs").unwrap();
        assert!(out_regs.text.contains("Architecture: Riscv64"));
        assert!(out_regs.text.contains("PC: 0x80000004"));
    }

    #[test]
    fn test_repl_eval_breakpoints_and_continue() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let dbg = Debugger::new(vm);
        let mut session = ReplSession::new(dbg);

        // Set breakpoint
        let out_bp = session.eval_line("break 0x80000008").unwrap();
        assert!(out_bp.text.contains("Breakpoint #1 set at 0x80000008"));

        // List breakpoints
        let out_list = session.eval_line("info break").unwrap();
        assert!(out_list.text.contains("0x80000008"));

        // Continue until breakpoint
        let out_cont = session.eval_line("continue").unwrap();
        assert!(out_cont.text.contains("Hit breakpoint #1 at 0x80000008"));

        // Delete breakpoint
        let out_del = session.eval_line("del 1").unwrap();
        assert!(out_del.text.contains("Removed breakpoint #1"));
    }

    #[test]
    fn test_repl_hex_dump_formatting() {
        let data = [
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64, 0x21, 0x00, 0x01,
            0x02, 0x03,
        ];
        let dump = format_hex_dump(0x1000, &data);
        assert!(dump.contains("0x00001000:"));
        assert!(dump.contains("48 65 6c 6c 6f 20 57 6f  72 6c 64 21 00 01 02 03"));
        assert!(dump.contains("|Hello World!....|"));
    }

    #[test]
    fn test_repl_riscv64_real_execution() {
        let mut vm = create_basic_riscv64_vm().unwrap();

        // 3 instructions:
        // lui x1, 0x10000
        // addi x2, x0, 88
        // sb x2, 0(x1)
        let code: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0x80, 0x05, 0x23, 0x80, 0x20, 0x00,
        ];
        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        let dbg = Debugger::new(vm);
        let mut session = ReplSession::new(dbg);

        // Check disasm command
        let out_dis = session.eval_line("disasm pc 3").unwrap();
        assert!(out_dis.text.contains("lui"));
        assert!(out_dis.text.contains("addi"));
        assert!(out_dis.text.contains("sb"));

        // Memory dump
        let out_mem = session.eval_line("mem 0x80000000 12").unwrap();
        assert!(out_mem.text.contains("b7 00 00 10 13 01 80 05"));

        // Step 2 instructions
        let out_step = session.eval_line("step 2").unwrap();
        assert!(out_step.text.contains("Single step completed"));

        // Finish step
        session.eval_line("step").unwrap();
        let uart_out = session
            .debugger()
            .vm()
            .uart()
            .unwrap()
            .lock()
            .unwrap()
            .drain_output();
        assert_eq!(uart_out, b"X");
    }

    #[test]
    fn test_repl_interactive_stream_simulation() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let dbg = Debugger::new(vm);
        let mut session = ReplSession::new(dbg);

        let input_script = b"step\nregs\nhelp\nquit\n";
        let mut output_buf = Vec::new();

        session
            .run_interactive(&input_script[..], &mut output_buf)
            .unwrap();

        let output_str = String::from_utf8(output_buf).unwrap();
        assert!(output_str.contains("MEL Interactive Debugger"));
        assert!(output_str.contains("Single step completed"));
        assert!(output_str.contains("Architecture: Riscv64"));
        assert!(output_str.contains("MEL Debugger Command Reference"));
        assert!(output_str.contains("Exiting MEL debugger."));
    }

    #[test]
    fn test_repl_symbol_resolution() {
        let vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let mut dbg = Debugger::new(vm);

        let mut symtab = mel_loader::SymbolTable::new();
        symtab.add_symbol(mel_loader::ElfSymbol {
            name: "target_handler".to_string(),
            address: 0x8000_0050,
            size: 0x20,
            kind: mel_loader::SymbolKind::Function,
            binding: mel_loader::SymbolBinding::Global,
            section_index: Some(1),
        });
        dbg.set_symbols(symtab);

        let mut session = ReplSession::new(dbg);

        // Break at symbol name
        let out_bp = session.eval_line("break target_handler").unwrap();
        assert!(out_bp.text.contains("Breakpoint #1 set at 0x80000050"));

        // Temp break at symbol name
        let out_tbp = session.eval_line("tbreak target_handler").unwrap();
        assert!(
            out_tbp
                .text
                .contains("Temporary breakpoint #2 set at 0x80000050")
        );

        // Disasm at symbol name
        let cmd = session.parse_command("disasm target_handler 4").unwrap();
        assert_eq!(
            cmd,
            ReplCommand::Disassemble {
                addr: Some(0x8000_0050),
                count: 4
            }
        );
    }
}
