pub mod column_picker;
pub mod diff_view;
pub mod help_overlay;
pub mod input_bar;
pub mod popup;
pub mod sql_pad;
pub mod stats_overlay;
pub mod status_bar;
pub mod tab_bar;
pub mod table;
pub mod table_render;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::app::{App, AppMode};

use self::column_picker::ColumnPicker;
use self::diff_view::DiffView;
use self::help_overlay::HelpOverlay;
use self::input_bar::InputBar;
use self::sql_pad::SqlPad;
use self::stats_overlay::StatsOverlay;
use self::status_bar::StatusBar;
use self::tab_bar::TabBar;
use self::table::TableView;

pub struct AppView<'a> {
    app: &'a App,
}

impl<'a> AppView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for AppView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // If multiple tabs, split off a tab bar row at the top
        let (tab_area, main_area) = if self.app.has_tabs() {
            let split = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
            (Some(split[0]), split[1])
        } else {
            (None, area)
        };

        if let Some(tab_area) = tab_area {
            TabBar::new(self.app).render(tab_area, buf);
        }

        let show_input_bar = matches!(
            self.app.mode,
            AppMode::Filter | AppMode::Search | AppMode::Regex | AppMode::GoToRow
        );
        let show_sql = matches!(self.app.mode, AppMode::Sql);
        let show_stats = matches!(self.app.mode, AppMode::Stats);
        let show_diff_setup = matches!(
            self.app.mode,
            AppMode::DiffSetupKey | AppMode::DiffSetupCols
        );
        let show_column_hide = matches!(self.app.mode, AppMode::ColumnHide);
        let show_diff = matches!(self.app.mode, AppMode::Diff);
        let show_help = matches!(self.app.mode, AppMode::Help);

        let chunks = Layout::vertical(self.app.mode.layout_constraints()).split(main_area);

        if show_help {
            TableView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
            HelpOverlay::new(self.app).render(area, buf);
        } else if show_column_hide {
            TableView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
            ColumnPicker::new(self.app).render(area, buf);
        } else if show_diff_setup {
            TableView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
            ColumnPicker::new(self.app).render(area, buf);
        } else if show_diff {
            DiffView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
        } else if show_stats {
            TableView::new(self.app).render(chunks[0], buf);
            StatsOverlay::new(self.app).render(chunks[1], buf);
            StatusBar::new(self.app).render(chunks[2], buf);
        } else if show_sql {
            if let (Some(schema), Some(batch)) = (
                self.app.sql.result_schema.as_ref(),
                self.app.sql.result.as_ref(),
            ) {
                use crate::ui::table_render::{DefaultStyler, UnifiedTable};
                let styler = DefaultStyler;
                UnifiedTable::new(schema, batch, &styler).render(chunks[0], buf);
            } else {
                TableView::new(self.app).render(chunks[0], buf);
            }
            SqlPad::new(self.app).render(chunks[1], buf);
            StatusBar::new(self.app).render(chunks[2], buf);
        } else if show_input_bar {
            TableView::new(self.app).render(chunks[0], buf);
            let (prompt, color) = match self.app.mode {
                AppMode::Regex => ("Regex:  ", Color::Magenta),
                AppMode::Filter => ("Filter: ", Color::Green),
                AppMode::GoToRow => ("Goto:   ", Color::Blue),
                _ => ("Search: ", Color::Yellow),
            };
            InputBar::new(prompt, color, &self.app.tab().filter.input)
                .render(chunks[1], buf);
            StatusBar::new(self.app).render(chunks[2], buf);
            if self.app.mode == AppMode::Filter {
                render_autocomplete_popup(self.app, chunks[1].y, main_area, buf);
            }
        } else {
            TableView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
        }
    }
}

/// Render the autocomplete popup above the filter bar.
fn render_autocomplete_popup(app: &App, filter_bar_y: u16, area: Rect, buf: &mut Buffer) {
    if !app.tab().filter.autocomplete_active || app.tab().filter.autocomplete_suggestions.is_empty()
    {
        return;
    }

    let max_visible = 5usize;
    let count = app.tab().filter.autocomplete_suggestions.len();
    let visible_count = count.min(max_visible);

    let selected = app
        .tab()
        .filter
        .autocomplete_index
        .min(count.saturating_sub(1));
    let scroll_start = if selected >= visible_count {
        selected - visible_count + 1
    } else {
        0
    };

    let max_name_len = app
        .tab()
        .filter
        .autocomplete_suggestions
        .iter()
        .skip(scroll_start)
        .take(visible_count)
        .map(|s| s.len())
        .max()
        .unwrap_or(0);
    let popup_width = (max_name_len + 4).min(area.width as usize);

    let dollar_x = {
        let prompt_len = 8u16;
        let input = &app.tab().filter.input;
        if let Some(pos) = input.rfind('$') {
            area.x + prompt_len + pos as u16
        } else {
            area.x + prompt_len
        }
    };

    let popup_x = dollar_x.min(area.x + area.width - popup_width as u16);

    let popup_height = visible_count as u16;
    let popup_y = filter_bar_y.saturating_sub(popup_height);

    let bg_style = Style::default().fg(Color::White).bg(Color::Black);
    let selected_style = Style::default().fg(Color::White).bg(Color::DarkGray);

    for (i, suggestion) in app
        .tab()
        .filter
        .autocomplete_suggestions
        .iter()
        .skip(scroll_start)
        .take(visible_count)
        .enumerate()
    {
        let y = popup_y + i as u16;
        if y >= area.y + area.height {
            break;
        }

        let is_selected = scroll_start + i == selected;
        let style = if is_selected {
            selected_style
        } else {
            bg_style
        };

        let padded = format!(" {:<width$} ", suggestion, width = popup_width.saturating_sub(4));
        let display: String = padded.chars().take(popup_width).collect();
        buf.set_string(popup_x, y, &display, style);
    }
}

