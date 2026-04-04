pub mod column_jump;
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
pub mod theme;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::App;
use crate::mode::{AppMode, InputVariant, OverlayVariant};

use self::column_jump::ColumnJump;
use self::column_picker::ColumnPicker;
use self::help_overlay::HelpOverlay;
use self::input_bar::InputBar;
use self::sql_pad::SqlPad;
use self::stats_overlay::StatsOverlay;
use self::status_bar::StatusBar;
use self::tab_bar::TabBar;
use self::table::TableView;

const AUTOCOMPLETE_MAX_VISIBLE: usize = 5;
pub const INPUT_PROMPT_WIDTH: u16 = 8;

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
            self.app.tab_bar_y.set(Some(split[0].y));
            (Some(split[0]), split[1])
        } else {
            self.app.tab_bar_y.set(None);
            (None, area)
        };

        if let Some(tab_area) = tab_area {
            TabBar::new(self.app).render(tab_area, buf);
        }

        let chunks = Layout::vertical(self.app.mode.layout_constraints()).split(main_area);

        match &self.app.mode {
            AppMode::Overlay(OverlayVariant::Help) => {
                TableView::new(self.app).render(chunks[0], buf);
                StatusBar::new(self.app).render(chunks[1], buf);
                HelpOverlay::new(self.app).render(area, buf);
            }
            AppMode::Overlay(OverlayVariant::ColumnPicker) => {
                TableView::new(self.app).render(chunks[0], buf);
                StatusBar::new(self.app).render(chunks[1], buf);
                ColumnPicker::new(self.app).render(area, buf);
            }
            AppMode::Overlay(OverlayVariant::Stats) => {
                TableView::new(self.app).render(chunks[0], buf);
                StatusBar::new(self.app).render(chunks[1], buf);
                StatsOverlay::new(self.app).render(area, buf);
            }
            AppMode::Overlay(OverlayVariant::ColumnJump) => {
                TableView::new(self.app).render(chunks[0], buf);
                StatusBar::new(self.app).render(chunks[1], buf);
                ColumnJump::new(self.app).render(area, buf);
            }
            AppMode::Sql => {
                if let (Some(schema), Some(batch)) = (
                    self.app.sql.result_schema.as_ref(),
                    self.app.sql.result.as_ref(),
                ) {
                    use crate::ui::table_render::{DefaultStyler, UnifiedTable};
                    let styler = DefaultStyler { theme: &self.app.theme };
                    UnifiedTable::new(schema, batch, &styler)
                        .theme(&self.app.theme)
                        .render(chunks[0], buf);
                } else {
                    TableView::new(self.app).render(chunks[0], buf);
                }
                SqlPad::new(self.app).render(chunks[1], buf);
                StatusBar::new(self.app).render(chunks[2], buf);
            }
            AppMode::Input(variant) => {
                TableView::new(self.app).render(chunks[0], buf);
                let prompt = match variant {
                    InputVariant::Filter => "Filter: ",
                    InputVariant::GoToRow => "Goto:   ",
                    InputVariant::Search => "Search: ",
                };
                InputBar::new(self.app, prompt, &self.app.text_input.input)
                    .render(chunks[1], buf);
                StatusBar::new(self.app).render(chunks[2], buf);
                if *variant == InputVariant::Filter {
                    render_autocomplete_popup(self.app, chunks[1].y, main_area, buf);
                }
            }
            _ => {
                TableView::new(self.app).render(chunks[0], buf);
                StatusBar::new(self.app).render(chunks[1], buf);
            }
        }
    }
}

/// Render the autocomplete popup above the filter bar.
fn render_autocomplete_popup(app: &App, filter_bar_y: u16, area: Rect, buf: &mut Buffer) {
    if !app.autocomplete.active || app.autocomplete.suggestions.is_empty() {
        return;
    }

    let max_visible = AUTOCOMPLETE_MAX_VISIBLE;
    let count = app.autocomplete.suggestions.len();
    let visible_count = count.min(max_visible);

    let selected = app
        .autocomplete
        .selected_index
        .min(count.saturating_sub(1));
    let scroll_start = if selected >= visible_count {
        selected - visible_count + 1
    } else {
        0
    };

    let max_name_len = app
        .autocomplete
        .suggestions
        .iter()
        .skip(scroll_start)
        .take(visible_count)
        .map(|s| s.len())
        .max()
        .unwrap_or(0);
    let popup_width = (max_name_len + 4).min(area.width as usize);

    let dollar_x = {
        let prompt_len = INPUT_PROMPT_WIDTH;
        let input = &app.text_input.input;
        if let Some(pos) = input.rfind('$') {
            area.x + prompt_len + pos as u16
        } else {
            area.x + prompt_len
        }
    };

    let popup_x = dollar_x.min(area.x + area.width - popup_width as u16);

    let popup_height = visible_count as u16;
    let popup_y = filter_bar_y.saturating_sub(popup_height);

    let theme = &app.theme;
    let bg_style = Style::default().fg(theme.fg).bg(theme.bg);
    let selected_style = Style::default().fg(theme.fg).bg(theme.selected_bg);

    for (i, suggestion) in app
        .autocomplete
        .suggestions
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
