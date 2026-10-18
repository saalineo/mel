//! TUI Debugger Application & Event Loop.
//!
//! Connects keyboard interactions, REPL command execution, multi-quadrant
//! diagnostic rendering, and non-blocking virtual machine execution.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use mel_core::Address;
use mel_debugger::breakpoints::DebugStopReason;
use mel_debugger::{ReplAction, ReplSession};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::disasm::{DisasmState, DisasmWidget};
use crate::tui::layout::TuiLayout;
use crate::tui::memory::{MemoryViewerState, MemoryViewerWidget};
use crate::tui::registers::{RegisterTracker, RegisterWidget};
use crate::tui::terminal::{TerminalState, TerminalWidget};

/// Identified UI quadrant / panel in focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    /// CPU Register & Flag Inspector (Top-Left)
    Registers,
    /// Live Disassembly Listing (Top-Right)
    Disassembly,
    /// Memory Hex & ASCII Viewer (Bottom-Left)
    Memory,
    /// Interactive UART Serial Console (Bottom-Right)
    Uart,
    /// Debugger Command Input Bar (Bottom)
    CommandBar,
}

impl FocusedPane {
    /// Return the next focusable pane in clockwise cycle.
    pub fn next(self) -> Self {
        match self {
            Self::Registers => Self::Disassembly,
            Self::Disassembly => Self::Memory,
            Self::Memory => Self::Uart,
            Self::Uart => Self::CommandBar,
            Self::CommandBar => Self::Registers,
        }
    }

    /// Return the previous focusable pane in counter-clockwise cycle.
    pub fn prev(self) -> Self {
        match self {
            Self::Registers => Self::CommandBar,
            Self::Disassembly => Self::Registers,
            Self::Memory => Self::Disassembly,
            Self::Uart => Self::Memory,
            Self::CommandBar => Self::Uart,
        }
    }
}

/// Central TUI Application State.
pub struct App {
    /// Embedded debugger REPL session.
    pub repl: ReplSession,
    /// Currently focused UI panel.
    pub active_pane: FocusedPane,
    /// Current command string being typed.
    pub command_input: String,
    /// Cursor column index in `command_input`.
    pub command_cursor: usize,
    /// Command history stack.
    pub command_history: Vec<String>,
    /// Active index into history when navigating with Up/Down.
    pub history_index: Option<usize>,
    /// Last status message / execution feedback.
    pub status_message: String,
    /// Display color of status message.
    pub status_color: Color,
    /// True if VM is continuously executing in the background.
    pub is_running: bool,
    /// Set to true to request application exit.
    pub should_quit: bool,
    /// Register state and step diff tracker.
    pub register_tracker: RegisterTracker,
    /// Disassembly panel state.
    pub disasm_state: DisasmState,
    /// Memory viewer panel state.
    pub memory_state: MemoryViewerState,
    /// Interactive UART terminal emulator state.
    pub terminal_state: TerminalState,
    /// Base address of cached memory buffer.
    pub memory_base: Address,
    /// Cached memory byte buffer for display.
    pub memory_buffer: Vec<u8>,
}

impl App {
    /// Create and initialize a new TUI App wrapping a debugger session.
    pub fn new(repl: ReplSession) -> Self {
        let arch = repl.debugger().vm().arch();
        let pc = repl.debugger().vm().pc().unwrap_or(0);
        let ram_base = arch.default_ram_base();

        let mut app = Self {
            repl,
            active_pane: FocusedPane::CommandBar,
            command_input: String::new(),
            command_cursor: 0,
            command_history: Vec::new(),
            history_index: None,
            status_message: "Ready. Press F10 to Step, F5 to Continue, F9 for Breakpoint."
                .to_string(),
            status_color: Color::Cyan,
            is_running: false,
            should_quit: false,
            register_tracker: RegisterTracker::new(arch),
            disasm_state: DisasmState::new(pc),
            memory_state: MemoryViewerState::new(ram_base),
            terminal_state: TerminalState::new(80, 24),
            memory_base: ram_base,
            memory_buffer: vec![0u8; 512],
        };

        app.sync_state();
        app
    }

    /// Synchronize all UI widgets with the current state of the VM and Debugger.
    pub fn sync_state(&mut self) {
        let vm = self.repl.debugger().vm();
        let current_pc = vm.pc().unwrap_or(0);

        // 1. Update Registers Tracker
        self.register_tracker.update_from_vm(vm);

        // 2. Synchronize Disassembly & Breakpoints
        self.disasm_state.pc = current_pc;
        self.disasm_state.breakpoints.clear();
        for bp in self.repl.debugger().breakpoints().breakpoints() {
            if bp.enabled {
                self.disasm_state.breakpoints.insert(bp.address);
            }
        }

        let disasm_base = current_pc.saturating_sub(32);
        if let Ok(insns) = self.repl.debugger().disassemble_at(disasm_base, 32) {
            self.disasm_state
                .populate(&insns, current_pc, Some(self.repl.debugger().symbols()));
        }
        self.disasm_state.center_on_pc(16);

        // 3. Update Memory Hex Viewer Buffer
        let base = self.memory_state.base_address;
        self.memory_base = base;
        self.memory_buffer.resize(512, 0);
        let _ = self
            .repl
            .debugger()
            .vm()
            .read_memory(base, &mut self.memory_buffer);

        // 4. Synchronize UART Output
        if let Some(uart_arc) = self.repl.debugger().vm().uart() {
            if let Ok(mut uart) = uart_arc.lock() {
                self.terminal_state.sync_from_uart(&mut uart);
            }
        }
    }

    /// Handle keyboard event and route to debugger actions or widget controls.
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Global Hotkeys
        match key.code {
            KeyCode::F(5) => {
                self.is_running = true;
                self.status_message =
                    "Execution resumed (running... press ESC to pause)".to_string();
                self.status_color = Color::Green;
                return;
            }
            KeyCode::F(10) => {
                self.step_one();
                return;
            }
            KeyCode::F(9) => {
                self.toggle_breakpoint();
                return;
            }
            KeyCode::Esc => {
                if self.is_running {
                    self.is_running = false;
                    self.status_message = "Execution paused by user.".to_string();
                    self.status_color = Color::Yellow;
                    return;
                } else if self.active_pane == FocusedPane::CommandBar
                    && !self.command_input.is_empty()
                {
                    self.command_input.clear();
                    self.command_cursor = 0;
                    return;
                } else if self.active_pane != FocusedPane::CommandBar {
                    self.active_pane = FocusedPane::CommandBar;
                    return;
                }
            }
            KeyCode::Tab => {
                self.active_pane = self.active_pane.next();
                return;
            }
            KeyCode::BackTab => {
                self.active_pane = self.active_pane.prev();
                return;
            }
            _ => {}
        }

        // Ctrl+C / Ctrl+Q global quit shortcut
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.should_quit = true;
                    return;
                }
                _ => {}
            }
        }

        // Pane-specific keyboard routing
        match self.active_pane {
            FocusedPane::CommandBar => self.handle_command_bar_key(key),
            FocusedPane::Uart => self.handle_uart_key(key),
            FocusedPane::Disassembly => self.handle_disasm_key(key),
            FocusedPane::Memory => self.handle_memory_key(key),
            FocusedPane::Registers => self.handle_registers_key(key),
        }
    }

    fn handle_command_bar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.submit_command();
            }
            KeyCode::Char(c) => {
                self.command_input.insert(self.command_cursor, c);
                self.command_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.command_cursor > 0 {
                    self.command_cursor -= 1;
                    self.command_input.remove(self.command_cursor);
                }
            }
            KeyCode::Delete => {
                if self.command_cursor < self.command_input.len() {
                    self.command_input.remove(self.command_cursor);
                }
            }
            KeyCode::Left => {
                self.command_cursor = self.command_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                if self.command_cursor < self.command_input.len() {
                    self.command_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.command_cursor = 0;
            }
            KeyCode::End => {
                self.command_cursor = self.command_input.len();
            }
            KeyCode::Up => {
                // Previous command in history
                if !self.command_history.is_empty() {
                    let next_idx = match self.history_index {
                        None => self.command_history.len().saturating_sub(1),
                        Some(i) => i.saturating_sub(1),
                    };
                    self.history_index = Some(next_idx);
                    if let Some(cmd) = self.command_history.get(next_idx) {
                        self.command_input = cmd.clone();
                        self.command_cursor = self.command_input.len();
                    }
                }
            }
            KeyCode::Down => {
                // Next command in history
                if let Some(i) = self.history_index {
                    if i + 1 < self.command_history.len() {
                        let next_idx = i + 1;
                        self.history_index = Some(next_idx);
                        if let Some(cmd) = self.command_history.get(next_idx) {
                            self.command_input = cmd.clone();
                            self.command_cursor = self.command_input.len();
                        }
                    } else {
                        self.history_index = None;
                        self.command_input.clear();
                        self.command_cursor = 0;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_uart_key(&mut self, key: KeyEvent) {
        if let Some(uart_arc) = self.repl.debugger_mut().vm_mut().uart() {
            if let Ok(mut uart) = uart_arc.lock() {
                self.terminal_state.forward_key_to_uart(key, &mut uart);
            }
        }
    }

    fn handle_disasm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.disasm_state.scroll_up(1),
            KeyCode::Down => self.disasm_state.scroll_down(1),
            KeyCode::PageUp => self.disasm_state.scroll_up(10),
            KeyCode::PageDown => self.disasm_state.scroll_down(10),
            KeyCode::Char('c') | KeyCode::Char('C') => self.disasm_state.center_on_pc(16),
            _ => {}
        }
    }

    fn handle_memory_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.memory_state.scroll_up(1),
            KeyCode::Down => self.memory_state.scroll_down(1),
            KeyCode::PageUp => self.memory_state.scroll_up(8),
            KeyCode::PageDown => self.memory_state.scroll_down(8),
            _ => {}
        }
        self.sync_state();
    }

    fn handle_registers_key(&mut self, key: KeyEvent) {
        if let KeyCode::Char('r') | KeyCode::Char('R') = key.code {
            self.sync_state();
        }
    }

    /// Submit and evaluate the typed command line.
    pub fn submit_command(&mut self) {
        let trimmed = self.command_input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        self.command_history.push(trimmed.clone());
        self.history_index = None;
        self.command_input.clear();
        self.command_cursor = 0;

        match self.repl.eval_line(&trimmed) {
            Ok(output) => {
                if output.action == ReplAction::Quit {
                    self.should_quit = true;
                    return;
                }
                self.status_message = if output.text.is_empty() {
                    format!("[OK] {}", trimmed)
                } else {
                    output.text.lines().next().unwrap_or("").to_string()
                };
                self.status_color = Color::Green;
            }
            Err(e) => {
                self.status_message = format!("[Error] {}", e);
                self.status_color = Color::LightRed;
            }
        }

        self.sync_state();
    }

    /// Step exactly one instruction and update UI.
    pub fn step_one(&mut self) {
        match self.repl.debugger_mut().step() {
            Ok(reason) => {
                let pc = self.repl.debugger().vm().pc().unwrap_or(0);
                self.status_message = match reason {
                    DebugStopReason::Step => format!("[Step] Stepped to PC: {:#010x}", pc),
                    DebugStopReason::Breakpoint { id, address, .. } => {
                        format!("[Breakpoint] Hit bp #{} at {:#010x}", id, address)
                    }
                    DebugStopReason::Halted { reason } => format!("[Halted] {}", reason),
                    DebugStopReason::Trap { cause, epc } => {
                        format!("[Trap] Cause: {:#x}, EPC: {:#010x}", cause, epc)
                    }
                    other => format!("[Stop] {:?}", other),
                };
                self.status_color = Color::Green;
            }
            Err(e) => {
                self.status_message = format!("[Step Error] {}", e);
                self.status_color = Color::LightRed;
            }
        }
        self.sync_state();
    }

    /// Toggle breakpoint at the current PC address.
    pub fn toggle_breakpoint(&mut self) {
        let pc = self.repl.debugger().vm().pc().unwrap_or(0);
        if self
            .repl
            .debugger()
            .breakpoints()
            .get_breakpoint_at(pc)
            .is_some()
        {
            let removed = self
                .repl
                .debugger_mut()
                .breakpoints_mut()
                .remove_breakpoint_at(pc);
            self.status_message = format!("Removed {} breakpoint(s) at {:#010x}", removed, pc);
            self.status_color = Color::Yellow;
        } else {
            let id = self
                .repl
                .debugger_mut()
                .breakpoints_mut()
                .add_breakpoint(pc);
            self.status_message = format!("Set breakpoint #{} at {:#010x}", id, pc);
            self.status_color = Color::Green;
        }
        self.sync_state();
    }

    /// Advance non-blocking continuous execution slice.
    pub fn tick(&mut self, slice_steps: u64) {
        if self.is_running {
            match self
                .repl
                .debugger_mut()
                .continue_execution(Some(slice_steps))
            {
                Ok(reason) => match reason {
                    DebugStopReason::Step | DebugStopReason::CycleLimitReached { .. } => {
                        // Keep running
                    }
                    DebugStopReason::Breakpoint { id, address, .. } => {
                        self.is_running = false;
                        self.status_message =
                            format!("[Breakpoint] Hit bp #{} at {:#010x}", id, address);
                        self.status_color = Color::Yellow;
                    }
                    DebugStopReason::Halted { reason } => {
                        self.is_running = false;
                        self.status_message = format!("[Halted] {}", reason);
                        self.status_color = Color::LightRed;
                    }
                    DebugStopReason::Trap { cause, epc } => {
                        self.is_running = false;
                        self.status_message =
                            format!("[Trap] Cause: {:#x}, EPC: {:#010x}", cause, epc);
                        self.status_color = Color::LightRed;
                    }
                    other => {
                        self.is_running = false;
                        self.status_message = format!("[Stopped] {:?}", other);
                        self.status_color = Color::Yellow;
                    }
                },
                Err(e) => {
                    self.is_running = false;
                    self.status_message = format!("[Exec Error] {}", e);
                    self.status_color = Color::LightRed;
                }
            }
            self.sync_state();
        } else {
            // Drain UART even when paused
            if let Some(uart_arc) = self.repl.debugger().vm().uart() {
                if let Ok(mut uart) = uart_arc.lock() {
                    self.terminal_state.sync_from_uart(&mut uart);
                }
            }
        }
    }

    /// Render all TUI panels, widgets, and the interactive command bar.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let layout = TuiLayout::compute(area);

        // 1. Top-Left: CPU Registers
        let reg_snapshot = self.register_tracker.snapshot();
        let reg_widget =
            RegisterWidget::new(&reg_snapshot).focused(self.active_pane == FocusedPane::Registers);
        frame.render_widget(reg_widget, layout.registers);

        // 2. Top-Right: Disassembly
        let disasm_widget = DisasmWidget::new(&self.disasm_state)
            .focused(self.active_pane == FocusedPane::Disassembly);
        frame.render_widget(disasm_widget, layout.disassembly);

        // 3. Bottom-Left: Memory Hex Viewer
        let mem_widget =
            MemoryViewerWidget::new(&self.memory_state, self.memory_base, &self.memory_buffer)
                .focused(self.active_pane == FocusedPane::Memory);
        frame.render_widget(mem_widget, layout.memory);

        // 4. Bottom-Right: UART Console
        let term_widget = TerminalWidget::new(&self.terminal_state)
            .focused(self.active_pane == FocusedPane::Uart);
        frame.render_widget(term_widget, layout.uart_console);

        // 5. Bottom: Command Bar & Controls
        self.render_command_bar(frame, layout.command_bar);
    }

    fn render_command_bar(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.active_pane == FocusedPane::CommandBar;
        let border_color = if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let title_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(" [5] Command Bar & Status ", title_style));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);

        // Line 1: Interactive Prompt
        let prompt_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let text_style = Style::default().fg(Color::White);

        let mut prompt_spans = vec![
            Span::styled("(dbg)> ", prompt_style),
            Span::styled(&self.command_input, text_style),
        ];

        if is_focused {
            prompt_spans.push(Span::styled(
                "█",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            ));
        }

        let prompt_line = Line::from(prompt_spans);
        frame.render_widget(Paragraph::new(prompt_line), chunks[0]);

        // Line 2: Status Message + Hotkey legend
        if chunks.len() > 1 {
            let key_style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            let action_style = Style::default().fg(Color::DarkGray);
            let status_style = Style::default().fg(self.status_color);

            let status_line = Line::from(vec![
                Span::styled(&self.status_message, status_style),
                Span::raw("  |  "),
                Span::styled("F5", key_style),
                Span::styled(" Cont ", action_style),
                Span::styled("F10", key_style),
                Span::styled(" Step ", action_style),
                Span::styled("F9", key_style),
                Span::styled(" Bp ", action_style),
                Span::styled("Tab", key_style),
                Span::styled(" Pane ", action_style),
                Span::styled("ESC", key_style),
                Span::styled(" Pause", action_style),
            ]);
            frame.render_widget(Paragraph::new(status_line), chunks[1]);
        }
    }

    /// Run the interactive TUI event loop on a Ratatui terminal instance.
    pub fn run_event_loop<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
        tick_rate: Duration,
    ) -> std::io::Result<()> {
        let mut last_tick = Instant::now();

        while !self.should_quit {
            terminal.draw(|f| self.render(f))?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = crossterm::event::read()? {
                    self.handle_key(key);
                }
            }

            if last_tick.elapsed() >= tick_rate {
                self.tick(1000);
                last_tick = Instant::now();
            }
        }

        Ok(())
    }
}
