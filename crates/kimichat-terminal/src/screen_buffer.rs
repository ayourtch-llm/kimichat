use vt100::Parser;

use super::DEFAULT_SCROLLBACK_LINES;

/// Terminal screen state buffer
pub struct ScreenBuffer {
    parser: Parser,
    scrollback_lines: usize,
    cols: u16,
    rows: u16,
}

impl ScreenBuffer {
    /// Create a new screen buffer
    pub fn new(cols: u16, rows: u16) -> Self {
        let parser = Parser::new(rows, cols, DEFAULT_SCROLLBACK_LINES);

        Self {
            parser,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            cols,
            rows,
        }
    }

    /// Process output data (feed to VT100 parser)
    pub fn process_output(&mut self, data: &str) {
        self.parser.process(data.as_bytes());
    }

    /// Get screen contents as text
    pub fn get_contents(&self, include_colors: bool, include_cursor: bool) -> String {
        let screen = self.parser.screen();

        if include_colors {
            // Get formatted output with ANSI color codes
            String::from_utf8_lossy(&screen.contents_formatted()).to_string()
        } else {
            // Get plain text
            screen.contents()
        }
    }

    /// Get cursor position (col, row)
    pub fn cursor_position(&self) -> (u16, u16) {
        let screen = self.parser.screen();
        screen.cursor_position()
    }

    /// Get terminal size (cols, rows)
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Resize the screen buffer
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        // Create a new parser with the new size
        self.parser = Parser::new(rows, cols, self.scrollback_lines);
    }

    /// Set scrollback buffer size
    pub fn set_scrollback_lines(&mut self, lines: usize) {
        self.scrollback_lines = lines;
        // Recreate parser with new scrollback
        self.parser = Parser::new(self.rows, self.cols, lines);
    }

    /// Get the underlying parser screen for advanced operations
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }
}
