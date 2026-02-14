use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use duckdb::arrow::array::Array;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::DefaultTerminal;

use crate::event::{Action, DataEvent, TermEvent};
use crate::ui::sql_pad::SQL_PAD_HEIGHT;
use crate::state::{
    SearchMode, SqlState, StatsState, TabState,
};
use crate::ui::AppView;
use crate::worker::Worker;

const BUFFER_MULTIPLIER: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Search,
    Regex,
    Filter,
    Sql,
    Stats,
    Quitting,
}

impl AppMode {
    pub fn badge(&self) -> &'static str {
        match self {
            AppMode::Normal => " NORMAL ",
            AppMode::Search => " SEARCH ",
            AppMode::Regex => " REGEX ",
            AppMode::Filter => " FILTER ",
            AppMode::Sql => " SQL ",
            AppMode::Stats => " STATS ",
            AppMode::Quitting => " QUIT ",
        }
    }

    pub fn badge_style(&self) -> Style {
        match self {
            AppMode::Normal => Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            AppMode::Search => Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            AppMode::Regex => Style::default()
                .bg(Color::Magenta)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            AppMode::Filter => Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            AppMode::Sql => Style::default()
                .bg(Color::Magenta)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            AppMode::Stats => Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            AppMode::Quitting => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        }
    }

    pub fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            AppMode::Normal => &[
                ("Tab", "mode"),
                ("q", "quit"),
                ("/", "search"),
                ("$", "regex"),
                ("f", "filter"),
                ("s", "sort"),
                ("S", "stats"),
                ("e", "sql"),
            ],
            AppMode::Search | AppMode::Regex => &[("Enter", "apply"), ("Esc", "cancel")],
            AppMode::Filter => &[
                ("Enter", "apply"),
                ("Esc", "cancel"),
                ("$col", "autocomplete"),
                ("Tab", "complete"),
            ],
            AppMode::Sql => &[("F5/Ctrl-e", "execute"), ("Esc", "cancel")],
            AppMode::Stats => &[("j/k", "scroll"), ("g/G", "top/bottom"), ("Esc", "close")],
            AppMode::Quitting => &[],
        }
    }

    pub fn hints_string(&self) -> String {
        self.hints()
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn layout_constraints(&self) -> Vec<Constraint> {
        match self {
            AppMode::Stats => vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Length(1),
            ],
            AppMode::Sql => vec![
                Constraint::Min(3),
                Constraint::Length(SQL_PAD_HEIGHT),
                Constraint::Length(1),
            ],
            AppMode::Search | AppMode::Regex | AppMode::Filter => vec![
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ],
            _ => vec![Constraint::Min(3), Constraint::Length(1)],
        }
    }

    pub fn cursor_position(&self, app: &App, area_height: u16) -> Option<(u16, u16)> {
        match self {
            AppMode::Search | AppMode::Regex | AppMode::Filter => {
                let filter_y = area_height - 2;
                let cursor_x = 8 + app.tab().filter.input.len() as u16;
                Some((cursor_x, filter_y))
            }
            AppMode::Sql => {
                let sql_input_start_y = area_height.saturating_sub(6);
                let cursor_y = sql_input_start_y + app.sql.cursor_row as u16;
                let cursor_x = 4 + app.sql.cursor_col as u16;
                Some((cursor_x, cursor_y))
            }
            _ => None,
        }
    }
}

pub struct App {
    pub mode: AppMode,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub sql: SqlState,
    pub stats: StatsState,
    pub status_message: Option<String>,
    action_txs: Vec<Sender<Action>>,
    data_rxs: Vec<Receiver<DataEvent>>,
    term_rx: Receiver<TermEvent>,
}

impl App {
    pub fn new(file_paths: Vec<PathBuf>) -> Result<Self> {
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

            action_tx.send(Action::LoadFile(file_path))?;

            tabs.push(TabState::new());
            action_txs.push(action_tx);
            data_rxs.push(data_rx);
        }

        Ok(Self {
            mode: AppMode::Normal,
            tabs,
            active_tab: 0,
            sql: SqlState::new(),
            stats: StatsState::new(),
            status_message: Some("Loading...".to_string()),
            action_txs,
            data_rxs,
            term_rx,
        })
    }

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
        use crate::ui::table::subscript_digit;
        use crate::ui::table_render::{build_formatters, compute_column_widths, visible_columns};

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
                let sort_ind = if let Some(pos) = sort_state.position(name) {
                    let order = sort_state.order_for(name).unwrap();
                    let arrow = order.indicator();
                    if multi {
                        format!(" {}{}", arrow, subscript_digit(pos + 1))
                    } else {
                        format!(" {}", arrow)
                    }
                } else {
                    String::new()
                };
                format!("{} [{}]{}", name, type_str, sort_ind)
            },
            (view_off, (view_off + visible_count).min(batch_rows)),
            50,
        );

        let row_prefix_width = 3;
        let cols = visible_columns(
            &col_widths,
            tab.viewport.terminal_width as usize,
            tab.viewport.column_offset,
            row_prefix_width,
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

    fn on_tab_switch(&mut self) {
        self.status_message = None;
        self.sql.result = None;
        self.sql.result_schema = None;
        self.sql.error = None;
        self.stats.batch = None;
        self.stats.schema = None;
        if matches!(self.mode, AppMode::Stats | AppMode::Sql) {
            self.mode = AppMode::Normal;
        }
        let row = self.tabs[self.active_tab].viewport.selected_row;
        self.request_buffer_around(row);
    }

    fn update_terminal_size(&mut self, width: u16, height: u16) {
        let chrome = if self.has_tabs() { 3 } else { 2 }; // +1 for tab bar
        let page_size = (height as usize).saturating_sub(chrome).max(1);
        for tab in &mut self.tabs {
            tab.viewport.page_size = page_size;
            tab.viewport.terminal_width = width;
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
                    let offset_y = if self.has_tabs() { 1u16 } else { 0 };
                    frame.set_cursor_position((cx, cy + offset_y));
                }
            })?;

            if self.mode == AppMode::Quitting {
                for tx in &self.action_txs {
                    let _ = tx.send(Action::Quit);
                }
                break;
            }

            self.process_data_events();

            match self.term_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(TermEvent::Key(key_event)) => {
                    crate::handlers::handle_key(self, key_event);
                }
                Ok(TermEvent::Resize(w, h)) => {
                    self.update_terminal_size(w, h);
                    let row = self.tab().viewport.selected_row;
                    self.request_buffer_around(row);
                }
                Ok(TermEvent::Tick) | Err(_) => {}
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
            } => {
                let tab = &mut self.tabs[tab_idx];
                tab.data.schema = Some(schema);
                tab.data.total_rows = total_rows;
                tab.data.file_name = Some(file_name);
                tab.data.table_name = Some(table_name);
                tab.fetch_pending = false;
                if is_active {
                    self.status_message = None;
                }
                self.request_buffer_around_for(tab_idx, 0);
            }
            DataEvent::PageLoaded {
                offset,
                batch,
                total_rows,
            } => {
                let tab = &mut self.tabs[tab_idx];
                tab.data.buffer_offset = offset;
                tab.data.current_batch = Some(batch);
                tab.data.total_rows = total_rows;
                tab.fetch_pending = false;
                if is_active {
                    self.try_resolve_search_column();
                }
            }
            DataEvent::MatchFound { row, match_index } => {
                let tab = &mut self.tabs[tab_idx];
                tab.search_pending = false;
                tab.search.match_index = match_index;
                tab.viewport.selected_row = row;
                tab.viewport.adjust_view();

                if let Some(ref term) = tab.search.active_search {
                    let is_regex = tab.search.search_mode == SearchMode::Regex;
                    tab.pending_search_col_find =
                        Some((row, term.clone(), is_regex));
                }

                if is_active {
                    self.ensure_buffer();
                    self.try_resolve_search_column();
                    self.status_message = Some(format!("Match at row {}", row + 1));
                }
            }
            DataEvent::MatchNotFound => {
                self.tabs[tab_idx].search_pending = false;
                if is_active {
                    self.status_message = Some("No match found".to_string());
                }
            }
            DataEvent::MatchCount { count } => {
                self.tabs[tab_idx].search.match_count = Some(count);
            }
            DataEvent::SortApplied => {
                let tab = &mut self.tabs[tab_idx];
                tab.fetch_pending = false;
                tab.search_pending = false;
                tab.pending_search_col_find = None;
                if is_active {
                    self.status_message = None;
                }
            }
            DataEvent::FilterApplied { total_rows } => {
                let tab = &mut self.tabs[tab_idx];
                tab.data.total_rows = total_rows;
                tab.viewport.selected_row = 0;
                tab.viewport.view_start = 0;
                tab.data.buffer_offset = 0;
                tab.filter.active_filter = Some(tab.filter.input.clone());
                tab.fetch_pending = false;
                tab.search_pending = false;
                tab.pending_search_col_find = None;
                if is_active {
                    self.status_message =
                        Some(format!("Filter applied: {total_rows} rows match"));
                }
            }
            DataEvent::FilterReset { total_rows } => {
                let tab = &mut self.tabs[tab_idx];
                tab.data.total_rows = total_rows;
                tab.viewport.selected_row = 0;
                tab.viewport.view_start = 0;
                tab.data.buffer_offset = 0;
                tab.filter.active_filter = None;
                tab.fetch_pending = false;
                tab.search_pending = false;
                tab.pending_search_col_find = None;
                if is_active {
                    self.status_message = Some("Filter cleared".to_string());
                }
            }
            DataEvent::SqlResult {
                batch,
                schema,
                ref sql,
            } => {
                if is_active {
                    let is_summarize = sql
                        .trim_start()
                        .to_uppercase()
                        .starts_with("SUMMARIZE");
                    if is_summarize && self.mode == AppMode::Stats {
                        let num_rows = batch.num_rows();
                        self.stats.schema = Some(schema);
                        self.stats.batch = Some(batch);
                        self.stats.loading = false;
                        self.status_message =
                            Some(format!("Statistics: {num_rows} columns"));
                    } else {
                        let num_rows = batch.num_rows();
                        self.sql.result_schema = Some(schema);
                        self.sql.result = Some(batch);
                        self.sql.error = None;
                        self.status_message =
                            Some(format!("SQL result: {num_rows} rows"));
                    }
                }
            }
            DataEvent::SqlError { ref error, ref sql } => {
                if is_active {
                    let is_summarize = sql
                        .trim_start()
                        .to_uppercase()
                        .starts_with("SUMMARIZE");
                    if is_summarize && self.mode == AppMode::Stats {
                        self.stats.batch = None;
                        self.stats.schema = None;
                        self.stats.loading = false;
                        self.status_message =
                            Some(format!("Stats error: {error}"));
                    } else {
                        self.sql.result = None;
                        self.sql.result_schema = None;
                        self.sql.error = Some(error.clone());
                        self.status_message =
                            Some(format!("SQL error: {error}"));
                    }
                }
            }
            DataEvent::Error(msg) => {
                self.tabs[tab_idx].fetch_pending = false;
                if is_active {
                    self.status_message = Some(format!("Error: {msg}"));
                }
            }
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

    fn try_resolve_search_column(&mut self) {
        let tab = self.tab();
        let (target_row, term, is_regex) = match tab.pending_search_col_find.as_ref() {
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
        let term_lower = term.to_lowercase();
        let re = if is_regex {
            regex::RegexBuilder::new(&term)
                .case_insensitive(true)
                .build()
                .ok()
        } else {
            None
        };

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
                let matches = if is_regex {
                    re.as_ref().map_or(false, |r| r.is_match(&val))
                } else {
                    val.to_lowercase().contains(&term_lower)
                };
                if matches {
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
        let sort_state = self.tab().data.sort_state.clone();
        let _ = self.action_txs[self.active_tab]
            .send(Action::ApplySort(sort_state));
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
