//! Memory Hex Viewer Widget.
//!
//! Provides a 16-byte-per-row hexadecimal and ASCII memory inspector with
//! address offset navigation, address jumping (`goto`), and pattern searching.

use mel_core::Address;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

/// Row width in bytes for standard hex dumps.
pub const BYTES_PER_ROW: usize = 16;

/// State for the memory viewer including viewport offset, cursor, and search results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryViewerState {
    /// Starting base address of the top displayed row (16-byte aligned).
    pub base_address: Address,
    /// Selected / focused cursor address if any.
    pub cursor_address: Option<Address>,
    /// Active search pattern query (text or hex string).
    pub search_query: Option<String>,
    /// List of matched start addresses from the last search.
    pub search_matches: Vec<Address>,
    /// Currently focused index in `search_matches`.
    pub match_index: usize,
    /// Length of the search pattern in bytes.
    pub match_length: usize,
}

impl MemoryViewerState {
    /// Create a new viewer state initialized to a 16-byte aligned address.
    pub fn new(initial_address: Address) -> Self {
        Self {
            base_address: initial_address & !0xF,
            cursor_address: None,
            search_query: None,
            search_matches: Vec::new(),
            match_index: 0,
            match_length: 0,
        }
    }

    /// Jump directly to an address, aligning the viewport to 16 bytes.
    pub fn goto(&mut self, address: Address) {
        self.base_address = address & !0xF;
        self.cursor_address = Some(address);
    }

    /// Scroll down by a specified number of 16-byte rows.
    pub fn scroll_down(&mut self, rows: usize) {
        let delta = (rows * BYTES_PER_ROW) as u64;
        self.base_address = self.base_address.saturating_add(delta);
    }

    /// Scroll up by a specified number of 16-byte rows.
    pub fn scroll_up(&mut self, rows: usize) {
        let delta = (rows * BYTES_PER_ROW) as u64;
        self.base_address = self.base_address.saturating_sub(delta);
    }

    /// Page down by the given row count.
    pub fn page_down(&mut self, rows: usize) {
        self.scroll_down(rows.max(1));
    }

    /// Page up by the given row count.
    pub fn page_up(&mut self, rows: usize) {
        self.scroll_up(rows.max(1));
    }

    /// Search for a raw byte pattern within a provided memory buffer.
    pub fn search_bytes(&mut self, pattern: &[u8], buffer_base: Address, buffer: &[u8]) {
        self.search_matches.clear();
        self.match_index = 0;
        self.match_length = pattern.len();

        if pattern.is_empty() || buffer.len() < pattern.len() {
            self.search_query = None;
            return;
        }

        let hex_str: String = pattern.iter().map(|b| format!("{b:02x}")).collect();
        self.search_query = Some(format!("0x{hex_str}"));

        for window_idx in 0..=(buffer.len() - pattern.len()) {
            if &buffer[window_idx..window_idx + pattern.len()] == pattern {
                let match_addr = buffer_base.saturating_add(window_idx as u64);
                self.search_matches.push(match_addr);
            }
        }

        if let Some(&first_match) = self.search_matches.first() {
            self.goto(first_match);
        }
    }

    /// Search for an ASCII text query within a provided memory buffer.
    pub fn search_text(&mut self, text: &str, buffer_base: Address, buffer: &[u8]) {
        if text.is_empty() {
            self.search_matches.clear();
            self.search_query = None;
            self.match_length = 0;
            return;
        }

        self.search_bytes(text.as_bytes(), buffer_base, buffer);
        self.search_query = Some(format!("\"{text}\""));
    }

    /// Advance viewport to the next search match.
    pub fn next_match(&mut self) -> Option<Address> {
        if self.search_matches.is_empty() {
            return None;
        }
        self.match_index = (self.match_index + 1) % self.search_matches.len();
        let target = self.search_matches[self.match_index];
        self.goto(target);
        Some(target)
    }

    /// Move viewport to the previous search match.
    pub fn prev_match(&mut self) -> Option<Address> {
        if self.search_matches.is_empty() {
            return None;
        }
        if self.match_index == 0 {
            self.match_index = self.search_matches.len() - 1;
        } else {
            self.match_index -= 1;
        }
        let target = self.search_matches[self.match_index];
        self.goto(target);
        Some(target)
    }
}

/// TUI Widget for rendering the scrollable hex dump.
#[derive(Debug, Clone)]
pub struct MemoryViewerWidget<'a> {
    state: &'a MemoryViewerState,
    buffer_base: Address,
    buffer: &'a [u8],
    focused: bool,
}

impl<'a> MemoryViewerWidget<'a> {
    /// Create a memory viewer widget with current state and memory buffer slice.
    pub fn new(state: &'a MemoryViewerState, buffer_base: Address, buffer: &'a [u8]) -> Self {
        Self {
            state,
            buffer_base,
            buffer,
            focused: false,
        }
    }

    /// Set focused border state.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Render into an active frame area.
    pub fn render_frame(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self.clone(), area);
    }
}

impl Widget for MemoryViewerWidget<'_> {
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

        let title_text = match (&self.state.search_query, self.state.search_matches.len()) {
            (Some(q), count) if count > 0 => format!(
                " [3] Memory Viewer [{:#010x}] [Search: {} ({}/{})] ",
                self.state.base_address,
                q,
                self.state.match_index + 1,
                count
            ),
            (Some(q), _) => format!(
                " [3] Memory Viewer [{:#010x}] [Search: {} (0 matches)] ",
                self.state.base_address, q
            ),
            (None, _) => format!(
                " [3] Memory Hex Viewer [{:#010x}] ",
                self.state.base_address
            ),
        };

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

        let header_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM);
        let addr_style = Style::default().fg(Color::Cyan);
        let hex_style = Style::default().fg(Color::White);
        let zero_byte_style = Style::default().fg(Color::DarkGray);
        let ascii_style = Style::default().fg(Color::Green);
        let match_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let cursor_style = Style::default()
            .fg(Color::White)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD);

        let mut current_y = inner.y;
        let max_y = inner.y + inner.height;

        // Header column labels
        if current_y < max_y && inner.width >= 64 {
            let header_line = Line::from(vec![
                Span::styled(" Address       ", header_style),
                Span::styled(
                    "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F  ",
                    header_style,
                ),
                Span::styled("|0123456789ABCDEF|", header_style),
            ]);
            buf.set_line(inner.x, current_y, &header_line, inner.width);
            current_y += 1;
        }

        // Render memory rows
        let active_match_range = if !self.state.search_matches.is_empty() {
            let start = self.state.search_matches[self.state.match_index];
            let end = start.saturating_add(self.state.match_length as u64);
            Some(start..end)
        } else {
            None
        };

        let mut row_addr = self.state.base_address;

        while current_y < max_y {
            let mut spans = Vec::new();

            // 1. Address label
            spans.push(Span::styled(format!("{row_addr:#010x}:  "), addr_style));

            // Extract 16 bytes for this row from buffer
            let mut row_bytes = [0u8; BYTES_PER_ROW];
            let mut row_valid = [false; BYTES_PER_ROW];

            for i in 0..BYTES_PER_ROW {
                let byte_addr = row_addr.saturating_add(i as u64);
                if byte_addr >= self.buffer_base {
                    let offset = (byte_addr - self.buffer_base) as usize;
                    if offset < self.buffer.len() {
                        row_bytes[i] = self.buffer[offset];
                        row_valid[i] = true;
                    }
                }
            }

            // 2. Hex representations
            for i in 0..BYTES_PER_ROW {
                if i == 8 {
                    spans.push(Span::raw(" "));
                }

                let byte_addr = row_addr.saturating_add(i as u64);
                let is_match = active_match_range
                    .as_ref()
                    .is_some_and(|r| r.contains(&byte_addr));
                let is_cursor = self.state.cursor_address == Some(byte_addr);

                if row_valid[i] {
                    let b = row_bytes[i];
                    let style = if is_cursor {
                        cursor_style
                    } else if is_match {
                        match_style
                    } else if b == 0 {
                        zero_byte_style
                    } else {
                        hex_style
                    };
                    spans.push(Span::styled(format!("{b:02x} "), style));
                } else {
                    spans.push(Span::styled(".. ", zero_byte_style));
                }
            }

            spans.push(Span::raw(" |"));

            // 3. ASCII representation
            for i in 0..BYTES_PER_ROW {
                let byte_addr = row_addr.saturating_add(i as u64);
                let is_match = active_match_range
                    .as_ref()
                    .is_some_and(|r| r.contains(&byte_addr));
                let is_cursor = self.state.cursor_address == Some(byte_addr);

                if row_valid[i] {
                    let b = row_bytes[i];
                    let ch = if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    };
                    let style = if is_cursor {
                        cursor_style
                    } else if is_match {
                        match_style
                    } else if b == 0 {
                        zero_byte_style
                    } else {
                        ascii_style
                    };
                    spans.push(Span::styled(ch.to_string(), style));
                } else {
                    spans.push(Span::styled(".", zero_byte_style));
                }
            }

            spans.push(Span::raw("|"));

            let line = Line::from(spans);
            buf.set_line(inner.x, current_y, &line, inner.width);
            current_y += 1;
            row_addr = row_addr.saturating_add(BYTES_PER_ROW as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_memory_viewer_initialization_and_alignment() {
        let state = MemoryViewerState::new(0x8000_0007);
        assert_eq!(state.base_address, 0x8000_0000);
        assert_eq!(state.cursor_address, None);
    }

    #[test]
    fn test_memory_viewer_scrolling_and_goto() {
        let mut state = MemoryViewerState::new(0x8000_0000);

        state.scroll_down(2);
        assert_eq!(state.base_address, 0x8000_0020);

        state.scroll_up(1);
        assert_eq!(state.base_address, 0x8000_0010);

        state.goto(0x8000_1234);
        assert_eq!(state.base_address, 0x8000_1230);
        assert_eq!(state.cursor_address, Some(0x8000_1234));

        state.page_down(10);
        assert_eq!(state.base_address, 0x8000_1230 + 160);

        state.page_up(10);
        assert_eq!(state.base_address, 0x8000_1230);
    }

    #[test]
    fn test_memory_viewer_byte_and_text_search() {
        let mut state = MemoryViewerState::new(0x1000);
        let base = 0x1000;
        let mut mem = vec![0u8; 128];

        // Place "BOOT" at offset 0x14 (address 0x1014) and 0x30 (address 0x1030)
        mem[0x14..0x18].copy_from_slice(b"BOOT");
        mem[0x30..0x34].copy_from_slice(b"BOOT");

        state.search_text("BOOT", base, &mem);
        assert_eq!(state.search_matches.len(), 2);
        assert_eq!(state.search_matches[0], 0x1014);
        assert_eq!(state.search_matches[1], 0x1030);
        assert_eq!(state.base_address, 0x1010); // Viewport aligns to row start 0x1010
        assert_eq!(state.match_index, 0);

        // Next match
        let next = state.next_match();
        assert_eq!(next, Some(0x1030));
        assert_eq!(state.base_address, 0x1030);
        assert_eq!(state.match_index, 1);

        // Wrap around
        let wrapped = state.next_match();
        assert_eq!(wrapped, Some(0x1014));
        assert_eq!(state.match_index, 0);

        // Prev match
        let prev = state.prev_match();
        assert_eq!(prev, Some(0x1030));
        assert_eq!(state.match_index, 1);
    }

    #[test]
    fn test_headless_memory_render() {
        let mut state = MemoryViewerState::new(0x8000_0000);
        let base = 0x8000_0000;
        let mut mem = vec![0u8; 256];
        mem[0] = 0x7F;
        mem[1] = b'E';
        mem[2] = b'L';
        mem[3] = b'F';
        mem[4] = 0x02;

        state.search_text("ELF", base, &mem);

        let widget = MemoryViewerWidget::new(&state, base, &mem).focused(true);

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
            content.contains("Memory"),
            "Buffer missing Memory Viewer title"
        );
        assert!(
            content.contains("80000000"),
            "Buffer missing address string in hex"
        );
        assert!(content.contains("7f"), "Buffer missing 0x7f byte");
        assert!(content.contains("ELF"), "Buffer missing ASCII ELF string");
        assert!(
            content.contains("Search:"),
            "Buffer missing Search label in title"
        );
    }
}
