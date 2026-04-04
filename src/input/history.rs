/// Stores previous inputs for recall with Up/Down.
pub struct InputHistory {
    entries: Vec<String>,
    position: Option<usize>,
    max_entries: usize,
}

impl InputHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            position: None,
            max_entries,
        }
    }

    pub fn push(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        // Remove duplicate if present
        self.entries.retain(|e| e != &entry);
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.position = None;
    }

    /// Move up in history (older entries). Returns the entry if available.
    pub fn up(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let pos = match self.position {
            Some(p) if p > 0 => p - 1,
            Some(p) => p,
            None => self.entries.len() - 1,
        };
        self.position = Some(pos);
        Some(&self.entries[pos])
    }

    /// Move down in history (newer entries). Returns the entry or None if at bottom.
    pub fn down(&mut self) -> Option<&str> {
        let pos = match self.position {
            Some(p) => p + 1,
            None => return None,
        };
        if pos < self.entries.len() {
            self.position = Some(pos);
            Some(&self.entries[pos])
        } else {
            self.position = None;
            None
        }
    }

    pub fn reset_position(&mut self) {
        self.position = None;
    }
}
