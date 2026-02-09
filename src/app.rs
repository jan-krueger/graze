use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::DefaultTerminal;

use crate::event::{Action, DataEvent, TermEvent};
use crate::ui::sql_pad::SQL_PAD_HEIGHT;
use crate::state::{
    DataState, FilterState, SearchState, SqlState, StatsState, Viewport,
};
use crate::ui::AppView;
use crate::worker::Worker;

const BUFFER_MULTIPLIER: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Filter,
    Sql,
    Stats,
    Quitting,
}

impl AppMode {
    pub fn badge(&self) -> &'static str {
        match self {
            AppMode::Normal => " NORMAL ",
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
                ("/", "query"),
                ("f", "col-filter"),
                ("s", "sort"),
                ("S", "stats"),
                ("e", "sql"),
            ],
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
            AppMode::Filter => vec![
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ],
            _ => vec![Constraint::Min(3), Constraint::Length(1)],
        }
    }

    pub fn cursor_position(&self, app: &App, area_height: u16) -> Option<(u16, u16)> {
        match self {
            AppMode::Filter => {
                let filter_y = area_height - 2;
                let cursor_x = 8 + app.filter.input.len() as u16;
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
    pub viewport: Viewport,
    pub data: DataState,
    pub filter: FilterState,
    pub search: SearchState,
    pub sql: SqlState,
    pub stats: StatsState,
    pub status_message: Option<String>,
    pub fetch_pending: bool,
    action_tx: Sender<Action>,
    data_rx: Receiver<DataEvent>,
    term_rx: Receiver<TermEvent>,
}

impl App {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::channel::<Action>();
        let (data_tx, data_rx) = mpsc::channel::<DataEvent>();
        let (term_tx, term_rx) = mpsc::channel::<TermEvent>();

        // Spawn worker thread
        let worker = Worker::new(action_rx, data_tx);
        thread::spawn(move || worker.run());

        // Spawn event thread
        thread::spawn(move || {
            event_thread(term_tx);
        });

        // Send initial load
        action_tx.send(Action::LoadFile(file_path))?;

        Ok(Self {
            mode: AppMode::Normal,
            viewport: Viewport::new(),
            data: DataState::new(),
            filter: FilterState::new(),
            search: SearchState::new(),
            sql: SqlState::new(),
            stats: StatsState::new(),
            status_message: Some("Loading...".to_string()),
            fetch_pending: false,
            action_tx,
            data_rx,
            term_rx,
        })
    }

    /// Offset into the buffer batch that corresponds to view_start.
    pub fn view_offset_in_buffer(&self) -> usize {
        self.viewport
            .view_start
            .saturating_sub(self.data.buffer_offset)
    }

    /// How many rows from the buffer are available for the current view.
    pub fn visible_row_count(&self) -> usize {
        if let Some(ref batch) = self.data.current_batch {
            let view_off = self.view_offset_in_buffer();
            let available = batch.num_rows().saturating_sub(view_off);
            available.min(self.viewport.page_size)
        } else {
            0
        }
    }

    pub fn absolute_row(&self) -> usize {
        self.viewport.selected_row
    }

    /// Compute how many columns are visible at the current terminal width and column offset.
    /// Uses the same column-width logic as the renderer for exact parity.
    pub fn visible_col_count(&self) -> usize {
        use crate::ui::table::subscript_digit;
        use crate::ui::table_render::{build_formatters, compute_column_widths, visible_columns};

        let schema = match self.data.schema.as_ref() {
            Some(s) => s,
            None => return 1,
        };
        let batch = match self.data.current_batch.as_ref() {
            Some(b) => b,
            None => return 1,
        };

        if schema.fields().is_empty() {
            return 1;
        }

        let formatters = build_formatters(batch);
        let multi = self.data.sort_state.specs().len() > 1;
        let sort_state = &self.data.sort_state;

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
            self.viewport.terminal_width as usize,
            self.viewport.column_offset,
            row_prefix_width,
        );

        cols.len().max(1)
    }

    /// Convenience: adjust column view using computed visible col count.
    pub(crate) fn adjust_column_view(&mut self) {
        let visible = self.visible_col_count();
        self.viewport.adjust_column_view_with(visible);
    }

    fn buffer_end(&self) -> usize {
        self.data.buffer_offset
            + self
                .data
                .current_batch
                .as_ref()
                .map_or(0, |b| b.num_rows())
    }

    fn fetch_size(&self) -> usize {
        self.viewport.page_size * BUFFER_MULTIPLIER
    }

    /// Send an action to the worker thread.
    pub(crate) fn send_action(&self, action: Action) {
        let _ = self.action_tx.send(action);
    }

    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let size = terminal.size()?;
        // header row (1) + status bar (1) = 2 lines of chrome
        self.viewport.page_size = (size.height as usize).saturating_sub(2).max(1);
        self.viewport.terminal_width = size.width;

        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                frame.render_widget(AppView::new(self), area);

                if let Some(pos) = self.mode.cursor_position(self, area.height) {
                    frame.set_cursor_position(pos);
                }
            })?;

            if self.mode == AppMode::Quitting {
                let _ = self.action_tx.send(Action::Quit);
                break;
            }

            self.process_data_events();

            match self.term_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(TermEvent::Key(key_event)) => {
                    crate::handlers::handle_key(self, key_event);
                }
                Ok(TermEvent::Resize(w, h)) => {
                    self.viewport.page_size = (h as usize).saturating_sub(2).max(1);
                    self.viewport.terminal_width = w;
                    self.request_buffer_around(self.viewport.selected_row);
                }
                Ok(TermEvent::Tick) | Err(_) => {}
            }
        }

        Ok(())
    }

    fn process_data_events(&mut self) {
        while let Ok(event) = self.data_rx.try_recv() {
            match event {
                DataEvent::FileLoaded {
                    schema,
                    total_rows,
                    file_name,
                    table_name,
                } => {
                    self.data.schema = Some(schema);
                    self.data.total_rows = total_rows;
                    self.data.file_name = Some(file_name);
                    self.data.table_name = Some(table_name);
                    self.status_message = None;
                    self.fetch_pending = false;
                    self.request_buffer_around(0);
                }
                DataEvent::PageLoaded {
                    offset,
                    batch,
                    total_rows,
                } => {
                    self.data.buffer_offset = offset;
                    self.data.current_batch = Some(batch);
                    self.data.total_rows = total_rows;
                    self.fetch_pending = false;
                }
                DataEvent::SortApplied => {
                    self.status_message = None;
                    self.fetch_pending = false;
                }
                DataEvent::FilterApplied { total_rows } => {
                    self.data.total_rows = total_rows;
                    self.viewport.selected_row = 0;
                    self.viewport.view_start = 0;
                    self.data.buffer_offset = 0;
                    self.filter.active_filter = Some(self.filter.input.clone());
                    self.status_message =
                        Some(format!("Filter applied: {total_rows} rows match"));
                    self.fetch_pending = false;
                }
                DataEvent::FilterReset { total_rows } => {
                    self.data.total_rows = total_rows;
                    self.viewport.selected_row = 0;
                    self.viewport.view_start = 0;
                    self.data.buffer_offset = 0;
                    self.filter.active_filter = None;
                    self.status_message = Some("Filter cleared".to_string());
                    self.fetch_pending = false;
                }
                DataEvent::SqlResult {
                    batch,
                    schema,
                    ref sql,
                } => {
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
                DataEvent::SqlError { ref error, ref sql } => {
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
                DataEvent::Error(msg) => {
                    self.status_message = Some(format!("Error: {msg}"));
                    self.fetch_pending = false;
                }
            }
        }
    }

    pub(crate) fn move_down(&mut self, n: usize) {
        self.viewport.move_cursor_down(n, self.data.total_rows);
        self.ensure_buffer();
    }

    pub(crate) fn move_up(&mut self, n: usize) {
        self.viewport.move_cursor_up(n);
        self.ensure_buffer();
    }

    /// Check if we need to fetch more data.
    pub(crate) fn ensure_buffer(&mut self) {
        let buf_end = self.buffer_end();

        let outside_buffer = self.viewport.selected_row < self.data.buffer_offset
            || self.viewport.selected_row >= buf_end;

        if self.fetch_pending && !outside_buffer {
            return;
        }

        let threshold = self.viewport.page_size;

        let needs_fetch = outside_buffer
            || (self.data.buffer_offset > 0
                && self.viewport.selected_row < self.data.buffer_offset + threshold)
            || (buf_end < self.data.total_rows
                && self.viewport.selected_row + threshold >= buf_end);

        if needs_fetch {
            self.request_buffer_around(self.viewport.selected_row);
        }
    }

    pub(crate) fn request_buffer_around(&mut self, center_row: usize) {
        let fetch_size = self.fetch_size();
        let new_offset = center_row.saturating_sub(fetch_size / 2);
        self.fetch_pending = true;
        let _ = self.action_tx.send(Action::FetchPage {
            offset: new_offset,
            limit: fetch_size,
        });
    }

    pub(crate) fn cycle_sort(&mut self) {
        let schema = match self.data.schema.as_ref() {
            Some(s) => s,
            None => return,
        };

        let fields = schema.fields();
        if fields.is_empty() {
            return;
        }

        let col_idx = self.viewport.selected_col.min(fields.len() - 1);
        let col_name = fields[col_idx].name().clone();

        self.data.sort_state.toggle(&col_name);
        let _ = self
            .action_tx
            .send(Action::ApplySort(self.data.sort_state.clone()));
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
