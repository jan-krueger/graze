pub mod column_picker;

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;

use crate::event::SortState;

pub use self::column_picker::ColumnPickerState;

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
    /// Number of leftmost columns frozen (pinned) on the left side.
    pub frozen_cols: usize,
    /// Feedback from renderer: how many data rows actually fit on screen (used by wrap mode).
    pub rendered_rows: std::cell::Cell<usize>,
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
            frozen_cols: 0,
            rendered_rows: std::cell::Cell::new(0),
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
    /// Pre-lowercased field names for autocomplete matching.
    pub field_names_lower: Vec<String>,
    /// Monotonically increasing counter, bumped when the batch changes.
    pub batch_generation: u64,
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
            field_names_lower: Vec::new(),
            batch_generation: 0,
        }
    }
}

/// Cached column widths to avoid recomputing every frame.
pub struct ColWidthCache {
    pub generation: u64,
    pub sample_start: usize,
    pub sample_end: usize,
    pub headers: Vec<String>,
    pub col_widths: Vec<u16>,
}

/// Filter mode state: active filter expression.
/// Text editing is now handled by `TextInput` on `App`.
/// Autocomplete is handled by `AutocompleteState` on `App`.
pub struct FilterState {
    pub active_filter: Option<String>,
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            active_filter: None,
        }
    }
}

/// Search state: active search term for highlighting and n/N navigation.
/// Search always uses regex (plain text is valid regex).
pub struct SearchState {
    pub active_search: Option<String>,
    pub match_count: Option<usize>,
    pub match_index: Option<usize>,
    /// Cached list of all matching row indices (sorted ascending).
    pub match_rows: Vec<usize>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            active_search: None,
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
    pub pending_search_col_find: Option<(usize, String)>,
    pub file_path: Option<PathBuf>,
    pub hidden_columns: Vec<bool>,
    pub col_width_cache: RefCell<Option<ColWidthCache>>,
    pub marked_rows: HashSet<usize>,
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
            col_width_cache: RefCell::new(None),
            marked_rows: HashSet::new(),
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

/// Full diff view state.
pub struct DiffState {
    pub backend: Option<crate::diff::DiffBackend>,
    pub page: Option<crate::diff::DiffPageData>,
    pub schema: Option<Arc<Schema>>,
    pub markers: Vec<DiffMarker>,
    pub counts: DiffCounts,
    pub total_rows: usize,
    /// Indices of all non-Common data rows (for n/N navigation).
    pub changed_row_indices: Vec<usize>,
    /// For each display column: Some(idx in b_side_batch) if it's a diff col, None otherwise.
    pub b_side_col_map: Vec<Option<usize>>,
    pub loading: bool,
    pub error: Option<String>,
    /// Viewport start (first visible display row).
    pub scroll_offset: usize,
    /// Cursor position (display row index).
    pub selected_row: usize,
    pub column_offset: usize,
    pub selected_col: usize,
    pub file_a: String,
    pub file_b: String,
    pub key_columns: Vec<String>,
    pub diff_columns: Vec<String>,
    pub hide_common: bool,
    /// Indices of visible rows when `hide_common` is true (non-Common rows).
    pub visible_rows: Vec<usize>,
}

impl DiffState {
    pub fn new() -> Self {
        Self {
            backend: None,
            page: None,
            schema: None,
            markers: Vec::new(),
            counts: DiffCounts::default(),
            total_rows: 0,
            changed_row_indices: Vec::new(),
            b_side_col_map: Vec::new(),
            loading: false,
            error: None,
            scroll_offset: 0,
            selected_row: 0,
            column_offset: 0,
            selected_col: 0,
            file_a: String::new(),
            file_b: String::new(),
            key_columns: Vec::new(),
            diff_columns: Vec::new(),
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
            self.total_rows
        }
    }

    /// Keep selected_row visible by scrolling the viewport.
    pub fn adjust_view(&mut self, page_size: usize) {
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        } else if self.selected_row >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected_row.saturating_sub(page_size.saturating_sub(1));
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

/// Column jump overlay state: fuzzy-find a column and jump to it.
pub struct ColumnJumpState {
    pub query: String,
    /// All columns: (original_col_idx, name).
    pub all_columns: Vec<(usize, String)>,
    /// Indices into `all_columns` that match the current query.
    pub filtered: Vec<usize>,
    /// Cursor position within `filtered`.
    pub cursor: usize,
}

impl ColumnJumpState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            all_columns: Vec::new(),
            filtered: Vec::new(),
            cursor: 0,
        }
    }

    /// Populate with column names from schema.
    pub fn populate(&mut self, columns: Vec<(usize, String)>) {
        self.all_columns = columns;
        self.query.clear();
        self.cursor = 0;
        self.refilter();
    }

    /// Refilter columns based on current query (case-insensitive substring, prefix preferred).
    pub fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        if q.is_empty() {
            self.filtered = (0..self.all_columns.len()).collect();
        } else {
            // Separate prefix matches and substring matches
            let mut prefix = Vec::new();
            let mut substring = Vec::new();
            for (i, (_col_idx, name)) in self.all_columns.iter().enumerate() {
                let lower = name.to_lowercase();
                if lower.starts_with(&q) {
                    prefix.push(i);
                } else if lower.contains(&q) {
                    substring.push(i);
                }
            }
            prefix.extend(substring);
            self.filtered = prefix;
        }
        // Clamp cursor
        if !self.filtered.is_empty() {
            self.cursor = self.cursor.min(self.filtered.len() - 1);
        } else {
            self.cursor = 0;
        }
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    /// Get the original column index of the selected item, if any.
    pub fn selected_col_idx(&self) -> Option<usize> {
        let &all_idx = self.filtered.get(self.cursor)?;
        Some(self.all_columns[all_idx].0)
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.all_columns.clear();
        self.filtered.clear();
        self.cursor = 0;
    }
}

/// Stats overlay state: batch, schema, loading flag, scroll position.
pub struct StatsState {
    pub batch: Option<RecordBatch>,
    pub schema: Option<Arc<Schema>>,
    pub loading: bool,
    pub scroll_offset: usize,
    /// Number of data rows visible in the popup (set by renderer, read by handler).
    pub visible_rows: std::cell::Cell<usize>,
}

impl StatsState {
    pub fn new() -> Self {
        Self {
            batch: None,
            schema: None,
            loading: false,
            scroll_offset: 0,
            visible_rows: std::cell::Cell::new(0),
        }
    }
}
