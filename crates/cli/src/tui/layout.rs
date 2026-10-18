//! 4-Quadrant + Bottom Bar TUI Terminal Layout.
//!
//! Partitions terminal screens into four primary diagnostic panels and an
//! interactive command / hotkey control bar.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Computed geometry for all panels in the MEL TUI interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiLayout {
    /// Top-Left panel: CPU general registers, program counter, and status flags.
    pub registers: Rect,
    /// Top-Right panel: Live disassembly listing centered on the active PC.
    pub disassembly: Rect,
    /// Bottom-Left panel: Hexadecimal memory dump viewer.
    pub memory: Rect,
    /// Bottom-Right panel: Interactive UART console and execution log.
    pub uart_console: Rect,
    /// Bottom bar: Command input prompt and hotkey legend.
    pub command_bar: Rect,
}

impl TuiLayout {
    /// Compute panel rectangles for a given terminal viewport.
    pub fn compute(area: Rect) -> Self {
        if area.width == 0 || area.height == 0 {
            return Self {
                registers: Rect::default(),
                disassembly: Rect::default(),
                memory: Rect::default(),
                uart_console: Rect::default(),
                command_bar: Rect::default(),
            };
        }

        // Vertical split: Main 4-quadrant area vs Bottom Command/Hotkey bar
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(3)])
            .split(area);

        let main_area = vertical_chunks[0];
        let command_bar = vertical_chunks[1];

        // Split main area vertically into top and bottom halves (50% / 50%)
        let row_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_area);

        let top_row = row_chunks[0];
        let bottom_row = row_chunks[1];

        // Top row: Registers (50%) | Disassembly (50%)
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(top_row);

        let registers = top_chunks[0];
        let disassembly = top_chunks[1];

        // Bottom row: Memory Hex Viewer (50%) | UART Console (50%)
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(bottom_row);

        let memory = bottom_chunks[0];
        let uart_console = bottom_chunks[1];

        Self {
            registers,
            disassembly,
            memory,
            uart_console,
            command_bar,
        }
    }

    /// Return all four diagnostic quadrant rectangles: `[registers, disassembly, memory, uart_console]`.
    pub fn quadrants(&self) -> [Rect; 4] {
        [
            self.registers,
            self.disassembly,
            self.memory,
            self.uart_console,
        ]
    }
}

/// Render a wireframe skeleton of the 4-quadrant TUI layout with panel titles and hotkey labels.
pub fn render_layout_skeleton(frame: &mut Frame, layout: &TuiLayout) {
    let border_style = Style::default().fg(Color::DarkGray);
    let title_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    // 1. Registers Quadrant
    let reg_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(" [1] CPU Registers & Flags ", title_style));
    frame.render_widget(reg_block, layout.registers);

    // 2. Disassembly Quadrant
    let dis_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(" [2] Disassembly (PC) ", title_style));
    frame.render_widget(dis_block, layout.disassembly);

    // 3. Memory Hex Viewer Quadrant
    let mem_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(" [3] Memory Hex Viewer ", title_style));
    frame.render_widget(mem_block, layout.memory);

    // 4. UART Console Quadrant
    let uart_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(" [4] UART Console & Log ", title_style));
    frame.render_widget(uart_block, layout.uart_console);

    // 5. Command & Hotkey Bar
    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let action_style = Style::default().fg(Color::White);
    let sep_style = Style::default().fg(Color::DarkGray);

    let hotkey_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("F5", key_style),
        Span::styled(" Continue  ", action_style),
        Span::styled("|  ", sep_style),
        Span::styled("F10", key_style),
        Span::styled(" Step  ", action_style),
        Span::styled("|  ", sep_style),
        Span::styled("F9", key_style),
        Span::styled(" Breakpoint  ", action_style),
        Span::styled("|  ", sep_style),
        Span::styled("Tab", key_style),
        Span::styled(" Pane  ", action_style),
        Span::styled("|  ", sep_style),
        Span::styled("Esc/q", key_style),
        Span::styled(" Quit", action_style),
    ]);

    let cmd_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(" Controls ", title_style));

    let cmd_paragraph = Paragraph::new(hotkey_line).block(cmd_block);
    frame.render_widget(cmd_paragraph, layout.command_bar);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_layout_computation_standard_terminal() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = TuiLayout::compute(area);

        // Command bar occupies bottom 3 rows
        assert_eq!(layout.command_bar.x, 0);
        assert_eq!(layout.command_bar.y, 37);
        assert_eq!(layout.command_bar.width, 120);
        assert_eq!(layout.command_bar.height, 3);

        // Main area is 37 rows high, top half gets 18 or 19 rows
        assert_eq!(layout.registers.x, 0);
        assert_eq!(layout.registers.y, 0);
        assert_eq!(layout.registers.width, 60);

        assert_eq!(layout.disassembly.x, 60);
        assert_eq!(layout.disassembly.y, 0);
        assert_eq!(layout.disassembly.width, 60);

        assert_eq!(layout.memory.x, 0);
        assert_eq!(layout.memory.y, layout.registers.height);
        assert_eq!(layout.memory.width, 60);

        assert_eq!(layout.uart_console.x, 60);
        assert_eq!(layout.uart_console.y, layout.disassembly.height);
        assert_eq!(layout.uart_console.width, 60);

        // Total vertical height checks
        assert_eq!(
            layout.registers.height + layout.memory.height + layout.command_bar.height,
            40
        );
    }

    #[test]
    fn test_layout_computation_zero_or_minimal_area() {
        let zero_layout = TuiLayout::compute(Rect::default());
        assert_eq!(zero_layout.registers.width, 0);
        assert_eq!(zero_layout.command_bar.height, 0);

        let small_layout = TuiLayout::compute(Rect::new(0, 0, 20, 5));
        assert!(small_layout.command_bar.height > 0);
        assert_eq!(small_layout.command_bar.width, 20);
    }

    #[test]
    fn test_layout_quadrants_array() {
        let area = Rect::new(0, 0, 80, 24);
        let layout = TuiLayout::compute(area);
        let quads = layout.quadrants();

        assert_eq!(quads.len(), 4);
        assert_eq!(quads[0], layout.registers);
        assert_eq!(quads[1], layout.disassembly);
        assert_eq!(quads[2], layout.memory);
        assert_eq!(quads[3], layout.uart_console);
    }

    #[test]
    fn test_headless_skeleton_render() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let layout = TuiLayout::compute(frame.area());
                render_layout_skeleton(frame, &layout);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        // Verify key titles and hotkey labels are present in rendered buffer
        assert!(
            content.contains("CPU Registers & Flags"),
            "Buffer missing Registers title"
        );
        assert!(
            content.contains("Disassembly (PC)"),
            "Buffer missing Disassembly title"
        );
        assert!(
            content.contains("Memory Hex Viewer"),
            "Buffer missing Memory title"
        );
        assert!(
            content.contains("UART Console & Log"),
            "Buffer missing UART title"
        );
        assert!(content.contains("F5"), "Buffer missing F5 hotkey");
        assert!(content.contains("F10"), "Buffer missing F10 hotkey");
        assert!(content.contains("F9"), "Buffer missing F9 hotkey");
    }
}
