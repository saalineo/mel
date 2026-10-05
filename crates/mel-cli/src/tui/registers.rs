//! CPU Register and Status Flag Viewer Widget (Multi-Arch).
//!
//! Provides multi-architecture register state formatting, flag bitfield decoding,
//! difference tracking between execution steps, and Ratatui rendering with color highlights.

use std::collections::HashMap;

use mel_core::{Architecture, RegisterId};
use mel_vm::VirtualMachine;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};
use ratatui::Frame;

/// Architectural register definition with canonical name and optional ABI alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegDef {
    pub name: &'static str,
    pub alias: Option<&'static str>,
}

const fn reg(name: &'static str, alias: &'static str) -> RegDef {
    RegDef {
        name,
        alias: Some(alias),
    }
}

const fn reg_no_alias(name: &'static str) -> RegDef {
    RegDef { name, alias: None }
}

/// Static register definitions for RISC-V 64.
pub static RISCV64_REGS: &[RegDef] = &[
    reg("x0", "zero"),
    reg("x1", "ra"),
    reg("x2", "sp"),
    reg("x3", "gp"),
    reg("x4", "tp"),
    reg("x5", "t0"),
    reg("x6", "t1"),
    reg("x7", "t2"),
    reg("x8", "s0/fp"),
    reg("x9", "s1"),
    reg("x10", "a0"),
    reg("x11", "a1"),
    reg("x12", "a2"),
    reg("x13", "a3"),
    reg("x14", "a4"),
    reg("x15", "a5"),
    reg("x16", "a6"),
    reg("x17", "a7"),
    reg("x18", "s2"),
    reg("x19", "s3"),
    reg("x20", "s4"),
    reg("x21", "s5"),
    reg("x22", "s6"),
    reg("x23", "s7"),
    reg("x24", "s8"),
    reg("x25", "s9"),
    reg("x26", "s10"),
    reg("x27", "s11"),
    reg("x28", "t3"),
    reg("x29", "t4"),
    reg("x30", "t5"),
    reg("x31", "t6"),
];

/// Static register definitions for ARM64 (AArch64).
pub static ARM64_REGS: &[RegDef] = &[
    reg("x0", "r0"),
    reg("x1", "r1"),
    reg("x2", "r2"),
    reg("x3", "r3"),
    reg("x4", "r4"),
    reg("x5", "r5"),
    reg("x6", "r6"),
    reg("x7", "r7"),
    reg("x8", "xr"),
    reg("x9", "r9"),
    reg("x10", "r10"),
    reg("x11", "r11"),
    reg("x12", "r12"),
    reg("x13", "r13"),
    reg("x14", "r14"),
    reg("x15", "r15"),
    reg("x16", "ip0"),
    reg("x17", "ip1"),
    reg("x18", "pr"),
    reg("x19", "r19"),
    reg("x20", "r20"),
    reg("x21", "r21"),
    reg("x22", "r22"),
    reg("x23", "r23"),
    reg("x24", "r24"),
    reg("x25", "r25"),
    reg("x26", "r26"),
    reg("x27", "r27"),
    reg("x28", "r28"),
    reg("x29", "fp"),
    reg("x30", "lr"),
];

/// Static register definitions for x86-64.
pub static X86_64_REGS: &[RegDef] = &[
    reg_no_alias("rax"),
    reg_no_alias("rbx"),
    reg_no_alias("rcx"),
    reg_no_alias("rdx"),
    reg_no_alias("rsi"),
    reg_no_alias("rdi"),
    reg_no_alias("rbp"),
    reg_no_alias("rsp"),
    reg_no_alias("r8"),
    reg_no_alias("r9"),
    reg_no_alias("r10"),
    reg_no_alias("r11"),
    reg_no_alias("r12"),
    reg_no_alias("r13"),
    reg_no_alias("r14"),
    reg_no_alias("r15"),
];

/// Static register definitions for Power ISA 64.
pub static POWERPC64_REGS: &[RegDef] = &[
    reg_no_alias("r0"),
    reg("r1", "sp"),
    reg("r2", "toc"),
    reg("r3", "arg1"),
    reg("r4", "arg2"),
    reg("r5", "arg3"),
    reg("r6", "arg4"),
    reg("r7", "arg5"),
    reg("r8", "arg6"),
    reg("r9", "arg7"),
    reg("r10", "arg8"),
    reg("r11", "env"),
    reg("r12", "entry"),
    reg("r13", "tp"),
    reg_no_alias("r14"),
    reg_no_alias("r15"),
    reg_no_alias("r16"),
    reg_no_alias("r17"),
    reg_no_alias("r18"),
    reg_no_alias("r19"),
    reg_no_alias("r20"),
    reg_no_alias("r21"),
    reg_no_alias("r22"),
    reg_no_alias("r23"),
    reg_no_alias("r24"),
    reg_no_alias("r25"),
    reg_no_alias("r26"),
    reg_no_alias("r27"),
    reg_no_alias("r28"),
    reg_no_alias("r29"),
    reg_no_alias("r30"),
    reg_no_alias("r31"),
];

/// Returns the register definition list for a given CPU architecture.
pub fn get_arch_registers(arch: Architecture) -> &'static [RegDef] {
    match arch {
        Architecture::Riscv64 => RISCV64_REGS,
        Architecture::Arm64 => ARM64_REGS,
        Architecture::X86_64 => X86_64_REGS,
        Architecture::PowerIso64 => POWERPC64_REGS,
    }
}

/// Returns the primary stack pointer register name for an architecture.
pub fn get_arch_sp_name(arch: Architecture) -> &'static str {
    match arch {
        Architecture::Riscv64 => "sp",
        Architecture::Arm64 => "sp",
        Architecture::X86_64 => "rsp",
        Architecture::PowerIso64 => "r1",
    }
}

/// Returns the primary status flags register name for an architecture.
pub fn get_arch_flags_name(arch: Architecture) -> &'static str {
    match arch {
        Architecture::Riscv64 => "mstatus",
        Architecture::Arm64 => "nzcv",
        Architecture::X86_64 => "rflags",
        Architecture::PowerIso64 => "msr",
    }
}

/// Decoded status flag item `(label, is_active)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFlag {
    pub name: &'static str,
    pub active: bool,
}

/// Decode architectural flag register bits into human-readable flags.
pub fn decode_flags(arch: Architecture, flags_val: u64) -> Vec<DecodedFlag> {
    match arch {
        Architecture::Riscv64 => vec![
            DecodedFlag {
                name: "MIE",
                active: (flags_val & 0x8) != 0,
            },
            DecodedFlag {
                name: "MPIE",
                active: (flags_val & 0x80) != 0,
            },
            DecodedFlag {
                name: "MPP_M",
                active: ((flags_val >> 11) & 0x3) == 3,
            },
        ],
        Architecture::Arm64 => vec![
            DecodedFlag {
                name: "N",
                active: ((flags_val >> 31) & 1) != 0,
            },
            DecodedFlag {
                name: "Z",
                active: ((flags_val >> 30) & 1) != 0,
            },
            DecodedFlag {
                name: "C",
                active: ((flags_val >> 29) & 1) != 0,
            },
            DecodedFlag {
                name: "V",
                active: ((flags_val >> 28) & 1) != 0,
            },
        ],
        Architecture::X86_64 => vec![
            DecodedFlag {
                name: "CF",
                active: (flags_val & 0x1) != 0,
            },
            DecodedFlag {
                name: "ZF",
                active: (flags_val & 0x40) != 0,
            },
            DecodedFlag {
                name: "SF",
                active: (flags_val & 0x80) != 0,
            },
            DecodedFlag {
                name: "OF",
                active: (flags_val & 0x800) != 0,
            },
            DecodedFlag {
                name: "IF",
                active: (flags_val & 0x200) != 0,
            },
        ],
        Architecture::PowerIso64 => vec![
            DecodedFlag {
                name: "EE",
                active: (flags_val & 0x8000) != 0,
            },
            DecodedFlag {
                name: "PR",
                active: (flags_val & 0x4000) != 0,
            },
            DecodedFlag {
                name: "SF",
                active: ((flags_val >> 63) & 1) != 0,
            },
        ],
    }
}

/// An entry representing a formatted register value with change status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterEntry {
    pub name: String,
    pub alias: Option<String>,
    pub value: u64,
    pub changed: bool,
}

/// Snapshot of full register state prepared for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterSnapshot {
    pub arch: Architecture,
    pub pc: u64,
    pub pc_changed: bool,
    pub sp: u64,
    pub sp_changed: bool,
    pub flags_name: &'static str,
    pub flags_val: u64,
    pub flags_changed: bool,
    pub decoded_flags: Vec<DecodedFlag>,
    pub gprs: Vec<RegisterEntry>,
}

/// State tracker that records register diffs between execution steps.
#[derive(Debug, Clone)]
pub struct RegisterTracker {
    pub arch: Architecture,
    pub current: HashMap<String, u64>,
    pub previous: HashMap<String, u64>,
}

impl RegisterTracker {
    /// Create a new tracker for the given architecture.
    pub fn new(arch: Architecture) -> Self {
        Self {
            arch,
            current: HashMap::new(),
            previous: HashMap::new(),
        }
    }

    /// Update the tracker with fresh values directly from a VirtualMachine instance.
    pub fn update_from_vm(&mut self, vm: &VirtualMachine) {
        let mut new_vals = HashMap::new();

        let pc = vm.pc().unwrap_or(0);
        new_vals.insert("pc".to_string(), pc);

        let sp_name = get_arch_sp_name(self.arch);
        let sp = vm
            .read_register(RegisterId::named(sp_name))
            .unwrap_or(0);
        new_vals.insert(sp_name.to_string(), sp);

        let flags_name = get_arch_flags_name(self.arch);
        let flags_val = vm
            .read_register(RegisterId::named(flags_name))
            .unwrap_or(0);
        new_vals.insert(flags_name.to_string(), flags_val);

        for def in get_arch_registers(self.arch) {
            let val = vm
                .read_register(RegisterId::named(def.name))
                .unwrap_or(0);
            new_vals.insert(def.name.to_string(), val);
        }

        self.previous = std::mem::replace(&mut self.current, new_vals);
    }

    /// Update the tracker using an explicit key-value map (useful for test fixtures).
    pub fn update_from_map(&mut self, map: HashMap<String, u64>) {
        self.previous = std::mem::replace(&mut self.current, map);
    }

    /// Generate a display snapshot with change markers.
    pub fn snapshot(&self) -> RegisterSnapshot {
        let pc = self.current.get("pc").copied().unwrap_or(0);
        let pc_prev = self.previous.get("pc").copied();
        let pc_changed = pc_prev.is_some_and(|p| p != pc);

        let sp_name = get_arch_sp_name(self.arch);
        let sp = self.current.get(sp_name).copied().unwrap_or(0);
        let sp_prev = self.previous.get(sp_name).copied();
        let sp_changed = sp_prev.is_some_and(|p| p != sp);

        let flags_name = get_arch_flags_name(self.arch);
        let flags_val = self.current.get(flags_name).copied().unwrap_or(0);
        let flags_prev = self.previous.get(flags_name).copied();
        let flags_changed = flags_prev.is_some_and(|p| p != flags_val);

        let decoded_flags = decode_flags(self.arch, flags_val);

        let mut gprs = Vec::new();
        for def in get_arch_registers(self.arch) {
            let val = self.current.get(def.name).copied().unwrap_or(0);
            let prev_val = self.previous.get(def.name).copied();
            let changed = prev_val.is_some_and(|p| p != val);

            gprs.push(RegisterEntry {
                name: def.name.to_string(),
                alias: def.alias.map(|s| s.to_string()),
                value: val,
                changed,
            });
        }

        RegisterSnapshot {
            arch: self.arch,
            pc,
            pc_changed,
            sp,
            sp_changed,
            flags_name,
            flags_val,
            flags_changed,
            decoded_flags,
            gprs,
        }
    }
}

/// TUI Widget for rendering multi-arch registers with color highlights.
#[derive(Debug, Clone)]
pub struct RegisterWidget<'a> {
    snapshot: &'a RegisterSnapshot,
    focused: bool,
}

impl<'a> RegisterWidget<'a> {
    /// Create a new register widget wrapping a state snapshot.
    pub fn new(snapshot: &'a RegisterSnapshot) -> Self {
        Self {
            snapshot,
            focused: false,
        }
    }

    /// Configure active focus outline border.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Helper to render directly using an active Ratatui frame.
    pub fn render_frame(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self.clone(), area);
    }
}

impl Widget for RegisterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        let border_color = if self.focused {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let title_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let title_text = format!(" [1] CPU Registers [{:?}] ", self.snapshot.arch);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title_text, title_style));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let changed_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let normal_val_style = Style::default().fg(Color::White);
        let label_style = Style::default().fg(Color::Cyan);
        let alias_style = Style::default().fg(Color::DarkGray);
        let dim_style = Style::default().fg(Color::DarkGray);
        let flag_on_style = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let flag_off_style = Style::default().fg(Color::DarkGray);

        let mut current_y = inner.y;
        let max_y = inner.y + inner.height;

        // Line 1: PC and SP
        if current_y < max_y {
            let pc_style = if self.snapshot.pc_changed {
                changed_style
            } else {
                normal_val_style
            };
            let sp_style = if self.snapshot.sp_changed {
                changed_style
            } else {
                normal_val_style
            };

            let line = Line::from(vec![
                Span::styled(" PC: ", label_style),
                Span::styled(format!("{:#018x}  ", self.snapshot.pc), pc_style),
                Span::styled("SP: ", label_style),
                Span::styled(format!("{:#018x}", self.snapshot.sp), sp_style),
            ]);
            buf.set_line(inner.x, current_y, &line, inner.width);
            current_y += 1;
        }

        // Line 2: Status Flags
        if current_y < max_y {
            let mut flag_spans = vec![
                Span::styled(format!(" {}: ", self.snapshot.flags_name), label_style),
                Span::styled(
                    format!("{:#010x} [", self.snapshot.flags_val),
                    if self.snapshot.flags_changed {
                        changed_style
                    } else {
                        dim_style
                    },
                ),
            ];

            for (i, flag) in self.snapshot.decoded_flags.iter().enumerate() {
                if i > 0 {
                    flag_spans.push(Span::styled(" ", dim_style));
                }
                let style = if flag.active {
                    flag_on_style
                } else {
                    flag_off_style
                };
                flag_spans.push(Span::styled(flag.name, style));
            }
            flag_spans.push(Span::styled("]", dim_style));

            let line = Line::from(flag_spans);
            buf.set_line(inner.x, current_y, &line, inner.width);
            current_y += 1;
        }

        // Line 3: Blank separator if room
        if current_y < max_y && inner.height >= 8 {
            current_y += 1;
        }

        // GPR Grid: 2 columns if width >= 56, otherwise 1 column
        let num_cols = if inner.width >= 56 { 2 } else { 1 };
        let gprs = &self.snapshot.gprs;
        let total = gprs.len();

        let rows = if num_cols == 2 {
            total.div_ceil(2)
        } else {
            total
        };

        for r in 0..rows {
            if current_y >= max_y {
                break;
            }

            let mut row_spans = Vec::new();

            // Column 1
            let idx1 = r;
            if idx1 < total {
                let entry = &gprs[idx1];
                let val_style = if entry.changed {
                    changed_style
                } else {
                    normal_val_style
                };
                let mark = if entry.changed { "*" } else { " " };
                let alias_str = entry
                    .alias
                    .as_deref()
                    .map(|a| format!("({a:<5})"))
                    .unwrap_or_else(|| "       ".to_string());

                row_spans.push(Span::styled(mark, changed_style));
                row_spans.push(Span::styled(format!("{:<4}", entry.name), label_style));
                row_spans.push(Span::styled(format!("{alias_str} "), alias_style));
                row_spans.push(Span::styled(
                    format!("{:#018x}", entry.value),
                    val_style,
                ));
            }

            // Column 2
            if num_cols == 2 {
                let idx2 = r + rows;
                if idx2 < total {
                    let entry = &gprs[idx2];
                    let val_style = if entry.changed {
                        changed_style
                    } else {
                        normal_val_style
                    };
                    let mark = if entry.changed { "*" } else { " " };
                    let alias_str = entry
                        .alias
                        .as_deref()
                        .map(|a| format!("({a:<5})"))
                        .unwrap_or_else(|| "       ".to_string());

                    row_spans.push(Span::raw("  "));
                    row_spans.push(Span::styled(mark, changed_style));
                    row_spans.push(Span::styled(format!("{:<4}", entry.name), label_style));
                    row_spans.push(Span::styled(format!("{alias_str} "), alias_style));
                    row_spans.push(Span::styled(
                        format!("{:#018x}", entry.value),
                        val_style,
                    ));
                }
            }

            let line = Line::from(row_spans);
            buf.set_line(inner.x, current_y, &line, inner.width);
            current_y += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_register_definitions_all_architectures() {
        assert_eq!(get_arch_registers(Architecture::Riscv64).len(), 32);
        assert_eq!(get_arch_registers(Architecture::Arm64).len(), 31);
        assert_eq!(get_arch_registers(Architecture::X86_64).len(), 16);
        assert_eq!(get_arch_registers(Architecture::PowerIso64).len(), 32);

        assert_eq!(get_arch_sp_name(Architecture::Riscv64), "sp");
        assert_eq!(get_arch_sp_name(Architecture::Arm64), "sp");
        assert_eq!(get_arch_sp_name(Architecture::X86_64), "rsp");
        assert_eq!(get_arch_sp_name(Architecture::PowerIso64), "r1");

        assert_eq!(get_arch_flags_name(Architecture::Riscv64), "mstatus");
        assert_eq!(get_arch_flags_name(Architecture::Arm64), "nzcv");
        assert_eq!(get_arch_flags_name(Architecture::X86_64), "rflags");
        assert_eq!(get_arch_flags_name(Architecture::PowerIso64), "msr");
    }

    #[test]
    fn test_flag_decoding_multi_arch() {
        // RISC-V MIE and MPIE and MPP=3
        let rv_flags = decode_flags(Architecture::Riscv64, 0x1888);
        assert!(rv_flags.iter().any(|f| f.name == "MIE" && f.active));
        assert!(rv_flags.iter().any(|f| f.name == "MPIE" && f.active));
        assert!(rv_flags.iter().any(|f| f.name == "MPP_M" && f.active));

        // ARM64 NZCV: N=1, Z=0, C=1, V=0 (0xA000_0000)
        let arm_flags = decode_flags(Architecture::Arm64, 0xA000_0000);
        assert!(arm_flags.iter().any(|f| f.name == "N" && f.active));
        assert!(arm_flags.iter().any(|f| f.name == "Z" && !f.active));
        assert!(arm_flags.iter().any(|f| f.name == "C" && f.active));
        assert!(arm_flags.iter().any(|f| f.name == "V" && !f.active));

        // x86-64 RFLAGS: ZF=1 (0x40), CF=1 (0x1)
        let x86_flags = decode_flags(Architecture::X86_64, 0x41);
        assert!(x86_flags.iter().any(|f| f.name == "ZF" && f.active));
        assert!(x86_flags.iter().any(|f| f.name == "CF" && f.active));
        assert!(x86_flags.iter().any(|f| f.name == "SF" && !f.active));

        // Power ISA: EE=1 (0x8000), SF=1 (bit 63)
        let ppc_flags = decode_flags(Architecture::PowerIso64, 0x8000_0000_0000_8000);
        assert!(ppc_flags.iter().any(|f| f.name == "EE" && f.active));
        assert!(ppc_flags.iter().any(|f| f.name == "SF" && f.active));
        assert!(ppc_flags.iter().any(|f| f.name == "PR" && !f.active));
    }

    #[test]
    fn test_register_diff_tracker() {
        let mut tracker = RegisterTracker::new(Architecture::Riscv64);

        let mut step1 = HashMap::new();
        step1.insert("pc".to_string(), 0x8000_0000);
        step1.insert("sp".to_string(), 0x8001_0000);
        step1.insert("x1".to_string(), 100);
        step1.insert("x2".to_string(), 0x8001_0000);
        tracker.update_from_map(step1);

        let snap1 = tracker.snapshot();
        assert_eq!(snap1.pc, 0x8000_0000);
        assert!(!snap1.pc_changed); // No previous state, so no diff

        // Step 2 with modified PC and x1
        let mut step2 = HashMap::new();
        step2.insert("pc".to_string(), 0x8000_0004);
        step2.insert("sp".to_string(), 0x8001_0000);
        step2.insert("x1".to_string(), 200);
        step2.insert("x2".to_string(), 0x8001_0000);
        tracker.update_from_map(step2);

        let snap2 = tracker.snapshot();
        assert_eq!(snap2.pc, 0x8000_0004);
        assert!(snap2.pc_changed);
        assert!(!snap2.sp_changed);

        let x1_entry = snap2.gprs.iter().find(|e| e.name == "x1").unwrap();
        assert_eq!(x1_entry.value, 200);
        assert!(x1_entry.changed);

        let x2_entry = snap2.gprs.iter().find(|e| e.name == "x2").unwrap();
        assert_eq!(x2_entry.value, 0x8001_0000);
        assert!(!x2_entry.changed);
    }

    #[test]
    fn test_register_widget_headless_render() {
        let mut tracker = RegisterTracker::new(Architecture::Riscv64);
        let mut vals = HashMap::new();
        vals.insert("pc".to_string(), 0x8000_0000);
        vals.insert("sp".to_string(), 0x8001_0000);
        vals.insert("mstatus".to_string(), 0x1800);
        vals.insert("x10".to_string(), 0xCAFE_BABE);
        tracker.update_from_map(vals);

        let snap = tracker.snapshot();
        let widget = RegisterWidget::new(&snap).focused(true);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                widget.render_frame(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        assert!(
            content.contains("CPU Registers"),
            "Buffer missing registers title"
        );
        assert!(
            content.contains("80000000"),
            "Buffer missing PC address in hex"
        );
        assert!(
            content.contains("mstatus"),
            "Buffer missing flags register name"
        );
        assert!(
            content.contains("cafebabe"),
            "Buffer missing x10 value in hex"
        );
    }
}
