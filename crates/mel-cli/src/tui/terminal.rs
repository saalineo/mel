//! Virtual UART Terminal Widget (VTE ANSI Parser).
//!
//! Provides ANSI escape sequence parsing, screen grid buffer management,
//! color rendering, cursor positioning, and bidirectional UART keystroke forwarding.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mel_devices::Uart16550;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Widget};
use vte::{Params, Parser, Perform};

/// A single character cell on the terminal screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    /// Rendered unicode character.
    pub c: char,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Style modifiers (Bold, Italic, Underline, etc.).
    pub modifier: Modifier,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::empty(),
        }
    }
}

/// State of the virtual terminal screen and scrollback buffer.
#[derive(Debug, Clone)]
pub struct TerminalState {
    /// Number of visible columns.
    pub cols: usize,
    /// Number of visible rows.
    pub rows: usize,
    /// Current 0-indexed column position of cursor.
    pub cursor_col: usize,
    /// Current 0-indexed row position of cursor.
    pub cursor_row: usize,
    /// Visibility of the cursor.
    pub cursor_visible: bool,
    /// Active foreground color.
    pub current_fg: Color,
    /// Active background color.
    pub current_bg: Color,
    /// Active text style modifiers.
    pub current_modifier: Modifier,
    /// 2D grid representing the active visible screen `[rows][cols]`.
    pub grid: Vec<Vec<TerminalCell>>,
    /// Scrollback history of lines scrolled off the top.
    pub scrollback: Vec<Vec<TerminalCell>>,
    /// Maximum number of scrollback lines to retain.
    pub max_scrollback: usize,
    /// Viewport scroll offset into history (0 = bottom / live view).
    pub scroll_offset: usize,
    /// Saved cursor column (for ANSI save/restore cursor `ESC 7`/`ESC 8` or `CSI s`/`CSI u`).
    pub saved_cursor_col: usize,
    /// Saved cursor row.
    pub saved_cursor_row: usize,
    /// Total count of bell control characters received.
    pub bell_count: usize,
}

impl TerminalState {
    /// Create a new terminal state with given dimensions and default scrollback limit.
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let grid = vec![vec![TerminalCell::default(); cols]; rows];
        Self {
            cols,
            rows,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            current_fg: Color::Reset,
            current_bg: Color::Reset,
            current_modifier: Modifier::empty(),
            grid,
            scrollback: Vec::new(),
            max_scrollback: 1000,
            scroll_offset: 0,
            saved_cursor_col: 0,
            saved_cursor_row: 0,
            bell_count: 0,
        }
    }

    /// Resize the visible terminal viewport, reallocating or preserving grid contents.
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(1);
        let new_rows = new_rows.max(1);
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }

        let mut new_grid = vec![vec![TerminalCell::default(); new_cols]; new_rows];
        for (r, row) in new_grid.iter_mut().enumerate().take(self.rows) {
            for (c, cell) in row.iter_mut().enumerate().take(self.cols) {
                *cell = self.grid[r][c];
            }
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.grid = new_grid;
        self.cursor_col = self.cursor_col.min(self.cols - 1);
        self.cursor_row = self.cursor_row.min(self.rows - 1);
    }

    /// Scroll up the terminal view into history.
    pub fn scroll_up(&mut self, count: usize) {
        let max_offset = self.scrollback.len();
        self.scroll_offset = (self.scroll_offset + count).min(max_offset);
    }

    /// Scroll down the terminal view towards live output.
    pub fn scroll_down(&mut self, count: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(count);
    }

    /// Scroll to the bottom (live output).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Reset screen grid and clear scrollback history.
    pub fn clear_all(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                *cell = TerminalCell::default();
            }
        }
        self.scrollback.clear();
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.scroll_offset = 0;
    }

    /// Return the text content of the visible grid as a vector of strings.
    pub fn visible_text(&self) -> Vec<String> {
        self.grid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.c)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Return the combined text content of the visible grid as a single string joined by newlines.
    pub fn visible_content_string(&self) -> String {
        self.visible_text().join("\n")
    }

    /// Feed raw bytes from UART output into the VTE parser.
    pub fn process_bytes(&mut self, bytes: &[u8]) {
        let mut parser = Parser::new();
        for &byte in bytes {
            parser.advance(self, byte);
        }
    }

    /// Feed a string into the terminal emulator.
    pub fn process_str(&mut self, s: &str) {
        self.process_bytes(s.as_bytes());
    }

    /// Drain pending bytes from a UART device and process them into the terminal.
    pub fn sync_from_uart(&mut self, uart: &mut Uart16550) {
        let output = uart.drain_output();
        if !output.is_empty() {
            self.process_bytes(&output);
        }
    }

    /// Handle an interactive keystroke by converting it to ANSI bytes and forwarding to UART.
    pub fn forward_key_to_uart(&mut self, key: KeyEvent, uart: &mut Uart16550) -> Vec<u8> {
        let bytes = key_event_to_uart_bytes(key);
        if !bytes.is_empty() {
            uart.push_input_bytes(&bytes);
        }
        bytes
    }

    fn line_feed(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            let old_row = self.grid.remove(0);
            if self.scrollback.len() >= self.max_scrollback {
                self.scrollback.remove(0);
            }
            self.scrollback.push(old_row);
            self.grid.push(vec![TerminalCell::default(); self.cols]);
        } else {
            self.cursor_row += 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    fn tab(&mut self) {
        let next_stop = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next_stop.min(self.cols.saturating_sub(1));
    }
}

fn sgr_color(code: u16) -> Option<Color> {
    match code {
        30 | 40 => Some(Color::Black),
        31 | 41 => Some(Color::Red),
        32 | 42 => Some(Color::Green),
        33 | 43 => Some(Color::Yellow),
        34 | 44 => Some(Color::Blue),
        35 | 45 => Some(Color::Magenta),
        36 | 46 => Some(Color::Cyan),
        37 | 47 => Some(Color::Gray),
        90 | 100 => Some(Color::DarkGray),
        91 | 101 => Some(Color::LightRed),
        92 | 102 => Some(Color::LightGreen),
        93 | 103 => Some(Color::LightYellow),
        94 | 104 => Some(Color::LightBlue),
        95 | 105 => Some(Color::LightMagenta),
        96 | 106 => Some(Color::LightCyan),
        97 | 107 => Some(Color::White),
        _ => None,
    }
}

impl Perform for TerminalState {
    fn print(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.line_feed();
        }

        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.grid[self.cursor_row][self.cursor_col] = TerminalCell {
                c,
                fg: self.current_fg,
                bg: self.current_bg,
                modifier: self.current_modifier,
            };
            self.cursor_col += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL (Bell)
                self.bell_count += 1;
            }
            0x08 => {
                // BS (Backspace)
                self.backspace();
            }
            0x09 => {
                // HT (Horizontal Tab)
                self.tab();
            }
            0x0A..=0x0C => {
                // LF, VT, FF (Line Feed)
                self.line_feed();
            }
            0x0D => {
                // CR (Carriage Return)
                self.carriage_return();
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                // Save cursor
                self.saved_cursor_col = self.cursor_col;
                self.saved_cursor_row = self.cursor_row;
            }
            b'8' => {
                // Restore cursor
                self.cursor_col = self.saved_cursor_col.min(self.cols.saturating_sub(1));
                self.cursor_row = self.saved_cursor_row.min(self.rows.saturating_sub(1));
            }
            b'c' => {
                // Full Reset (RIS)
                self.clear_all();
                self.current_fg = Color::Reset;
                self.current_bg = Color::Reset;
                self.current_modifier = Modifier::empty();
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Collect flattened parameters and subparameters
        let mut param_list: Vec<u16> = Vec::new();
        for p in params.iter() {
            param_list.extend_from_slice(p);
        }

        let get_param = |idx: usize, default_val: u16| -> u16 {
            param_list.get(idx).copied().unwrap_or(default_val)
        };
        let get_nonzero_param = |idx: usize, default_val: u16| -> u16 {
            let val = get_param(idx, default_val);
            if val == 0 {
                default_val
            } else {
                val
            }
        };

        match action {
            'A' => {
                // Cursor Up (CUU)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(count);
            }
            'B' => {
                // Cursor Down (CUD)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_row = (self.cursor_row + count).min(self.rows.saturating_sub(1));
            }
            'C' => {
                // Cursor Forward (CUF)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_col = (self.cursor_col + count).min(self.cols.saturating_sub(1));
            }
            'D' => {
                // Cursor Back (CUB)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(count);
            }
            'E' => {
                // Cursor Next Line (CNL)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_col = 0;
                self.cursor_row = (self.cursor_row + count).min(self.rows.saturating_sub(1));
            }
            'F' => {
                // Cursor Previous Line (CPL)
                let count = get_nonzero_param(0, 1) as usize;
                self.cursor_col = 0;
                self.cursor_row = self.cursor_row.saturating_sub(count);
            }
            'G' => {
                // Cursor Horizontal Absolute (CHA)
                let col = get_nonzero_param(0, 1) as usize;
                self.cursor_col = (col.saturating_sub(1)).min(self.cols.saturating_sub(1));
            }
            'H' | 'f' => {
                // Cursor Position (CUP / HVP)
                let r = get_nonzero_param(0, 1) as usize;
                let c = get_nonzero_param(1, 1) as usize;
                self.cursor_row = (r.saturating_sub(1)).min(self.rows.saturating_sub(1));
                self.cursor_col = (c.saturating_sub(1)).min(self.cols.saturating_sub(1));
            }
            'J' => {
                // Erase in Display (ED)
                let mode = get_param(0, 0);
                match mode {
                    0 => {
                        // Clear from cursor to end of screen
                        if self.cursor_row < self.rows {
                            for c in self.cursor_col..self.cols {
                                self.grid[self.cursor_row][c] = TerminalCell::default();
                            }
                            for r in (self.cursor_row + 1)..self.rows {
                                for c in 0..self.cols {
                                    self.grid[r][c] = TerminalCell::default();
                                }
                            }
                        }
                    }
                    1 => {
                        // Clear from beginning to cursor
                        if self.cursor_row < self.rows {
                            for r in 0..self.cursor_row {
                                for c in 0..self.cols {
                                    self.grid[r][c] = TerminalCell::default();
                                }
                            }
                            for c in 0..=self.cursor_col.min(self.cols.saturating_sub(1)) {
                                self.grid[self.cursor_row][c] = TerminalCell::default();
                            }
                        }
                    }
                    2 | 3 => {
                        // Clear entire screen
                        for r in 0..self.rows {
                            for c in 0..self.cols {
                                self.grid[r][c] = TerminalCell::default();
                            }
                        }
                    }
                    _ => {}
                }
            }
            'K' => {
                // Erase in Line (EL)
                let mode = get_param(0, 0);
                if self.cursor_row < self.rows {
                    match mode {
                        0 => {
                            // Clear from cursor to end of line
                            for c in self.cursor_col..self.cols {
                                self.grid[self.cursor_row][c] = TerminalCell::default();
                            }
                        }
                        1 => {
                            // Clear from start to cursor
                            for c in 0..=self.cursor_col.min(self.cols.saturating_sub(1)) {
                                self.grid[self.cursor_row][c] = TerminalCell::default();
                            }
                        }
                        2 => {
                            // Clear entire line
                            for c in 0..self.cols {
                                self.grid[self.cursor_row][c] = TerminalCell::default();
                            }
                        }
                        _ => {}
                    }
                }
            }
            's' => {
                // Save cursor position
                self.saved_cursor_col = self.cursor_col;
                self.saved_cursor_row = self.cursor_row;
            }
            'u' => {
                // Restore cursor position
                self.cursor_col = self.saved_cursor_col.min(self.cols.saturating_sub(1));
                self.cursor_row = self.saved_cursor_row.min(self.rows.saturating_sub(1));
            }
            'h' => {
                // Set Mode (?25h -> Show Cursor)
                self.cursor_visible = true;
            }
            'l' => {
                // Reset Mode (?25l -> Hide Cursor)
                self.cursor_visible = false;
            }
            'm' => {
                // SGR (Select Graphic Rendition)
                if param_list.is_empty() {
                    self.current_fg = Color::Reset;
                    self.current_bg = Color::Reset;
                    self.current_modifier = Modifier::empty();
                    return;
                }

                let mut idx = 0;
                while idx < param_list.len() {
                    let code = param_list[idx];
                    match code {
                        0 => {
                            self.current_fg = Color::Reset;
                            self.current_bg = Color::Reset;
                            self.current_modifier = Modifier::empty();
                        }
                        1 => self.current_modifier.insert(Modifier::BOLD),
                        2 => self.current_modifier.insert(Modifier::DIM),
                        3 => self.current_modifier.insert(Modifier::ITALIC),
                        4 => self.current_modifier.insert(Modifier::UNDERLINED),
                        7 => self.current_modifier.insert(Modifier::REVERSED),
                        22 => {
                            self.current_modifier.remove(Modifier::BOLD);
                            self.current_modifier.remove(Modifier::DIM);
                        }
                        23 => self.current_modifier.remove(Modifier::ITALIC),
                        24 => self.current_modifier.remove(Modifier::UNDERLINED),
                        27 => self.current_modifier.remove(Modifier::REVERSED),
                        30..=37 | 90..=97 => {
                            if let Some(c) = sgr_color(code) {
                                self.current_fg = c;
                            }
                        }
                        38 => {
                            // Extended foreground: 38;5;n or 38;2;r;g;b
                            if idx + 2 < param_list.len() && param_list[idx + 1] == 5 {
                                self.current_fg = Color::Indexed(param_list[idx + 2] as u8);
                                idx += 2;
                            } else if idx + 4 < param_list.len() && param_list[idx + 1] == 2 {
                                self.current_fg = Color::Rgb(
                                    param_list[idx + 2] as u8,
                                    param_list[idx + 3] as u8,
                                    param_list[idx + 4] as u8,
                                );
                                idx += 4;
                            }
                        }
                        39 => self.current_fg = Color::Reset,
                        40..=47 | 100..=107 => {
                            if let Some(c) = sgr_color(code) {
                                self.current_bg = c;
                            }
                        }
                        48 => {
                            // Extended background: 48;5;n or 48;2;r;g;b
                            if idx + 2 < param_list.len() && param_list[idx + 1] == 5 {
                                self.current_bg = Color::Indexed(param_list[idx + 2] as u8);
                                idx += 2;
                            } else if idx + 4 < param_list.len() && param_list[idx + 1] == 2 {
                                self.current_bg = Color::Rgb(
                                    param_list[idx + 2] as u8,
                                    param_list[idx + 3] as u8,
                                    param_list[idx + 4] as u8,
                                );
                                idx += 4;
                            }
                        }
                        49 => self.current_bg = Color::Reset,
                        _ => {}
                    }
                    idx += 1;
                }
            }
            _ => {}
        }
    }
}

/// Convert an interactive Crossterm `KeyEvent` into corresponding serial UART bytes.
pub fn key_event_to_uart_bytes(key: KeyEvent) -> Vec<u8> {
    // Check Ctrl combinations
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = key.code {
            let upper = c.to_ascii_uppercase();
            if upper.is_ascii_alphabetic() {
                // Ctrl+A = 1, Ctrl+B = 2, ..., Ctrl+Z = 26
                return vec![(upper as u8) - b'A' + 1];
            } else {
                match c {
                    '@' | ' ' => return vec![0],
                    '[' => return vec![0x1b],
                    '\\' => return vec![0x1c],
                    ']' => return vec![0x1d],
                    '^' => return vec![0x1e],
                    '_' => return vec![0x1f],
                    _ => {}
                }
            }
        }
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x08],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(1) => b"\x1bOP".to_vec(),
        KeyCode::F(2) => b"\x1bOQ".to_vec(),
        KeyCode::F(3) => b"\x1bOR".to_vec(),
        KeyCode::F(4) => b"\x1bOS".to_vec(),
        KeyCode::F(5) => b"\x1b[15~".to_vec(),
        KeyCode::F(6) => b"\x1b[17~".to_vec(),
        KeyCode::F(7) => b"\x1b[18~".to_vec(),
        KeyCode::F(8) => b"\x1b[19~".to_vec(),
        KeyCode::F(9) => b"\x1b[20~".to_vec(),
        KeyCode::F(10) => b"\x1b[21~".to_vec(),
        KeyCode::F(11) => b"\x1b[23~".to_vec(),
        KeyCode::F(12) => b"\x1b[24~".to_vec(),
        _ => Vec::new(),
    }
}

/// Ratatui widget for rendering the virtual UART interactive terminal.
pub struct TerminalWidget<'a> {
    state: &'a TerminalState,
    is_focused: bool,
}

impl<'a> TerminalWidget<'a> {
    /// Create a new terminal widget referencing the terminal state.
    pub fn new(state: &'a TerminalState) -> Self {
        Self {
            state,
            is_focused: false,
        }
    }

    /// Set the focused state of the widget (changes border highlight color).
    pub fn focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }
}

impl<'a> Widget for TerminalWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.is_focused {
            Color::Yellow
        } else {
            Color::Cyan
        };

        let scroll_hint = if self.state.scroll_offset > 0 {
            format!(" [Scrollback -{}] ", self.state.scroll_offset)
        } else {
            "".to_string()
        };

        let title = format!(" [4] UART Interactive Terminal{} ", scroll_hint);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let total_scrollback = self.state.scrollback.len();
        let display_rows = inner.height as usize;

        for (row_idx, y) in (inner.y..(inner.y + inner.height)).enumerate() {
            let cell_row: Option<&[TerminalCell]> = if self.state.scroll_offset > 0 {
                let virtual_idx = total_scrollback + self.state.rows
                    - self.state.scroll_offset
                    - (display_rows - row_idx);
                if virtual_idx < total_scrollback {
                    self.state.scrollback.get(virtual_idx).map(|v| v.as_slice())
                } else {
                    let grid_idx = virtual_idx - total_scrollback;
                    self.state.grid.get(grid_idx).map(|v| v.as_slice())
                }
            } else {
                self.state.grid.get(row_idx).map(|v| v.as_slice())
            };

            if let Some(cells) = cell_row {
                for (col_idx, x) in (inner.x..(inner.x + inner.width)).enumerate() {
                    if let Some(cell) = cells.get(col_idx) {
                        let is_cursor = self.state.scroll_offset == 0
                            && self.state.cursor_visible
                            && row_idx == self.state.cursor_row
                            && col_idx == self.state.cursor_col;

                        let style = if is_cursor {
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            let mut s = Style::default();
                            if cell.fg != Color::Reset {
                                s = s.fg(cell.fg);
                            }
                            if cell.bg != Color::Reset {
                                s = s.bg(cell.bg);
                            }
                            s = s.add_modifier(cell.modifier);
                            s
                        };

                        if let Some(buf_cell) = buf.cell_mut((x, y)) {
                            buf_cell.set_char(cell.c);
                            buf_cell.set_style(style);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_plain_text_and_cursor_advance() {
        let mut term = TerminalState::new(20, 5);
        term.process_str("Hello, MEL!");

        assert_eq!(term.cursor_row, 0);
        assert_eq!(term.cursor_col, 11);
        let text = term.visible_text();
        assert_eq!(text[0], "Hello, MEL!");
    }

    #[test]
    fn test_crlf_and_scrolling() {
        let mut term = TerminalState::new(10, 3);
        term.process_str("Line 1\r\nLine 2\r\nLine 3\r\nLine 4\r\n");

        assert_eq!(term.scrollback.len(), 2);
        let text = term.visible_text();
        assert_eq!(text[0], "Line 3");
        assert_eq!(text[1], "Line 4");
        assert_eq!(text[2], "");
    }

    #[test]
    fn test_backspace_and_tab() {
        let mut term = TerminalState::new(20, 5);
        term.process_str("abc");
        assert_eq!(term.cursor_col, 3);
        term.process_bytes(&[0x08]); // Backspace
        assert_eq!(term.cursor_col, 2);
        term.process_str("Z");
        assert_eq!(term.cursor_col, 3);
        assert_eq!(term.visible_text()[0], "abZ");

        term.clear_all();
        term.process_str("a\tb");
        // 'a' at 0, tab moves to 8, 'b' at 8 -> cursor at 9
        assert_eq!(term.cursor_col, 9);
        assert_eq!(term.grid[0][0].c, 'a');
        assert_eq!(term.grid[0][8].c, 'b');
    }

    #[test]
    fn test_ansi_cursor_positioning() {
        let mut term = TerminalState::new(20, 10);
        // CSI 4 ; 6 H -> row 4, col 6 (1-indexed -> row 3, col 5)
        term.process_str("\x1b[4;6H*");
        assert_eq!(term.cursor_row, 3);
        assert_eq!(term.cursor_col, 6);
        assert_eq!(term.grid[3][5].c, '*');

        // Cursor Up 2 -> row 1
        term.process_str("\x1b[2A^");
        assert_eq!(term.cursor_row, 1);
        assert_eq!(term.grid[1][6].c, '^');

        // Cursor Back 3 -> col 4
        term.process_str("\x1b[3D<");
        assert_eq!(term.cursor_col, 5);
        assert_eq!(term.grid[1][4].c, '<');
    }

    #[test]
    fn test_ansi_erase_functions() {
        let mut term = TerminalState::new(10, 3);
        term.process_str("0123456789\r\n0123456789\r\n0123456789");

        // Move to row 2 col 5 (0-indexed: row 1, col 4), Erase Line to end (CSI 0 K)
        term.process_str("\x1b[2;5H\x1b[0K");
        let text = term.visible_text();
        assert_eq!(text[1], "0123");

        // Erase entire display (CSI 2 J)
        term.process_str("\x1b[2J");
        let text2 = term.visible_text();
        assert_eq!(text2[0], "");
        assert_eq!(text2[1], "");
        assert_eq!(text2[2], "");
    }

    #[test]
    fn test_sgr_colors_and_styles() {
        let mut term = TerminalState::new(30, 5);
        // Bold + Red FG + Green BG
        term.process_str("\x1b[1;31;42mColorTest\x1b[0mNormal");

        assert_eq!(term.grid[0][0].fg, Color::Red);
        assert_eq!(term.grid[0][0].bg, Color::Green);
        assert!(term.grid[0][0].modifier.contains(Modifier::BOLD));

        // Normal part after reset
        assert_eq!(term.grid[0][9].fg, Color::Reset);
        assert_eq!(term.grid[0][9].bg, Color::Reset);
        assert_eq!(term.grid[0][9].modifier, Modifier::empty());

        // 256 Color fg
        term.process_str("\r\n\x1b[38;5;201mPink\x1b[0m");
        assert_eq!(term.grid[1][0].fg, Color::Indexed(201));

        // 24-bit RGB fg
        term.process_str("\r\n\x1b[38;2;100;150;200mRGB\x1b[0m");
        assert_eq!(term.grid[2][0].fg, Color::Rgb(100, 150, 200));
    }

    #[test]
    fn test_cursor_save_and_restore() {
        let mut term = TerminalState::new(20, 5);
        term.process_str("ABC\x1b[sDEF\x1b[uG");
        // ABC (cursor 3), save, DEF (cursor 6), restore (cursor 3), G (overwrites D)
        assert_eq!(term.visible_text()[0], "ABCGEF");
    }

    #[test]
    fn test_bell_and_reset() {
        let mut term = TerminalState::new(20, 5);
        term.process_str("\x07\x07Hello\x1bc");
        assert_eq!(term.bell_count, 2);
        // Full reset clears screen
        assert_eq!(term.visible_text()[0], "");
        assert_eq!(term.cursor_col, 0);
        assert_eq!(term.cursor_row, 0);
    }

    #[test]
    fn test_keystroke_to_bytes() {
        use crossterm::event::{KeyEventKind, KeyEventState};

        let mk_key = |code, modifiers| KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };

        // Regular char
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Char('x'), KeyModifiers::NONE)),
            vec![b'x']
        );

        // Enter -> \r
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Enter, KeyModifiers::NONE)),
            vec![b'\r']
        );

        // Backspace -> 0x08
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Backspace, KeyModifiers::NONE)),
            vec![0x08]
        );

        // Ctrl+C -> 0x03
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            vec![0x03]
        );

        // Ctrl+D -> 0x04
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            vec![0x04]
        );

        // Up arrow -> \x1b[A
        assert_eq!(
            key_event_to_uart_bytes(mk_key(KeyCode::Up, KeyModifiers::NONE)),
            b"\x1b[A".to_vec()
        );
    }

    #[test]
    fn test_uart_sync_and_forwarding() {
        let mut uart = Uart16550::new("UART0");
        let mut term = TerminalState::new(30, 5);

        // Simulate guest writing to UART
        let guest_msg = b"Booting kernel...\r\n";
        for &b in guest_msg {
            let _ = mel_devices::MmioDevice::write(
                &mut uart,
                mel_devices::UART_THR,
                b as u64,
                1,
            );
        }

        term.sync_from_uart(&mut uart);
        assert_eq!(term.visible_text()[0], "Booting kernel...");

        // Forward host keystroke to UART rx queue
        use crossterm::event::{KeyEventKind, KeyEventState};
        let key = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        term.forward_key_to_uart(key, &mut uart);

        // Read back from UART RBR
        let rx_byte = mel_devices::MmioDevice::read(
            &mut uart,
            mel_devices::UART_RBR,
            1,
        )
        .unwrap();
        assert_eq!(rx_byte as u8, b'q');
    }

    #[test]
    fn test_headless_ratatui_render() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut term_state = TerminalState::new(36, 8);
        term_state.process_str("\x1b[1;32m[MEL] Ready>\x1b[0m ");

        terminal
            .draw(|frame| {
                let area = frame.area();
                let widget = TerminalWidget::new(&term_state).focused(true);
                frame.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);
        assert!(content.contains("[4] UART Interactive Terminal"));
        assert!(content.contains("[MEL] Ready>"));
    }
}
