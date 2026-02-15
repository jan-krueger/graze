use std::path::PathBuf;
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
    pub col_width_overrides: Vec<i16>,
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
            col_width_overrides: Vec::new(),
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
            // Unknown total (VIEW phase) — allow scrolling without clamping.
            self.selected_row += n;
        } else {
            self.selected_row = (self.selected_row + n).min(total_rows - 1);
        }
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
    pub cursor_pos: usize,
    pub active_filter: Option<String>,
    pub autocomplete_suggestions: Vec<String>,
    pub autocomplete_index: usize,
    pub autocomplete_active: bool,
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            active_filter: None,
            autocomplete_suggestions: Vec::new(),
            autocomplete_index: 0,
            autocomplete_active: false,
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

    pub fn insert_at_cursor(&mut self, c: char) {
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
    /// Cached list of all matching row indices (sorted ascending).
    pub match_rows: Vec<usize>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            active_search: None,
            search_mode: SearchMode::Plain,
            match_count: None,
            match_index: None,
            match_rows: Vec::new(),
        }
    }

    /// Find the next match after `current_row` (wraps around).
    pub fn next_match(&self, current_row: usize) -> Option<(usize, usize)> {
        if self.match_rows.is_empty() {
            return None;
        }
        // Find first row > current_row
        match self.match_rows.binary_search(&(current_row + 1)) {
            Ok(idx) => Some((idx, self.match_rows[idx])),
            Err(idx) => {
                if idx < self.match_rows.len() {
                    Some((idx, self.match_rows[idx]))
                } else {
                    // Wrap to beginning
                    Some((0, self.match_rows[0]))
                }
            }
        }
    }

    /// Find the previous match before `current_row` (wraps around).
    pub fn prev_match(&self, current_row: usize) -> Option<(usize, usize)> {
        if self.match_rows.is_empty() {
            return None;
        }
        // Find last row < current_row
        match self.match_rows.binary_search(&current_row) {
            Ok(idx) => {
                if idx > 0 {
                    Some((idx - 1, self.match_rows[idx - 1]))
                } else {
                    // Wrap to end
                    let last = self.match_rows.len() - 1;
                    Some((last, self.match_rows[last]))
                }
            }
            Err(idx) => {
                if idx > 0 {
                    Some((idx - 1, self.match_rows[idx - 1]))
                } else {
                    let last = self.match_rows.len() - 1;
                    Some((last, self.match_rows[last]))
                }
            }
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
    pub file_path: Option<PathBuf>,
    pub hidden_columns: Vec<bool>,
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
            file_path: None,
            hidden_columns: Vec::new(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMarker {
    OnlyA,
    OnlyB,
    Common,
    Changed,
}

#[derive(Debug, Clone, Default)]
pub struct DiffCounts {
    pub only_a: usize,
    pub only_b: usize,
    pub common: usize,
    pub changed: usize,
}

/// Find the next visible (non-hidden) column in the given direction.
/// Returns `current` if no visible column is found.
pub fn next_visible_col(current: usize, direction: isize, hidden: &[bool], max_col: usize) -> usize {
    let mut col = current as isize + direction;
    while col >= 0 && (col as usize) <= max_col {
        if !hidden.get(col as usize).copied().unwrap_or(false) {
            return col as usize;
        }
        col += direction;
    }
    current
}

/// Find the first visible (non-hidden) column at or after `start`.
pub fn first_visible_col(hidden: &[bool], max_col: usize) -> usize {
    for i in 0..=max_col {
        if !hidden.get(i).copied().unwrap_or(false) {
            return i;
        }
    }
    0
}

/// Column picker state used by both diff setup steps.
pub struct DiffSetup {
    pub columns: Vec<String>,
    pub selected: Vec<bool>,
    pub cursor: usize,
}

impl DiffSetup {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            selected: Vec::new(),
            cursor: 0,
        }
    }
}

/// Full diff view state.
pub struct DiffState {
    pub batch: Option<RecordBatch>,
    pub schema: Option<Arc<Schema>>,
    pub markers: Vec<DiffMarker>,
    pub changed_cells: Vec<Vec<bool>>,
    pub counts: DiffCounts,
    pub loading: bool,
    pub error: Option<String>,
    pub scroll_offset: usize,
    pub column_offset: usize,
    pub file_a: String,
    pub file_b: String,
    pub key_columns: Vec<String>,
    pub diff_columns: Vec<String>,
    pub setup: DiffSetup,
    pub hide_common: bool,
    /// Indices of visible rows when `hide_common` is true (non-Common rows).
    pub visible_rows: Vec<usize>,
}

impl DiffState {
    pub fn new() -> Self {
        Self {
            batch: None,
            schema: None,
            markers: Vec::new(),
            changed_cells: Vec::new(),
            counts: DiffCounts::default(),
            loading: false,
            error: None,
            scroll_offset: 0,
            column_offset: 0,
            file_a: String::new(),
            file_b: String::new(),
            key_columns: Vec::new(),
            diff_columns: Vec::new(),
            setup: DiffSetup::new(),
            hide_common: false,
            visible_rows: Vec::new(),
        }
    }

    /// Rebuild the visible_rows index based on hide_common toggle.
    pub fn rebuild_visible_rows(&mut self) {
        if self.hide_common {
            self.visible_rows = self
                .markers
                .iter()
                .enumerate()
                .filter(|(_, m)| **m != DiffMarker::Common)
                .map(|(i, _)| i)
                .collect();
        } else {
            self.visible_rows.clear();
        }
    }

    /// Total rows visible in the current view (filtered or full).
    pub fn visible_row_count(&self) -> usize {
        if self.hide_common {
            self.visible_rows.len()
        } else {
            self.batch.as_ref().map_or(0, |b| b.num_rows())
        }
    }

    /// Map a display row index to the actual data row index.
    pub fn display_to_data_row(&self, display_row: usize) -> usize {
        if self.hide_common {
            self.visible_rows.get(display_row).copied().unwrap_or(0)
        } else {
            display_row
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
