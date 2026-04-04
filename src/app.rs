use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use duckdb::arrow::array::Array;
use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;
use ratatui::DefaultTerminal;

use crate::event::{Action, DataEvent, TermEvent};
use crate::input::TextInput;
use crate::input::autocomplete::AutocompleteState;
use crate::input::history::InputHistory;
use crate::mode::{AppMode, OverlayVariant};
use crate::state::{
    ColumnJumpState, ColumnPickerState, SqlState, StatsState, TabState,
};
use crate::ui::AppView;
use crate::ui::theme::Theme;
use crate::worker::Worker;

const BUFFER_MULTIPLIER: usize = 5;

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct App {
    pub mode: AppMode,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub sql: SqlState,
    pub stats: StatsState,
    pub text_input: TextInput,
    pub autocomplete: AutocompleteState,
    pub column_picker: ColumnPickerState,
    pub column_jump: ColumnJumpState,
    pub filter_history: InputHistory,
    pub status_message: Option<String>,
    pub tick: usize,
    pub wrap: bool,
    pub stdin_label: bool,
    pub theme: Theme,
    /// Layout area of the table (set by renderer, read by mouse handler).
    pub table_area: std::cell::Cell<ratatui::layout::Rect>,
    /// Y coordinate of the tab bar (0 if multi-tab, otherwise not present).
    pub tab_bar_y: std::cell::Cell<Option<u16>>,
    action_txs: Vec<Sender<Action>>,
    data_rxs: Vec<Receiver<DataEvent>>,
    term_rx: Receiver<TermEvent>,
    query_counter: usize,
}

impl App {
    pub fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.tick % SPINNER_FRAMES.len()]
    }
}

impl App {
    pub fn new(file_paths: Vec<PathBuf>, theme: Theme) -> Result<Self> {
        let (term_tx, term_rx) = mpsc::channel::<TermEvent>();

        // Spawn event thread
        thread::spawn(move || {
            event_thread(term_tx);
        });

        let mut tabs = Vec::with_capacity(file_paths.len());
        let mut action_txs = Vec::with_capacity(file_paths.len());
        let mut data_rxs = Vec::with_capacity(file_paths.len());

        for file_path in file_paths {
            let (action_tx, action_rx) = mpsc::channel::<Action>();
            let (data_tx, data_rx) = mpsc::channel::<DataEvent>();

            let worker = Worker::new(action_rx, data_tx);
            thread::spawn(move || worker.run());

            let mut tab = TabState::new();
            tab.file_path = Some(file_path.clone());
            action_tx.send(Action::LoadFile(file_path))?;

            tabs.push(tab);
            action_txs.push(action_tx);
            data_rxs.push(data_rx);
        }

        Ok(Self {
            mode: AppMode::Normal,
            tabs,
            active_tab: 0,
            sql: SqlState::new(),
            stats: StatsState::new(),
            text_input: TextInput::new(),
            autocomplete: AutocompleteState::new(),
            column_picker: ColumnPickerState::new(),
            column_jump: ColumnJumpState::new(),
            filter_history: InputHistory::new(50),
            status_message: Some("Loading...".to_string()),
            tick: 0,
            wrap: false,
            stdin_label: false,
            theme,
            table_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            tab_bar_y: std::cell::Cell::new(None),
            action_txs,
            data_rxs,
            term_rx,
            query_counter: 0,
        })
    }

    // --- Mode lifecycle (Phase 3) ---

    pub fn transition_to(&mut self, new_mode: AppMode) {
        self.exit_current_mode();
        self.mode = new_mode;
        self.enter_current_mode();
    }

    fn exit_current_mode(&mut self) {
        match &self.mode {
            AppMode::Input(_) => {
                self.text_input.clear();
                self.text_input.char_filter = None;
                self.autocomplete.clear();
                self.filter_history.reset_position();
            }
            AppMode::Overlay(OverlayVariant::Help) => {}
            AppMode::Overlay(OverlayVariant::ColumnPicker) => {
                // column_picker state is left for the caller to inspect before transitioning
            }
            AppMode::Overlay(OverlayVariant::ColumnJump) => {
                self.column_jump.clear();
            }
            AppMode::Overlay(OverlayVariant::Stats) => {
                self.stats.batch = None;
                self.stats.schema = None;
                self.stats.loading = false;
                self.status_message = None;
            }
            AppMode::Sql => {
                self.sql.result = None;
                self.sql.result_schema = None;
                self.sql.error = None;
                self.status_message = None;
            }
            _ => {}
        }
    }

    fn enter_current_mode(&mut self) {
        match &self.mode {
            AppMode::Overlay(OverlayVariant::Stats) => {
                self.stats.loading = true;
                self.stats.batch = None;
                self.stats.schema = None;
                self.stats.scroll_offset = 0;
                let table = self
                    .tab()
                    .data
                    .table_name
                    .as_deref()
                    .unwrap_or("data")
                    .to_string();
                self.status_message = Some("Loading statistics...".to_string());
                self.send_action(Action::ExecuteSql(format!("SUMMARIZE {table}")));
            }
            AppMode::Sql => {
                self.sql.error = None;
            }
            _ => {}
        }
    }

    // --- Tab accessors ---

    /// Get a reference to the active tab.
    pub fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    /// Get a mutable reference to the active tab.
    pub fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Whether multiple tabs are open (controls tab bar visibility).
    pub fn has_tabs(&self) -> bool {
        self.tabs.len() > 1
    }

    /// Offset into the buffer batch that corresponds to view_start.
    pub fn view_offset_in_buffer(&self) -> usize {
        self.tab()
            .viewport
            .view_start
            .saturating_sub(self.tab().data.buffer_offset)
    }

    /// How many rows from the buffer are available for the current view.
    pub fn visible_row_count(&self) -> usize {
        if let Some(ref batch) = self.tab().data.current_batch {
            let view_off = self.view_offset_in_buffer();
            let available = batch.num_rows().saturating_sub(view_off);
            available.min(self.tab().viewport.page_size)
        } else {
            0
        }
    }

    pub fn absolute_row(&self) -> usize {
        self.tab().viewport.selected_row
    }

    /// Compute how many columns are visible at the current terminal width and column offset.
    pub fn visible_col_count(&self) -> usize {
        use crate::ui::table_render::{
            build_formatters, compute_column_widths, sort_indicator_string, visible_columns,
        };

        let tab = self.tab();
        let schema = match tab.data.schema.as_ref() {
            Some(s) => s,
            None => return 1,
        };
        let batch = match tab.data.current_batch.as_ref() {
            Some(b) => b,
            None => return 1,
        };

        if schema.fields().is_empty() {
            return 1;
        }

        let formatters = build_formatters(batch);
        let multi = tab.data.sort_state.specs().len() > 1;
        let sort_state = &tab.data.sort_state;

        let view_off = self.view_offset_in_buffer();
        let visible_count = self.visible_row_count();
        let batch_rows = batch.num_rows();

        let (_headers, col_widths) = compute_column_widths(
            schema,
            batch,
            &formatters,
            &|_i, name, type_str| {
                let sort_ind = sort_indicator_string(sort_state, name, multi);
                format!("{} [{}]{}", name, type_str, sort_ind)
            },
            (view_off, (view_off + visible_count).min(batch_rows)),
            crate::ui::table_render::DEFAULT_MAX_COL_WIDTH,
        );

        let left_margin =
            crate::ui::table_render::gutter_width(tab.data.total_rows)
            + crate::ui::table_render::ROW_PREFIX_WIDTH;
        let hidden = &tab.hidden_columns;
        let cols = visible_columns(
            &col_widths,
            tab.viewport.terminal_width as usize,
            tab.viewport.column_offset,
            left_margin,
            if hidden.is_empty() { None } else { Some(hidden) },
        );

        cols.len().max(1)
    }

    /// Convenience: adjust column view using computed visible col count.
    pub(crate) fn adjust_column_view(&mut self) {
        let visible = self.visible_col_count();
        self.tab_mut().viewport.adjust_column_view_with(visible);
    }

    fn buffer_end(&self) -> usize {
        let tab = self.tab();
        tab.data.buffer_offset
            + tab
                .data
                .current_batch
                .as_ref()
                .map_or(0, |b| b.num_rows())
    }

    fn fetch_size_for(&self, tab_idx: usize) -> usize {
        self.tabs[tab_idx].viewport.page_size * BUFFER_MULTIPLIER
    }

    /// Send an action to the active tab's worker thread.
    pub(crate) fn send_action(&self, action: Action) {
        let _ = self.action_txs[self.active_tab].send(action);
    }

    /// Switch to another tab with wrapping.
    pub fn switch_tab(&mut self, delta: isize) {
        let len = self.tabs.len();
        self.active_tab =
            ((self.active_tab as isize + delta).rem_euclid(len as isize)) as usize;
        self.on_tab_switch();
    }

    pub fn add_tab_from_batch(&mut self, batch: RecordBatch, schema: Arc<Schema>) {
        self.query_counter += 1;
        let name = format!("Query {}", self.query_counter);

        let (action_tx, action_rx) = mpsc::channel::<Action>();
        let (data_tx, data_rx) = mpsc::channel::<DataEvent>();

        let worker = Worker::new(action_rx, data_tx);
        thread::spawn(move || worker.run());

        let _ = action_tx.send(Action::LoadBatch {
            batch,
            schema,
            name,
        });

        let tab = TabState::new();
        self.tabs.push(tab);
        self.action_txs.push(action_tx);
        self.data_rxs.push(data_rx);

        self.active_tab = self.tabs.len() - 1;
        self.mode = AppMode::Normal;
        self.sql.result = None;
        self.sql.result_schema = None;
        self.sql.error = None;
        self.status_message = Some("Loading query result...".to_string());
    }

    fn on_tab_switch(&mut self) {
        self.status_message = None;
        self.sql.result = None;
        self.sql.result_schema = None;
        self.sql.error = None;
        self.stats.batch = None;
        self.stats.schema = None;
        if matches!(self.mode, AppMode::Overlay(OverlayVariant::Stats) | AppMode::Sql) {
            self.mode = AppMode::Normal;
        }
        let row = self.tabs[self.active_tab].viewport.selected_row;
        self.request_buffer_around(row);
    }

    fn update_terminal_size(&mut self, width: u16, height: u16) {
        // Chrome: status bar (1) + table header row (1) + tab bar (1 if multi-tab)
        let chrome = if self.has_tabs() { 3 } else { 2 };
        // Subtract 1 more for the table header row within the table area
        let page_size = (height as usize).saturating_sub(chrome + 1).max(1);
        for tab in &mut self.tabs {
            tab.viewport.page_size = page_size;
            tab.viewport.terminal_width = width;
        }
    }

    /// After render, sync page_size from rendered_rows when wrap mode is active.
    fn sync_wrap_page_size(&mut self) {
        if self.wrap {
            let rendered = self.tab().viewport.rendered_rows.get();
            if rendered > 0 {
                self.tab_mut().viewport.page_size = rendered;
            }
        }
    }

    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let size = terminal.size()?;
        self.update_terminal_size(size.width, size.height);

        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                frame.render_widget(AppView::new(self), area);

                if let Some((cx, cy)) = self.mode.cursor_position(self, area.height) {
                    frame.set_cursor_position((cx, cy));
                }
            })?;

            self.sync_wrap_page_size();

            if self.mode == AppMode::Quitting {
                for tx in &self.action_txs {
                    let _ = tx.send(Action::Quit);
                }
                break;
            }

            self.process_data_events();

            match self.term_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(TermEvent::Key(key_event)) => {
                    self.status_message = None;
                    crate::handlers::handle_key(self, key_event);
                }
                Ok(TermEvent::Mouse(mouse)) => {
                    crate::handlers::mouse::handle_mouse(self, mouse);
                }
                Ok(TermEvent::Resize(w, h)) => {
                    self.update_terminal_size(w, h);
                    let row = self.tab().viewport.selected_row;
                    self.request_buffer_around(row);
                }
                Ok(TermEvent::Tick) | Err(_) => {
                    self.tick += 1;
                }
            }
        }

        Ok(())
    }

    fn process_data_events(&mut self) {
        for tab_idx in 0..self.data_rxs.len() {
            while let Ok(event) = self.data_rxs[tab_idx].try_recv() {
                self.apply_data_event(tab_idx, event);
            }
        }
    }

    fn apply_data_event(&mut self, tab_idx: usize, event: DataEvent) {
        let is_active = tab_idx == self.active_tab;
        match event {
            DataEvent::FileLoaded {
                schema,
                total_rows,
                file_name,
                table_name,
            } => self.on_file_loaded(tab_idx, is_active, schema, total_rows, file_name, table_name),
            DataEvent::PageLoaded {
                offset,
                batch,
                total_rows,
            } => self.on_page_loaded(tab_idx, is_active, offset, batch, total_rows),
            DataEvent::MatchesCollected { rows } => {
                self.on_matches_collected(tab_idx, is_active, rows)
            }
            DataEvent::SortApplied => self.on_sort_applied(tab_idx, is_active),
            DataEvent::FilterApplied { total_rows } => {
                self.on_filter_applied(tab_idx, is_active, total_rows)
            }
            DataEvent::FilterReset { total_rows } => {
                self.on_filter_reset(tab_idx, is_active, total_rows)
            }
            DataEvent::SqlResult {
                batch,
                schema,
                sql,
            } => self.on_sql_result(is_active, batch, schema, &sql),
            DataEvent::SqlError { error, sql } => self.on_sql_error(is_active, &error, &sql),
            DataEvent::Materialized { total_rows } => {
                self.on_materialized(tab_idx, is_active, total_rows)
            }
            DataEvent::Error(msg) => self.on_error(tab_idx, is_active, msg),
        }
    }

    fn on_file_loaded(
        &mut self,
        tab_idx: usize,
        is_active: bool,
        schema: Arc<Schema>,
        total_rows: usize,
        file_name: String,
        table_name: String,
    ) {
        let tab = &mut self.tabs[tab_idx];
        let num_cols = schema.fields().len();
        tab.data.field_names_lower = schema
            .fields()
            .iter()
            .map(|f| f.name().to_lowercase())
            .collect();
        tab.data.schema = Some(schema);
        tab.data.total_rows = total_rows;
        tab.data.file_name = if self.stdin_label && tab_idx == 0 {
            Some("<stdin>".to_string())
        } else {
            Some(file_name)
        };
        tab.data.table_name = Some(table_name);
        tab.fetch_pending = false;
        if tab.hidden_columns.len() != num_cols {
            tab.hidden_columns = vec![false; num_cols];
        }
        if is_active {
            self.status_message = None;
        }
        self.request_buffer_around_for(tab_idx, 0);
    }

    fn on_page_loaded(
        &mut self,
        tab_idx: usize,
        is_active: bool,
        offset: usize,
        batch: RecordBatch,
        total_rows: usize,
    ) {
        let tab = &mut self.tabs[tab_idx];
        tab.data.buffer_offset = offset;
        tab.data.current_batch = Some(batch);
        tab.data.total_rows = total_rows;
        tab.data.batch_generation += 1;
        tab.fetch_pending = false;
        if is_active {
            self.try_resolve_search_column();
        }
    }

    fn on_matches_collected(&mut self, tab_idx: usize, is_active: bool, rows: Vec<usize>) {
        let tab = &mut self.tabs[tab_idx];
        tab.search_pending = false;
        let count = rows.len();
        tab.search.match_count = Some(count);
        tab.search.match_rows = rows;
        if is_active {
            if count > 0 {
                self.status_message = Some(format!("{count} matches found"));
            } else {
                self.status_message = Some("No matches found".to_string());
            }
        }
    }

    fn on_sort_applied(&mut self, tab_idx: usize, is_active: bool) {
        let tab = &mut self.tabs[tab_idx];
        tab.fetch_pending = false;
        tab.pending_search_col_find = None;
        if is_active {
            self.status_message = None;
        }
        self.refresh_search(tab_idx);
    }

    fn on_filter_applied(&mut self, tab_idx: usize, is_active: bool, total_rows: usize) {
        let tab = &mut self.tabs[tab_idx];
        tab.data.total_rows = total_rows;
        tab.viewport.selected_row = 0;
        tab.viewport.view_start = 0;
        tab.data.buffer_offset = 0;
        // active_filter is already set by the input handler before sending the action
        tab.fetch_pending = false;
        tab.pending_search_col_find = None;
        if is_active {
            self.status_message = Some(format!("Filter applied: {total_rows} rows match"));
        }
        self.refresh_search(tab_idx);
    }

    fn on_filter_reset(&mut self, tab_idx: usize, is_active: bool, total_rows: usize) {
        let tab = &mut self.tabs[tab_idx];
        tab.data.total_rows = total_rows;
        tab.viewport.selected_row = 0;
        tab.viewport.view_start = 0;
        tab.data.buffer_offset = 0;
        tab.filter.active_filter = None;
        tab.fetch_pending = false;
        tab.pending_search_col_find = None;
        if is_active {
            self.status_message = Some("Filter cleared".to_string());
        }
        self.refresh_search(tab_idx);
    }

    fn on_sql_result(
        &mut self,
        is_active: bool,
        batch: RecordBatch,
        schema: Arc<Schema>,
        sql: &str,
    ) {
        if !is_active {
            return;
        }
        let is_summarize = sql.trim_start().to_uppercase().starts_with("SUMMARIZE");
        if is_summarize && matches!(self.mode, AppMode::Overlay(OverlayVariant::Stats)) {
            let num_rows = batch.num_rows();
            self.stats.schema = Some(schema);
            self.stats.batch = Some(batch);
            self.stats.loading = false;
            self.status_message = Some(format!("Statistics: {num_rows} columns"));
        } else {
            let num_rows = batch.num_rows();
            self.sql.result_schema = Some(schema);
            self.sql.result = Some(batch);
            self.sql.error = None;
            self.status_message = Some(format!("SQL result: {num_rows} rows"));
        }
    }

    fn on_sql_error(&mut self, is_active: bool, error: &str, sql: &str) {
        if !is_active {
            return;
        }
        let is_summarize = sql.trim_start().to_uppercase().starts_with("SUMMARIZE");
        if is_summarize && matches!(self.mode, AppMode::Overlay(OverlayVariant::Stats)) {
            self.stats.batch = None;
            self.stats.schema = None;
            self.stats.loading = false;
            self.status_message = Some(format!("Stats error: {error}"));
        } else {
            self.sql.result = None;
            self.sql.result_schema = None;
            self.sql.error = Some(error.to_string());
            self.status_message = Some(format!("SQL error: {error}"));
        }
    }

    fn on_materialized(&mut self, tab_idx: usize, is_active: bool, total_rows: usize) {
        let tab = &mut self.tabs[tab_idx];
        tab.data.total_rows = total_rows;
        tab.fetch_pending = false;
        if is_active {
            self.status_message = Some("Indexed".to_string());
        }
    }

    fn on_error(&mut self, tab_idx: usize, is_active: bool, msg: String) {
        self.tabs[tab_idx].fetch_pending = false;
        if msg.starts_with("Invalid filter:") {
            self.tabs[tab_idx].filter.active_filter = None;
        }
        if is_active {
            self.status_message = Some(format!("Error: {msg}"));
        }
    }

    pub(crate) fn move_down(&mut self, n: usize) {
        let total = self.tab().data.total_rows;
        self.tab_mut().viewport.move_cursor_down(n, total);
        self.ensure_buffer();
    }

    pub(crate) fn move_up(&mut self, n: usize) {
        self.tab_mut().viewport.move_cursor_up(n);
        self.ensure_buffer();
    }

    /// Check if we need to fetch more data.
    pub(crate) fn ensure_buffer(&mut self) {
        let buf_end = self.buffer_end();
        let tab = self.tab();

        let outside_buffer = tab.viewport.selected_row < tab.data.buffer_offset
            || tab.viewport.selected_row >= buf_end;

        if tab.fetch_pending && !outside_buffer {
            return;
        }

        let threshold = tab.viewport.page_size;

        let needs_fetch = outside_buffer
            || (tab.data.buffer_offset > 0
                && tab.viewport.selected_row < tab.data.buffer_offset + threshold)
            || (buf_end < tab.data.total_rows
                && tab.viewport.selected_row + threshold >= buf_end);

        if needs_fetch {
            let row = self.tab().viewport.selected_row;
            self.request_buffer_around(row);
        }
    }

    pub(crate) fn request_buffer_around(&mut self, center_row: usize) {
        self.request_buffer_around_for(self.active_tab, center_row);
    }

    fn request_buffer_around_for(&mut self, tab_idx: usize, center_row: usize) {
        let fetch_size = self.fetch_size_for(tab_idx);
        let new_offset = center_row.saturating_sub(fetch_size / 2);
        self.tabs[tab_idx].fetch_pending = true;
        let _ = self.action_txs[tab_idx].send(Action::FetchPage {
            offset: new_offset,
            limit: fetch_size,
        });
    }

    /// Re-collect search matches if there's an active search (after sort/filter change).
    fn refresh_search(&mut self, tab_idx: usize) {
        let tab = &mut self.tabs[tab_idx];
        if let Some(ref term) = tab.search.active_search {
            let term = term.clone();
            tab.search.match_rows.clear();
            tab.search.match_count = None;
            tab.search.match_index = None;
            tab.search_pending = true;
            self.send_action(Action::CollectMatches(term));
        } else {
            tab.search_pending = false;
        }
    }

    fn try_resolve_search_column(&mut self) {
        let tab = self.tab();
        let (target_row, term) = match tab.pending_search_col_find.as_ref() {
            Some(v) => v.clone(),
            None => return,
        };

        let batch = match tab.data.current_batch.as_ref() {
            Some(b) => b,
            None => return,
        };
        let schema = match tab.data.schema.as_ref() {
            Some(s) => s,
            None => return,
        };

        // Check if target row is in the current buffer
        if target_row < tab.data.buffer_offset
            || target_row >= tab.data.buffer_offset + batch.num_rows()
        {
            return;
        }

        let batch_row = target_row - tab.data.buffer_offset;
        let re = regex::RegexBuilder::new(&term)
            .case_insensitive(true)
            .build()
            .ok();

        let formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
            (0..batch.num_columns())
                .map(|i| {
                    duckdb::arrow::util::display::ArrayFormatter::try_new(
                        batch.column(i).as_ref(),
                        &Default::default(),
                    )
                    .ok()
                })
                .collect();

        let mut found_col: Option<usize> = None;
        for (col_idx, _field) in schema.fields().iter().enumerate() {
            if col_idx >= formatters.len() {
                continue;
            }
            let column = batch.column(col_idx);
            if column.is_null(batch_row) {
                continue;
            }
            if let Some(ref fmt) = formatters[col_idx] {
                let val = fmt.value(batch_row).to_string();
                if re.as_ref().map_or(false, |r| r.is_match(&val)) {
                    found_col = Some(col_idx);
                    break;
                }
            }
        }

        // Drop borrows before mutable operations
        drop(formatters);
        self.tab_mut().pending_search_col_find = None;

        if let Some(col_idx) = found_col {
            self.tab_mut().viewport.selected_col = col_idx;
            self.adjust_column_view();
        }
    }

    pub(crate) fn cycle_sort(&mut self) {
        let schema = match self.tab().data.schema.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };

        let fields = schema.fields();
        if fields.is_empty() {
            return;
        }

        let col_idx = self.tab().viewport.selected_col.min(fields.len() - 1);
        let col_name = fields[col_idx].name().clone();

        self.tab_mut().data.sort_state.toggle(&col_name);

        // Show feedback
        let sort_state = &self.tab().data.sort_state;
        if let Some(order) = sort_state.order_for(&col_name) {
            self.status_message = Some(format!("Sort: {} {}", col_name, order.as_sql().to_lowercase()));
        } else {
            self.status_message = Some(format!("Sort cleared: {col_name}"));
        }

        let sort_state = self.tab().data.sort_state.clone();
        let _ = self.action_txs[self.active_tab]
            .send(Action::ApplySort(sort_state));
    }

    /// Export marked rows as CSV. Returns None if no rows are marked.
    pub fn marked_row_csv(&self) -> Option<String> {
        let tab = self.tab();
        if tab.marked_rows.is_empty() {
            return None;
        }
        let schema = tab.data.schema.as_ref()?;
        let batch = tab.data.current_batch.as_ref()?;
        let formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
            (0..batch.num_columns())
                .map(|i| {
                    duckdb::arrow::util::display::ArrayFormatter::try_new(
                        batch.column(i).as_ref(),
                        &Default::default(),
                    )
                    .ok()
                })
                .collect();

        let fields = schema.fields();
        let mut csv = String::new();

        // Header
        let header: Vec<&str> = fields.iter().map(|f| f.name().as_str()).collect();
        csv.push_str(&header.join(","));
        csv.push('\n');

        // Rows (sorted by row index)
        let mut sorted_rows: Vec<usize> = tab.marked_rows.iter().copied().collect();
        sorted_rows.sort();

        for abs_row in sorted_rows {
            // Convert to buffer-relative row
            if abs_row < tab.data.buffer_offset {
                continue;
            }
            let buf_row = abs_row - tab.data.buffer_offset;
            if buf_row >= batch.num_rows() {
                continue;
            }

            let mut cells = Vec::with_capacity(fields.len());
            for col_idx in 0..fields.len() {
                let column = batch.column(col_idx);
                if column.is_null(buf_row) {
                    cells.push(String::new());
                } else if let Some(ref fmt) = formatters[col_idx] {
                    let val = fmt.value(buf_row).to_string();
                    // CSV-escape values containing commas, quotes, or newlines
                    if val.contains(',') || val.contains('"') || val.contains('\n') {
                        cells.push(format!("\"{}\"", val.replace('"', "\"\"")));
                    } else {
                        cells.push(val);
                    }
                } else {
                    cells.push(String::new());
                }
            }
            csv.push_str(&cells.join(","));
            csv.push('\n');
        }

        Some(csv)
    }
}

fn event_thread(tx: Sender<TermEvent>) {
    loop {
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) => {
                    if tx.send(TermEvent::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    if tx.send(TermEvent::Mouse(mouse)).is_err() {
                        break;
                    }
                }
                Ok(Event::Resize(w, h)) => {
                    if tx.send(TermEvent::Resize(w, h)).is_err() {
                        break;
                    }
                }
                _ => {}
            },
            Ok(false) => {
                if tx.send(TermEvent::Tick).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
