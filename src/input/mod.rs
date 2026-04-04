pub mod autocomplete;
pub mod history;

/// Reusable cursor-based text input with optional character filtering.
pub struct TextInput {
    pub input: String,
    pub cursor_pos: usize,
    pub char_filter: Option<fn(char) -> bool>,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            char_filter: None,
        }
    }

    pub fn with_char_filter(filter: fn(char) -> bool) -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            char_filter: Some(filter),
        }
    }

    /// Byte offset in `input` corresponding to `cursor_pos` (char index).
    pub fn cursor_byte_pos(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    pub fn insert_char(&mut self, c: char) {
        if let Some(filter) = self.char_filter {
            if !filter(c) {
                return;
            }
        }
        let byte_pos = self.cursor_byte_pos();
        self.input.insert(byte_pos, c);
        self.cursor_pos += 1;
    }

    /// Delete the character before the cursor. Returns `true` if anything was removed.
    pub fn delete_before_cursor(&mut self) -> bool {
        if self.cursor_pos == 0 {
            return false;
        }
        self.cursor_pos -= 1;
        let byte_pos = self.cursor_byte_pos();
        self.input.remove(byte_pos);
        true
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let len = self.input.chars().count();
        if self.cursor_pos < len {
            self.cursor_pos += 1;
        }
    }

    pub fn move_cursor_to_start(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor_pos = self.input.chars().count();
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
    }

    pub fn set(&mut self, text: &str) {
        self.input = text.to_string();
        self.cursor_pos = self.input.chars().count();
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }
}
