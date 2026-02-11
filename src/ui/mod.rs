pub mod input_bar;
pub mod sql_pad;
pub mod stats_overlay;
pub mod status_bar;
pub mod table;
pub mod table_render;

use ratatui::buffer::Buffer;
use ratatui::layout::{Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::app::{App, AppMode};

use self::input_bar::InputBar;
use self::sql_pad::SqlPad;
use self::stats_overlay::StatsOverlay;
use self::status_bar::StatusBar;
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
        let show_input_bar = matches!(
            self.app.mode,
            AppMode::Filter | AppMode::Search | AppMode::Regex
        );
        let show_sql = matches!(self.app.mode, AppMode::Sql);
        let show_stats = matches!(self.app.mode, AppMode::Stats);

        let chunks = Layout::vertical(self.app.mode.layout_constraints()).split(area);

        if show_stats {
            TableView::new(self.app).render(chunks[0], buf);
            StatsOverlay::new(self.app).render(chunks[1], buf);
            StatusBar::new(self.app).render(chunks[2], buf);
        } else if show_sql {
            if self.app.sql.result.is_some() {
                SqlResultTableView::new(self.app).render(chunks[0], buf);
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
                _ => ("Search: ", Color::Yellow),
            };
            InputBar::new(prompt, color, &self.app.filter.input)
                .render(chunks[1], buf);
            StatusBar::new(self.app).render(chunks[2], buf);
            if self.app.mode == AppMode::Filter {
                render_autocomplete_popup(self.app, chunks[1].y, area, buf);
            }
        } else {
            TableView::new(self.app).render(chunks[0], buf);
            StatusBar::new(self.app).render(chunks[1], buf);
        }
    }
}

/// Render the autocomplete popup above the filter bar.
fn render_autocomplete_popup(app: &App, filter_bar_y: u16, area: Rect, buf: &mut Buffer) {
    if !app.filter.autocomplete_active || app.filter.autocomplete_suggestions.is_empty() {
        return;
    }

    let max_visible = 5usize;
    let count = app.filter.autocomplete_suggestions.len();
    let visible_count = count.min(max_visible);

    let selected = app.filter.autocomplete_index.min(count.saturating_sub(1));
    let scroll_start = if selected >= visible_count {
        selected - visible_count + 1
    } else {
        0
    };

    let max_name_len = app
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
        let input = &app.filter.input;
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

/// Renders SQL query results in the table area.
struct SqlResultTableView<'a> {
    app: &'a App,
}

impl<'a> SqlResultTableView<'a> {
    fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for SqlResultTableView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use duckdb::arrow::array::Array;
        use ratatui::style::Modifier;
        use unicode_width::UnicodeWidthStr;

        use crate::ui::table::type_color;
        use crate::ui::table_render::{
            build_formatters, compute_column_widths, truncate_to_width, visible_columns,
        };

        let schema = match self.app.sql.result_schema.as_ref() {
            Some(s) => s,
            None => return,
        };
        let batch = match self.app.sql.result.as_ref() {
            Some(b) => b,
            None => return,
        };

        if area.height < 2 || area.width < 4 {
            return;
        }

        let batch_rows = batch.num_rows();
        let fields = schema.fields();
        let visible_rows = batch_rows.min((area.height as usize).saturating_sub(1));

        let formatters = build_formatters(batch);

        let (headers, col_widths) = compute_column_widths(
            schema,
            batch,
            &formatters,
            &|_i, name, type_str| format!("{} [{}]", name, type_str),
            (0, visible_rows),
            50,
        );

        let row_prefix_width = 3;
        let visible_cols = visible_columns(&col_widths, area.width as usize, 0, row_prefix_width);

        if visible_cols.is_empty() {
            return;
        }

        // Render header row
        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let mut x = area.x + row_prefix_width as u16;
        let header_y = area.y;

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            Style::default(),
        );

        for &col_idx in &visible_cols {
            let field = &fields[col_idx];
            let header = &headers[col_idx];
            let width = col_widths[col_idx] as usize;

            if header.width() > width {
                let display: String = header.chars().take(width).collect();
                buf.set_string(x, header_y, &display, header_style);
            } else {
                let name_part = field.name();
                buf.set_string(x, header_y, name_part, header_style);
                let mut cx = x + name_part.width() as u16;

                let type_str = format!("{}", field.data_type());
                let tc = type_color(field.data_type());
                let type_style = Style::default().fg(tc).add_modifier(Modifier::BOLD);

                buf.set_string(cx, header_y, " [", header_style);
                cx += 2;
                buf.set_string(cx, header_y, &type_str, type_style);
                cx += type_str.width() as u16;
                buf.set_string(cx, header_y, "]", header_style);
                cx += 1;

                let used = (cx - x) as usize;
                if used < width {
                    buf.set_string(
                        cx,
                        header_y,
                        " ".repeat(width - used),
                        Style::default(),
                    );
                }
            }
            x += col_widths[col_idx] + 2;
        }

        // Render data rows
        let data_start_y = header_y + 1;
        let max_display_rows = (area.height - 1) as usize;

        for display_row in 0..max_display_rows {
            let row_y = data_start_y + display_row as u16;
            if row_y >= area.y + area.height {
                break;
            }

            if display_row >= batch_rows {
                buf.set_string(
                    area.x,
                    row_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                continue;
            }

            let row_style = Style::default();

            buf.set_string(
                area.x,
                row_y,
                " ".repeat(area.width as usize),
                row_style,
            );
            buf.set_string(area.x, row_y, "   ", row_style);

            let mut x = area.x + row_prefix_width as u16;
            for &col_idx in &visible_cols {
                let width = col_widths[col_idx] as usize;
                let column = batch.column(col_idx);
                let is_null = column.is_null(display_row);

                let (display_val, cell_style) = if is_null {
                    ("NULL".to_string(), row_style.fg(Color::DarkGray))
                } else if let Some(ref fmt) = formatters[col_idx] {
                    let val = fmt.value(display_row).to_string();
                    (val, row_style)
                } else {
                    ("?".to_string(), row_style)
                };

                buf.set_string(
                    x,
                    row_y,
                    &truncate_to_width(&display_val, width),
                    cell_style,
                );
                x += col_widths[col_idx] + 2;
            }
        }
    }
}
