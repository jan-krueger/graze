use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use ratatui::DefaultTerminal;
use unicode_width::UnicodeWidthStr;

use crate::diff::DiffBackend;
use crate::event::TermEvent;
use crate::state::{ColumnPickerState, DiffState};
use crate::ui::diff_view::DiffView;
use crate::ui::popup::Popup;
use crate::ui::theme::Theme;

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    PickKey,
    PickCols,
    Loading,
    Viewing,
    Quitting,
}

pub struct DiffApp {
    diff: DiffState,
    theme: Theme,
    column_picker: ColumnPickerState,
    status_message: Option<String>,
    tick: usize,
    page_size: usize,
    terminal_width: u16,
    terminal_height: u16,
    term_rx: Receiver<TermEvent>,
    diff_rx: Option<Receiver<Result<DiffBackend>>>,
    mode: Mode,
    path_a: PathBuf,
    path_b: PathBuf,
    columns: Vec<String>,
}

fn read_schema_columns(path: &Path) -> Result<Vec<String>> {
    let conn = duckdb::Connection::open_in_memory()?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let path_str = path.to_string_lossy();
    let read_fn = match ext.as_str() {
        "csv" | "tsv" => "read_csv_auto",
        "parquet" => "read_parquet",
        "json" | "jsonl" | "ndjson" => "read_json_auto",
        _ => anyhow::bail!("Unsupported file extension: .{ext}"),
    };
    let sql = format!("SELECT * FROM {read_fn}('{path_str}') LIMIT 0");
    let mut stmt = conn.prepare(&sql)?;
    let arrow = stmt.query_arrow([])?;
    let schema = arrow.get_schema();
    Ok(schema.fields().iter().map(|f| f.name().clone()).collect())
}

impl DiffApp {
    pub fn new(
        path_a: PathBuf,
        path_b: PathBuf,
        key_cols: Option<Vec<String>>,
        diff_cols: Option<Vec<String>>,
        theme: Theme,
    ) -> Result<Self> {
        let (term_tx, term_rx) = mpsc::channel::<TermEvent>();
        thread::spawn(move || event_thread(term_tx));

        let columns = read_schema_columns(&path_a)
            .context("Failed to read schema from file A")?;

        let file_a = path_a
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_b = path_b
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut diff = DiffState::new();
        diff.file_a = file_a;
        diff.file_b = file_b;

        let mode = if let Some(keys) = key_cols {
            diff.key_columns = keys;
            diff.diff_columns = diff_cols.unwrap_or_else(|| {
                columns
                    .iter()
                    .filter(|c| !diff.key_columns.contains(c))
                    .cloned()
                    .collect()
            });
            Mode::Loading
        } else {
            Mode::PickKey
        };

        let mut app = Self {
            diff,
            theme,
            column_picker: ColumnPickerState::new(),
            status_message: None,
            tick: 0,
            page_size: 50,
            terminal_width: 80,
            terminal_height: 24,
            term_rx,
            diff_rx: None,
            mode,
            path_a,
            path_b,
            columns,
        };

        if app.mode == Mode::PickKey {
            let len = app.columns.len();
            app.column_picker
                .populate(app.columns.clone(), vec![false; len]);
        } else if app.mode == Mode::Loading {
            app.start_diff();
        }

        Ok(app)
    }

    fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.tick % SPINNER_FRAMES.len()]
    }

    fn start_diff(&mut self) {
        self.diff.loading = true;
        self.diff.error = None;
        self.status_message = Some("Computing diff...".to_string());
        self.mode = Mode::Loading;

        let path_a = self.path_a.clone();
        let path_b = self.path_b.clone();
        let key_columns = self.diff.key_columns.clone();
        let diff_columns = self.diff.diff_columns.clone();

        let (tx, rx) = mpsc::channel::<Result<DiffBackend>>();
        self.diff_rx = Some(rx);

        thread::spawn(move || {
            let result = DiffBackend::compute(&path_a, &path_b, &key_columns, &diff_columns);
            let _ = tx.send(result);
        });
    }

    fn poll_diff_result(&mut self) {
        let result = match self.diff_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.diff_rx = None;
                    self.diff.loading = false;
                    if self.diff.error.is_none() && self.diff.backend.is_none() {
                        self.diff.error = Some("Diff thread disconnected".to_string());
                        self.status_message = Some("Diff failed".to_string());
                    }
                    return;
                }
            },
            None => return,
        };

        self.diff_rx = None;
        self.diff.loading = false;

        match result {
            Ok(backend) => {
                self.diff.counts = backend.counts.clone();
                self.diff.markers = backend.markers.clone();
                self.diff.changed_row_indices = backend.changed_row_indices.clone();
                self.diff.schema = Some(backend.schema.clone());
                self.diff.b_side_col_map = backend.b_side_col_map.clone();
                self.diff.total_rows = backend.total_rows;
                self.diff.selected_col = 0;
                self.diff.selected_row = 0;
                self.diff.scroll_offset = 0;
                self.diff.rebuild_visible_rows();

                let page_size = self.diff_page_size();
                let row_indices: Vec<usize> =
                    (0..page_size.min(backend.total_rows)).collect();
                match backend.fetch_rows(&row_indices) {
                    Ok(page) => {
                        self.diff.page = Some(page);
                    }
                    Err(e) => {
                        self.diff.error =
                            Some(format!("Failed to fetch first page: {e:#}"));
                    }
                }

                self.diff.backend = Some(backend);
                self.mode = Mode::Viewing;

                let c = &self.diff.counts;
                self.status_message = Some(format!(
                    "+{} -{} ~{} ={}",
                    c.only_a, c.only_b, c.changed, c.common
                ));
            }
            Err(e) => {
                self.diff.error = Some(format!("{e:#}"));
                self.status_message = Some(format!("Diff error: {e:#}"));
            }
        }
    }

    fn diff_page_size(&self) -> usize {
        let chrome = 3; // header bar + column header + status bar
        (self.terminal_height as usize).saturating_sub(chrome).max(1)
    }

    fn ensure_diff_page(&mut self) {
        let backend = match self.diff.backend.as_ref() {
            Some(b) => b,
            None => return,
        };

        let screen_page_size = self.diff_page_size();
        let buffer_size = screen_page_size * 3;

        let visible_count = self.diff.visible_row_count();
        if visible_count == 0 {
            return;
        }

        let center = self.diff.selected_row;
        let half = buffer_size / 2;
        let start = center.saturating_sub(half);
        let end = (start + buffer_size).min(visible_count);
        let start = end.saturating_sub(buffer_size);

        let needed_data_rows: Vec<usize> = if self.diff.hide_common {
            self.diff.visible_rows[start..end].to_vec()
        } else {
            (start..end).collect()
        };

        if let Some(page) = self.diff.page.as_ref() {
            if !page.row_indices.is_empty() {
                let page_first = page.row_indices[0];
                let page_last = *page.row_indices.last().unwrap();
                let need_first = needed_data_rows[0];
                let need_last = *needed_data_rows.last().unwrap();
                if need_first >= page_first && need_last <= page_last {
                    return;
                }
            }
        }

        match backend.fetch_rows(&needed_data_rows) {
            Ok(page) => {
                self.diff.page = Some(page);
            }
            Err(e) => {
                self.status_message = Some(format!("Page fetch error: {e:#}"));
            }
        }
    }

    fn update_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
        self.page_size = (height as usize).saturating_sub(3).max(1);
    }

    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let size = terminal.size()?;
        self.update_terminal_size(size.width, size.height);

        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                frame.render_widget(DiffAppView { app: self }, area);
            })?;

            if self.mode == Mode::Quitting {
                break;
            }

            self.poll_diff_result();

            match self.term_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(TermEvent::Key(key_event)) => {
                    self.handle_key(key_event);
                }
                Ok(TermEvent::Mouse(mouse)) => {
                    self.handle_mouse(mouse);
                }
                Ok(TermEvent::Resize(w, h)) => {
                    self.update_terminal_size(w, h);
                }
                Ok(TermEvent::Tick) | Err(_) => {
                    self.tick += 1;
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match &self.mode {
            Mode::PickKey => self.handle_pick_key(key),
            Mode::PickCols => self.handle_pick_cols(key),
            Mode::Loading => {
                if key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    self.mode = Mode::Quitting;
                }
            }
            Mode::Viewing => self.handle_viewing(key),
            Mode::Quitting => {}
        }
    }

    fn handle_pick_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.column_picker.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.column_picker.move_up(),
            KeyCode::Char(' ') => self.column_picker.toggle_at_cursor(),
            KeyCode::Enter => {
                let key_columns = self.column_picker.selected_names();
                if key_columns.is_empty() {
                    self.status_message =
                        Some("Select at least one key column".to_string());
                    return;
                }
                self.diff.key_columns = key_columns.clone();

                // Reset picker for diff column selection
                let columns = self.columns.clone();
                let selected: Vec<bool> = columns
                    .iter()
                    .map(|c| !key_columns.contains(c))
                    .collect();
                self.column_picker.populate(columns, selected);
                self.mode = Mode::PickCols;
                self.status_message = None;
            }
            KeyCode::Esc => {
                self.mode = Mode::Quitting;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Quitting;
            }
            _ => {}
        }
    }

    fn handle_pick_cols(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.column_picker.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.column_picker.move_up(),
            KeyCode::Char(' ') => self.column_picker.toggle_at_cursor(),
            KeyCode::Enter => {
                let diff_columns = self.column_picker.selected_names();
                if diff_columns.is_empty() {
                    self.status_message =
                        Some("Select at least one column to compare".to_string());
                    return;
                }
                self.diff.diff_columns = diff_columns;
                self.start_diff();
            }
            KeyCode::Esc => {
                self.mode = Mode::Quitting;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Quitting;
            }
            _ => {}
        }
    }

    fn handle_viewing(&mut self, key: crossterm::event::KeyEvent) {
        let visible_rows = self.diff.visible_row_count();
        let page_size = self.diff_page_size();
        let half_page = (page_size / 2).max(1);

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.mode = Mode::Quitting;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Quitting;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if visible_rows > 0 {
                    self.diff.selected_row =
                        (self.diff.selected_row + 1).min(visible_rows - 1);
                    self.diff.adjust_view(page_size);
                    self.ensure_diff_page();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff.selected_row = self.diff.selected_row.saturating_sub(1);
                self.diff.adjust_view(page_size);
                self.ensure_diff_page();
            }
            KeyCode::Char('g') => {
                self.diff.selected_row = 0;
                self.diff.adjust_view(page_size);
                self.ensure_diff_page();
            }
            KeyCode::Char('G') => {
                if visible_rows > 0 {
                    self.diff.selected_row = visible_rows - 1;
                    self.diff.adjust_view(page_size);
                    self.ensure_diff_page();
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if visible_rows > 0 {
                    self.diff.selected_row =
                        (self.diff.selected_row + half_page).min(visible_rows - 1);
                    self.diff.adjust_view(page_size);
                    self.ensure_diff_page();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.diff.selected_row =
                    self.diff.selected_row.saturating_sub(half_page);
                self.diff.adjust_view(page_size);
                self.ensure_diff_page();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.diff.selected_col > 0 {
                    self.diff.selected_col -= 1;
                    if self.diff.selected_col < self.diff.column_offset {
                        self.diff.column_offset = self.diff.selected_col;
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(ref schema) = self.diff.schema {
                    let max_col = schema.fields().len().saturating_sub(1);
                    if self.diff.selected_col < max_col {
                        self.diff.selected_col += 1;
                        if self.diff.column_offset < self.diff.selected_col {
                            self.diff.column_offset += 1;
                        }
                    }
                }
            }
            KeyCode::Char('n') => {
                self.nav_next_changed(page_size);
            }
            KeyCode::Char('N') => {
                self.nav_prev_changed(page_size);
            }
            KeyCode::Char('c') => {
                self.diff.hide_common = !self.diff.hide_common;
                self.diff.rebuild_visible_rows();
                let visible = self.diff.visible_row_count();
                if visible > 0 {
                    self.diff.selected_row =
                        self.diff.selected_row.min(visible - 1);
                } else {
                    self.diff.selected_row = 0;
                }
                self.diff.adjust_view(page_size);
                self.ensure_diff_page();
            }
            _ => {}
        }
    }

    fn nav_next_changed(&mut self, page_size: usize) {
        if self.diff.changed_row_indices.is_empty() {
            return;
        }
        let current_data_row =
            self.diff.display_to_data_row(self.diff.selected_row);
        let idx = match self
            .diff
            .changed_row_indices
            .binary_search(&(current_data_row + 1))
        {
            Ok(i) => i,
            Err(i) => {
                if i < self.diff.changed_row_indices.len() {
                    i
                } else {
                    0
                }
            }
        };
        let data_row = self.diff.changed_row_indices[idx];
        self.jump_to_diff_cell(data_row, self.diff.selected_col, page_size);
    }

    fn nav_prev_changed(&mut self, page_size: usize) {
        if self.diff.changed_row_indices.is_empty() {
            return;
        }
        let current_data_row =
            self.diff.display_to_data_row(self.diff.selected_row);
        let idx = match self
            .diff
            .changed_row_indices
            .binary_search(&current_data_row)
        {
            Ok(i) => {
                if i > 0 {
                    i - 1
                } else {
                    self.diff.changed_row_indices.len() - 1
                }
            }
            Err(i) => {
                if i > 0 {
                    i - 1
                } else {
                    self.diff.changed_row_indices.len() - 1
                }
            }
        };
        let data_row = self.diff.changed_row_indices[idx];
        self.jump_to_diff_cell(data_row, self.diff.selected_col, page_size);
    }

    fn handle_mouse(&mut self, event: crossterm::event::MouseEvent) {
        if self.mode != Mode::Viewing {
            return;
        }
        let page_size = self.diff_page_size();
        let visible_rows = self.diff.visible_row_count();
        match event.kind {
            crossterm::event::MouseEventKind::ScrollDown => {
                if visible_rows > 0 {
                    self.diff.selected_row =
                        (self.diff.selected_row + 3).min(visible_rows.saturating_sub(1));
                    self.diff.adjust_view(page_size);
                    self.ensure_diff_page();
                }
            }
            crossterm::event::MouseEventKind::ScrollUp => {
                self.diff.selected_row = self.diff.selected_row.saturating_sub(3);
                self.diff.adjust_view(page_size);
                self.ensure_diff_page();
            }
            _ => {}
        }
    }

    fn jump_to_diff_cell(&mut self, data_row: usize, col: usize, page_size: usize) {
        let display_row = if self.diff.hide_common {
            self.diff
                .visible_rows
                .iter()
                .position(|&r| r == data_row)
                .unwrap_or(0)
        } else {
            data_row
        };
        self.diff.selected_row = display_row;
        self.diff.selected_col = col;
        self.diff.adjust_view(page_size);
        self.diff.column_offset = self.diff.selected_col;
        self.ensure_diff_page();
    }
}

struct DiffAppView<'a> {
    app: &'a DiffApp,
}

impl Widget for DiffAppView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match &self.app.mode {
            Mode::PickKey | Mode::PickCols => {
                // Clear background
                let bg = Style::default();
                for y in area.y..area.y + area.height {
                    buf.set_string(area.x, y, " ".repeat(area.width as usize), bg);
                }

                // Render column picker
                render_picker(self.app, area, buf);

                // Status bar
                let status_area = Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                render_diff_status_bar(self.app, status_area, buf);
            }
            Mode::Loading => {
                let chunks =
                    Layout::vertical([Constraint::Min(3), Constraint::Length(1)])
                        .split(area);
                DiffView::new(&self.app.diff, &self.app.theme, self.app.spinner_char())
                    .render(chunks[0], buf);
                render_diff_status_bar(self.app, chunks[1], buf);
            }
            Mode::Viewing => {
                let chunks =
                    Layout::vertical([Constraint::Min(3), Constraint::Length(1)])
                        .split(area);
                DiffView::new(&self.app.diff, &self.app.theme, self.app.spinner_char())
                    .render(chunks[0], buf);
                render_diff_status_bar(self.app, chunks[1], buf);
            }
            Mode::Quitting => {}
        }
    }
}

fn render_picker(app: &DiffApp, area: Rect, buf: &mut Buffer) {
    let picker = &app.column_picker;
    if picker.columns.is_empty() {
        return;
    }

    let title = match app.mode {
        Mode::PickKey => "Select key column(s)",
        Mode::PickCols => "Select columns to compare",
        _ => return,
    };

    let max_name_len = picker.columns.iter().map(|c| c.width()).max().unwrap_or(8);
    let footer = "Space:toggle  Enter:confirm  Esc:quit";
    let content_width = 6 + max_name_len + 2;
    let max_visible = (area.height as usize).saturating_sub(6);
    let visible_rows = picker.columns.len().min(max_visible).max(1);

    let theme = &app.theme;
    let popup = Popup::new(title, footer, content_width, visible_rows, theme);
    let inner = popup.render_frame(area, buf);

    let normal_style = Style::default().fg(theme.fg);
    let selected_style = Style::default().fg(theme.fg).bg(theme.selected_bg);
    let check_style = Style::default().fg(Color::Green);

    let scroll_start = if picker.cursor >= visible_rows {
        picker.cursor - visible_rows + 1
    } else {
        0
    };

    for (i, col_idx) in (scroll_start..picker.columns.len())
        .take(visible_rows)
        .enumerate()
    {
        let row_y = inner.y + i as u16;
        if row_y >= inner.y + inner.height {
            break;
        }

        let is_cursor = col_idx == picker.cursor;
        let is_checked = picker.selected[col_idx];
        let row_style = if is_cursor { selected_style } else { normal_style };

        let prefix = if is_cursor { ">> " } else { "   " };
        buf.set_string(inner.x, row_y, prefix, row_style);

        let checkbox = if is_checked { "[x]" } else { "[ ]" };
        let cb_style = if is_checked && !is_cursor {
            check_style
        } else {
            row_style
        };
        buf.set_string(inner.x + 3, row_y, checkbox, cb_style);

        let name = &picker.columns[col_idx];
        let padded_name = format!("{:<width$}", name, width = max_name_len);
        buf.set_string(inner.x + 7, row_y, &padded_name, row_style);

        let used = 7 + max_name_len;
        let pad = (inner.width as usize).saturating_sub(used);
        buf.set_string(
            inner.x + used as u16,
            row_y,
            &" ".repeat(pad),
            row_style,
        );
    }
}

fn render_diff_status_bar(app: &DiffApp, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let bg_style = Style::default().bg(theme.status_bg).fg(theme.status_fg);

    buf.set_string(
        area.x,
        area.y,
        " ".repeat(area.width as usize),
        bg_style,
    );

    // Mode badge
    let (badge, badge_style) = match app.mode {
        Mode::PickKey => (
            " KEY ",
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Mode::PickCols => (
            " COLS ",
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        _ => (
            " DIFF ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    };
    buf.set_string(area.x, area.y, badge, badge_style);
    let mut x = area.x + badge.len() as u16;

    // File names
    let files = format!(" {} vs {} ", app.diff.file_a, app.diff.file_b);
    buf.set_string(x, area.y, &files, bg_style);
    x += files.len() as u16;

    // Position (in viewing mode)
    if app.mode == Mode::Viewing {
        let visible = app.diff.visible_row_count();
        if visible > 0 {
            let pos = format!(" {}/{} ", app.diff.selected_row + 1, visible);
            buf.set_string(x, area.y, &pos, bg_style);
            x += pos.len() as u16;
        }

        // Diff counts
        let counts = format!(
            " +{} -{} ~{} ={} ",
            app.diff.counts.only_a,
            app.diff.counts.only_b,
            app.diff.counts.changed,
            app.diff.counts.common
        );
        buf.set_string(x, area.y, &counts, bg_style.fg(Color::Cyan));
        x += counts.len() as u16;
    }

    // Status message
    if let Some(ref msg) = app.status_message {
        let msg_str = format!(" | {msg} ");
        let msg_style = bg_style.fg(Color::Yellow);
        buf.set_string(x, area.y, &msg_str, msg_style);
        x += msg_str.len() as u16;
    }

    // Right-aligned hints
    let hints = match app.mode {
        Mode::PickKey | Mode::PickCols => "Space:toggle Enter:confirm Esc:quit",
        Mode::Loading => "Esc:cancel",
        Mode::Viewing => "n:next N:prev h/l:col c:changes q:quit",
        Mode::Quitting => "",
    };
    let hints_width = hints.len() as u16;
    if area.width > hints_width + x - area.x + 1 {
        let hints_x = area.x + area.width - hints_width - 1;
        buf.set_string(
            hints_x,
            area.y,
            hints,
            bg_style.add_modifier(Modifier::DIM),
        );
    }
    let _ = x;
}

fn event_thread(tx: std::sync::mpsc::Sender<TermEvent>) {
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
