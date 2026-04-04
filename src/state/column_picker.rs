/// Reusable column picker state used by diff setup and column hide.
pub struct ColumnPickerState {
    pub columns: Vec<String>,
    pub selected: Vec<bool>,
    pub cursor: usize,
}

impl ColumnPickerState {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            selected: Vec::new(),
            cursor: 0,
        }
    }

    pub fn populate(&mut self, columns: Vec<String>, selected: Vec<bool>) {
        assert_eq!(columns.len(), selected.len());
        self.columns = columns;
        self.selected = selected;
        self.cursor = 0;
    }

    pub fn toggle_at_cursor(&mut self) {
        if self.cursor < self.selected.len() {
            self.selected[self.cursor] = !self.selected[self.cursor];
        }
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if !self.columns.is_empty() && self.cursor + 1 < self.columns.len() {
            self.cursor += 1;
        }
    }

    /// Return names of all selected columns.
    pub fn selected_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .zip(self.selected.iter())
            .filter(|(_, sel)| **sel)
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
}
