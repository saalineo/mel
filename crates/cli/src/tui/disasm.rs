//! Live Disassembly Widget (Multi-Arch).
//!
//! Visualizes disassembled machine instructions centered around the active Program Counter (PC),
//! showing instruction bytes, mnemonics, operands, symbol annotations, and breakpoint markers.

use std::collections::HashSet;

use mel_core::{Address, Architecture};
use mel_debugger::{DisassembledInstruction, disassemble};
use mel_loader::SymbolTable;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

/// Single line in the disassembly listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisasmLine {
    /// Virtual / physical instruction address.
    pub address: Address,
    /// Raw instruction machine code bytes.
    pub bytes: Vec<u8>,
    /// Architecture opcode mnemonic (e.g. `addi`, `mov`, `blr`).
    pub mnemonic: String,
    /// Instruction operands (e.g. `sp, sp, -16`, `rax, [rbp - 8]`).
    pub operands: String,
    /// Resolved symbol label if available (e.g. `<_start>`, `<main+0x14>`).
    pub symbol: Option<String>,
    /// True if this instruction corresponds to the CPU's current PC.
    pub is_pc: bool,
    /// True if an active breakpoint is placed at this address.
    pub has_breakpoint: bool,
}

/// State for the disassembly view including loaded instructions, PC, cursor, and viewport offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisasmState {
    /// Current program counter.
    pub pc: Address,
    /// Selected / focused instruction address.
    pub cursor_address: Option<Address>,
    /// Set of active breakpoint addresses.
    pub breakpoints: HashSet<Address>,
    /// Decoded instruction lines.
    pub instructions: Vec<DisasmLine>,
    /// Top visible instruction index in `instructions`.
    pub scroll_offset: usize,
}

impl DisasmState {
    /// Create a new empty disassembly state.
    pub fn new(pc: Address) -> Self {
        Self {
            pc,
            cursor_address: Some(pc),
            breakpoints: HashSet::new(),
            instructions: Vec::new(),
            scroll_offset: 0,
        }
    }

    /// Update the current PC and refresh `is_pc` flags across lines.
    pub fn set_pc(&mut self, pc: Address) {
        self.pc = pc;
        for line in &mut self.instructions {
            line.is_pc = line.address == pc;
        }
    }

    /// Center the viewport on the current PC given the visible row height.
    pub fn center_on_pc(&mut self, visible_rows: usize) {
        if self.instructions.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        let pc_index = self
            .instructions
            .iter()
            .position(|line| line.address == self.pc)
            .unwrap_or(0);

        let half_height = visible_rows / 2;
        self.scroll_offset = pc_index.saturating_sub(half_height);

        let max_offset = self.instructions.len().saturating_sub(visible_rows);
        if self.scroll_offset > max_offset {
            self.scroll_offset = max_offset;
        }
    }

    /// Scroll up by a number of instructions.
    pub fn scroll_up(&mut self, count: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(count);
    }

    /// Scroll down by a number of instructions.
    pub fn scroll_down(&mut self, count: usize) {
        let max_offset = self.instructions.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + count).min(max_offset);
    }

    /// Jump viewport / cursor to a specific instruction address.
    pub fn goto(&mut self, address: Address) {
        if let Some(pos) = self.instructions.iter().position(|l| l.address == address) {
            self.scroll_offset = pos;
            self.cursor_address = Some(address);
        } else {
            self.cursor_address = Some(address);
        }
    }

    /// Toggle breakpoint at the specified address.
    pub fn toggle_breakpoint(&mut self, address: Address) -> bool {
        let active = if self.breakpoints.contains(&address) {
            self.breakpoints.remove(&address);
            false
        } else {
            self.breakpoints.insert(address);
            true
        };

        for line in &mut self.instructions {
            if line.address == address {
                line.has_breakpoint = active;
            }
        }
        active
    }

    /// Replace active breakpoints with a new collection.
    pub fn set_breakpoints(&mut self, bps: impl IntoIterator<Item = Address>) {
        self.breakpoints = bps.into_iter().collect();
        for line in &mut self.instructions {
            line.has_breakpoint = self.breakpoints.contains(&line.address);
        }
    }

    /// Populate disassembly lines from decoded instructions and optional symbol table.
    pub fn populate(
        &mut self,
        decoded: &[DisassembledInstruction],
        current_pc: Address,
        symbols: Option<&SymbolTable>,
    ) {
        self.pc = current_pc;
        self.instructions.clear();

        for insn in decoded {
            let symbol = symbols.and_then(|sym_tab| {
                sym_tab.resolve_address(insn.address).map(|(sym, offset)| {
                    if offset == 0 {
                        format!("<{}>", sym.name)
                    } else {
                        format!("<{}+{:#x}>", sym.name, offset)
                    }
                })
            });

            self.instructions.push(DisasmLine {
                address: insn.address,
                bytes: insn.bytes.clone(),
                mnemonic: insn.mnemonic.clone(),
                operands: insn.operands.clone(),
                symbol,
                is_pc: insn.address == current_pc,
                has_breakpoint: self.breakpoints.contains(&insn.address),
            });
        }
    }

    /// Disassemble raw byte payload using Capstone and populate state.
    pub fn disassemble_payload(
        &mut self,
        arch: Architecture,
        bytes: &[u8],
        base_addr: Address,
        current_pc: Address,
        symbols: Option<&SymbolTable>,
    ) -> Result<(), mel_core::MelError> {
        let decoded = disassemble(arch, bytes, base_addr)?;
        self.populate(&decoded, current_pc, symbols);
        Ok(())
    }
}

/// TUI Widget for rendering the live disassembly window.
#[derive(Debug, Clone)]
pub struct DisasmWidget<'a> {
    state: &'a DisasmState,
    focused: bool,
}

impl<'a> DisasmWidget<'a> {
    /// Create a disassembly widget referencing the view state.
    pub fn new(state: &'a DisasmState) -> Self {
        Self {
            state,
            focused: false,
        }
    }

    /// Set focused border state.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Helper to render directly into an active Ratatui frame.
    pub fn render_frame(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self.clone(), area);
    }
}

impl Widget for DisasmWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
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

        let title_text = format!(
            " [2] Disassembly (PC: {:#010x}) [{} insns] ",
            self.state.pc,
            self.state.instructions.len()
        );

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

        let bp_style = Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD);
        let pc_arrow_style = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let addr_style = Style::default().fg(Color::Cyan);
        let bytes_style = Style::default().fg(Color::DarkGray);
        let mnemonic_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let operands_style = Style::default().fg(Color::LightCyan);
        let symbol_style = Style::default().fg(Color::Magenta);
        let pc_bg_style = Style::default().bg(Color::Rgb(20, 45, 20));
        let cursor_bg_style = Style::default().bg(Color::Rgb(30, 30, 60));

        let visible_slice = self
            .state
            .instructions
            .iter()
            .skip(self.state.scroll_offset)
            .take(inner.height as usize);

        for (idx, line) in visible_slice.enumerate() {
            let row_y = inner.y + idx as u16;
            let mut spans = Vec::new();

            // 1. Breakpoint marker: '●' or ' '
            if line.has_breakpoint {
                spans.push(Span::styled("● ", bp_style));
            } else {
                spans.push(Span::raw("  "));
            }

            // 2. PC Indicator: '=>' or '  '
            if line.is_pc {
                spans.push(Span::styled("=> ", pc_arrow_style));
            } else {
                spans.push(Span::raw("   "));
            }

            // 3. Address: 0x0000000080000000
            spans.push(Span::styled(
                format!("{:#010x}  ", line.address),
                addr_style,
            ));

            // 4. Hex Bytes (format up to 4 bytes for compactness)
            let hex_bytes = line
                .bytes
                .iter()
                .take(4)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            spans.push(Span::styled(format!("{hex_bytes:<12} "), bytes_style));

            // 5. Symbol Annotation (if present)
            if let Some(ref sym) = line.symbol {
                spans.push(Span::styled(format!("{sym:<16} "), symbol_style));
            }

            // 6. Opcode Mnemonic
            spans.push(Span::styled(
                format!("{:<8} ", line.mnemonic),
                mnemonic_style,
            ));

            // 7. Operands
            spans.push(Span::styled(&line.operands, operands_style));

            let mut rendered_line = Line::from(spans);

            // Apply full-row background style for current PC or active cursor
            if line.is_pc {
                rendered_line = rendered_line.style(pc_bg_style);
            } else if self.state.cursor_address == Some(line.address) {
                rendered_line = rendered_line.style(cursor_bg_style);
            }

            buf.set_line(inner.x, row_y, &rendered_line, inner.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_loader::{ElfSymbol, SymbolBinding, SymbolKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample_instructions() -> Vec<DisassembledInstruction> {
        vec![
            DisassembledInstruction {
                address: 0x8000_0000,
                size: 4,
                bytes: vec![0x13, 0x01, 0x01, 0x00],
                mnemonic: "addi".to_string(),
                operands: "sp, sp, 16".to_string(),
            },
            DisassembledInstruction {
                address: 0x8000_0004,
                size: 4,
                bytes: vec![0x93, 0x02, 0xa0, 0x02],
                mnemonic: "li".to_string(),
                operands: "t0, 42".to_string(),
            },
            DisassembledInstruction {
                address: 0x8000_0008,
                size: 4,
                bytes: vec![0x73, 0x00, 0x00, 0x00],
                mnemonic: "ecall".to_string(),
                operands: String::new(),
            },
            DisassembledInstruction {
                address: 0x8000_000c,
                size: 4,
                bytes: vec![0x6f, 0x00, 0x00, 0x00],
                mnemonic: "j".to_string(),
                operands: "0x8000000c".to_string(),
            },
        ]
    }

    #[test]
    fn test_disassembly_state_pc_centering() {
        let mut state = DisasmState::new(0x8000_0004);
        let insns = sample_instructions();
        state.populate(&insns, 0x8000_0004, None);

        assert_eq!(state.instructions.len(), 4);
        assert!(state.instructions[1].is_pc);
        assert!(!state.instructions[0].is_pc);

        // Center on PC with viewport height of 10 rows (half is 5)
        state.center_on_pc(10);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_disassembly_scrolling_and_goto() {
        let mut state = DisasmState::new(0x8000_0000);
        let insns = sample_instructions();
        state.populate(&insns, 0x8000_0000, None);

        state.scroll_down(2);
        assert_eq!(state.scroll_offset, 2);

        state.scroll_up(1);
        assert_eq!(state.scroll_offset, 1);

        state.goto(0x8000_000c);
        assert_eq!(state.scroll_offset, 3);
        assert_eq!(state.cursor_address, Some(0x8000_000c));
    }

    #[test]
    fn test_breakpoint_markers_and_toggling() {
        let mut state = DisasmState::new(0x8000_0000);
        let insns = sample_instructions();
        state.populate(&insns, 0x8000_0000, None);

        assert!(!state.instructions[0].has_breakpoint);

        let active = state.toggle_breakpoint(0x8000_0000);
        assert!(active);
        assert!(state.instructions[0].has_breakpoint);

        let deactivated = state.toggle_breakpoint(0x8000_0000);
        assert!(!deactivated);
        assert!(!state.instructions[0].has_breakpoint);
    }

    #[test]
    fn test_symbol_annotation_resolution() {
        let mut sym_tab = SymbolTable::new();
        sym_tab.add_symbol(ElfSymbol {
            name: "_start".to_string(),
            address: 0x8000_0000,
            size: 8,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section_index: Some(1),
        });

        let mut state = DisasmState::new(0x8000_0000);
        let insns = sample_instructions();
        state.populate(&insns, 0x8000_0000, Some(&sym_tab));

        assert_eq!(state.instructions[0].symbol.as_deref(), Some("<_start>"));
        assert_eq!(
            state.instructions[1].symbol.as_deref(),
            Some("<_start+0x4>")
        );
        assert_eq!(state.instructions[2].symbol, None);
    }

    #[test]
    fn test_headless_disasm_render() {
        let mut state = DisasmState::new(0x8000_0004);
        let insns = sample_instructions();
        state.populate(&insns, 0x8000_0004, None);
        state.toggle_breakpoint(0x8000_0000);

        let widget = DisasmWidget::new(&state).focused(true);

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
            content.contains("Disassembly"),
            "Buffer missing Disassembly title"
        );
        assert!(content.contains("=>"), "Buffer missing PC arrow marker");
        assert!(content.contains("●"), "Buffer missing breakpoint dot");
        assert!(content.contains("addi"), "Buffer missing addi mnemonic");
        assert!(content.contains("li"), "Buffer missing li mnemonic");
    }
}
