use std::sync::Arc;

use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;

use crate::event::SortState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Row,
    Column,
}

/// Viewport state: cursor position, scrolling, and terminal dimensions.
pub struct Viewport {
    pub view_start: usize,
    pub selected_row: usize,
    pub page_size: usize,
    pub column_offset: usize,
    pub selected_col: usize,
    pub selection_mode: SelectionMode,
    pub terminal_width: u16,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            view_start: 0,
            selected_row: 0,
            page_size: 50,
            column_offset: 0,
            selected_col: 0,
            selection_mode: SelectionMode::Row,
            terminal_width: 0,
        }
    }

    /// Row index of the selected row relative to the top of the visible view.
    pub fn selected_row_in_view(&self) -> usize {
        self.selected_row.saturating_sub(self.view_start)
    }

    /// Keep the selected row visible by scrolling the view.
    pub fn adjust_view(&mut self) {
        if self.selected_row < self.view_start {
            self.view_start = self.selected_row;
        } else if self.selected_row >= self.view_start + self.page_size {
            self.view_start = self.selected_row - self.page_size + 1;
        }
    }

    /// Keep the selected column visible by scrolling the column offset.
    /// Uses `visible_col_count` computed from schema + terminal_width.
    pub fn adjust_column_view_with(&mut self, visible: usize) {
        if self.selected_col < self.column_offset {
            self.column_offset = self.selected_col;
        } else if visible > 0 && self.selected_col >= self.column_offset + visible {
            self.column_offset = self.selected_col - visible + 1;
        }
    }

    pub fn move_cursor_down(&mut self, n: usize, total_rows: usize) {
        if total_rows == 0 {
            return;
        }
        self.selected_row = (self.selected_row + n).min(total_rows - 1);
        self.adjust_view();
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        self.selected_row = self.selected_row.saturating_sub(n);
        self.adjust_view();
    }
}

/// Core data state: schema, current buffer batch, row counts.
pub struct DataState {
    pub schema: Option<Arc<Schema>>,
    pub current_batch: Option<RecordBatch>,
    pub buffer_offset: usize,
    pub total_rows: usize,
    pub file_name: Option<String>,
    pub table_name: Option<String>,
    pub sort_state: SortState,
}

impl DataState {
    pub fn new() -> Self {
        Self {
            schema: None,
            current_batch: None,
            buffer_offset: 0,
            total_rows: 0,
            file_name: None,
            table_name: None,
            sort_state: SortState::default(),
        }
    }
}

/// Filter mode state: input, active filter, autocomplete.
pub struct FilterState {
    pub input: String,
    pub active_filter: Option<String>,
    pub autocomplete_suggestions: Vec<String>,
    pub autocomplete_index: usize,
    pub autocomplete_active: bool,
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            active_filter: None,
            autocomplete_suggestions: Vec::new(),
            autocomplete_index: 0,
            autocomplete_active: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Plain,
    Regex,
}

/// Search state: active search term for highlighting and n/N navigation.
pub struct SearchState {
    pub active_search: Option<String>,
    pub search_mode: SearchMode,
    pub match_count: Option<usize>,
    pub match_index: Option<usize>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            active_search: None,
            search_mode: SearchMode::Plain,
            match_count: None,
            match_index: None,
        }
    }
}

/// Per-tab state: everything that is independent per file.
pub struct TabState {
    pub viewport: Viewport,
    pub data: DataState,
    pub filter: FilterState,
    pub search: SearchState,
    pub fetch_pending: bool,
    pub search_pending: bool,
    pub pending_search_col_find: Option<(usize, String, bool)>,
}

impl TabState {
    pub fn new() -> Self {
        Self {
            viewport: Viewport::new(),
            data: DataState::new(),
            filter: FilterState::new(),
            search: SearchState::new(),
            fetch_pending: false,
            search_pending: false,
            pending_search_col_find: None,
        }
    }
}

/// SQL scratchpad state: editor lines, cursor, results.
pub struct SqlState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub result: Option<RecordBatch>,
    pub result_schema: Option<Arc<Schema>>,
    pub error: Option<String>,
}

impl SqlState {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            result: None,
            result_schema: None,
            error: None,
        }
    }
}

/// Stats overlay state: batch, schema, loading flag, scroll position.
pub struct StatsState {
    pub batch: Option<RecordBatch>,
    pub schema: Option<Arc<Schema>>,
    pub loading: bool,
    pub scroll_offset: usize,
}

impl StatsState {
    pub fn new() -> Self {
        Self {
            batch: None,
            schema: None,
            loading: false,
            scroll_offset: 0,
        }
    }
}
