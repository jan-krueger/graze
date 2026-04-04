/// Autocomplete popup state for filter input.
pub struct AutocompleteState {
    pub suggestions: Vec<String>,
    pub selected_index: usize,
    pub active: bool,
}

impl AutocompleteState {
    pub fn new() -> Self {
        Self {
            suggestions: Vec::new(),
            selected_index: 0,
            active: false,
        }
    }

    pub fn clear(&mut self) {
        self.suggestions.clear();
        self.selected_index = 0;
        self.active = false;
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<String>) {
        if suggestions.is_empty() {
            self.clear();
        } else {
            self.suggestions = suggestions;
            if self.selected_index >= self.suggestions.len() {
                self.selected_index = 0;
            }
            self.active = true;
        }
    }

    pub fn move_up(&mut self) {
        if !self.suggestions.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.suggestions.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn move_down(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        }
    }

    pub fn selected(&self) -> Option<&str> {
        if self.active && !self.suggestions.is_empty() {
            let idx = self.selected_index.min(self.suggestions.len() - 1);
            Some(&self.suggestions[idx])
        } else {
            None
        }
    }
}
