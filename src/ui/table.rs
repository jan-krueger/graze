use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::search::SearchMatcher;
use crate::state::{SearchMode, SelectionMode};
use crate::ui::table_render::{UnifiedTable, TableStyler};

#[derive(Debug)]
struct SimpleFilter {
    column: String,
    op: String,
    value: String,
}

fn parse_simple_filter(filter: &str) -> Option<SimpleFilter> {
    let trimmed = filter.trim();

    let upper = trimmed.to_uppercase();
    if upper.contains(" AND ") || upper.contains(" OR ") || trimmed.contains('(') {
        return None;
    }

    let ops = &["!=", "<>", "<=", ">=", "=", "<", ">"];
    let kw_ops = &["ILIKE", "LIKE", "IS NOT", "IS"];

    for &kw in kw_ops {
        if let Some(pos) = upper.find(&format!(" {kw} ")) {
            let col_part = trimmed[..pos].trim();
            let val_part = trimmed[pos + kw.len() + 2..].trim();
            let column = unquote_column(col_part)?;
            let value = unquote_value(val_part);
            return Some(SimpleFilter {
                column,
                op: kw.to_string(),
                value,
            });
        }
    }

    for &op in ops {
        if let Some(pos) = trimmed.find(op) {
            let col_part = trimmed[..pos].trim();
            let val_part = trimmed[pos + op.len()..].trim();
            if col_part.is_empty() || val_part.is_empty() {
                continue;
            }
            let column = unquote_column(col_part)?;
            let value = unquote_value(val_part);
            return Some(SimpleFilter {
                column,
                op: op.to_string(),
                value,
            });
        }
    }

    None
}

fn unquote_column(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        Some(s[1..s.len() - 1].to_string())
    } else if s.contains(' ') {
        None
    } else {
        Some(s.to_string())
    }
}

fn unquote_value(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn cell_matches_filter(cell_text: &str, filter: &SimpleFilter) -> bool {
    match filter.op.as_str() {
        "=" => cell_text == filter.value,
        "!=" | "<>" => cell_text != filter.value,
        ">" | "<" | ">=" | "<=" => {
            if let (Ok(cell_num), Ok(filter_num)) = (
                cell_text.parse::<f64>(),
                filter.value.parse::<f64>(),
            ) {
                match filter.op.as_str() {
                    ">" => cell_num > filter_num,
                    "<" => cell_num < filter_num,
                    ">=" => cell_num >= filter_num,
                    "<=" => cell_num <= filter_num,
                    _ => false,
                }
            } else {
                match filter.op.as_str() {
                    ">" => cell_text > filter.value.as_str(),
                    "<" => cell_text < filter.value.as_str(),
                    ">=" => cell_text >= filter.value.as_str(),
                    "<=" => cell_text <= filter.value.as_str(),
                    _ => false,
                }
            }
        }
        "LIKE" => sql_like_match(cell_text, &filter.value, true),
        "ILIKE" => sql_like_match(cell_text, &filter.value, false),
        "IS" => {
            let upper_val = filter.value.to_uppercase();
            if upper_val == "NULL" {
                false
            } else {
                false
            }
        }
        "IS NOT" => {
            let upper_val = filter.value.to_uppercase();
            upper_val == "NULL"
        }
        _ => false,
    }
}

fn sql_like_match(text: &str, pattern: &str, case_sensitive: bool) -> bool {
    let text = if case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let pattern = if case_sensitive {
        pattern.to_string()
    } else {
        pattern.to_lowercase()
    };
    like_match(text.as_bytes(), pattern.as_bytes())
}

fn like_match(text: &[u8], pattern: &[u8]) -> bool {
    let mut t = 0;
    let mut p = 0;
    let mut star_t = usize::MAX;
    let mut star_p = usize::MAX;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'_' || pattern[p] == text[t]) {
            t += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == b'%' {
            star_t = t;
            star_p = p;
            p += 1;
        } else if star_p != usize::MAX {
            star_t += 1;
            t = star_t;
            p = star_p + 1;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'%' {
        p += 1;
    }

    p == pattern.len()
}

fn search_matcher_from_app(app: &App) -> Option<SearchMatcher> {
    let tab = app.tab();
    let term = tab.search.active_search.as_ref()?;
    let is_regex = tab.search.search_mode == SearchMode::Regex;
    SearchMatcher::new(term, is_regex)
}

/// Styler for the normal table view: row/column selection, search highlighting, filter highlighting.
struct NormalStyler {
    /// Buffer-relative index of the selected row (selected_row - buffer_offset).
    selected_data_row: usize,
    col_select_active: bool,
    selected_col: usize,
    search_matcher: Option<SearchMatcher>,
    simple_filter: Option<SimpleFilter>,
    highlight_col_idx: Option<usize>,
    /// Buffer-relative row index of the current search match (for distinct highlighting).
    current_match_data_row: Option<usize>,
    // Theme colors
    selected_bg: Color,
    null_fg: Color,
    search_match_fg: Color,
    search_match_bg: Color,
}

impl TableStyler for NormalStyler {
    fn col_header_style(&self, col_idx: usize, base: Style) -> Style {
        if self.col_select_active && col_idx == self.selected_col {
            base.bg(self.selected_bg)
        } else {
            base
        }
    }

    fn row_prefix(&self, data_row: usize) -> (&str, Style) {
        let is_selected = data_row == self.selected_data_row;
        let show_highlight = is_selected && !self.col_select_active;
        if show_highlight {
            (">> ", Style::default().bg(self.selected_bg).fg(Color::Yellow))
        } else {
            ("   ", Style::default())
        }
    }

    fn row_bg(&self, data_row: usize) -> Style {
        let is_selected = data_row == self.selected_data_row;
        let show_highlight = is_selected && !self.col_select_active;
        if show_highlight {
            Style::default().bg(self.selected_bg)
        } else {
            Style::default()
        }
    }

    fn cell(
        &self,
        col_idx: usize,
        data_row: usize,
        formatted: &str,
        is_null: bool,
        base: Style,
    ) -> (String, Style) {
        let is_col_selected = self.col_select_active && col_idx == self.selected_col;
        let base_style = if is_col_selected {
            Style::default().bg(self.selected_bg)
        } else {
            base
        };

        if is_null {
            return ("NULL".to_string(), base_style.fg(self.null_fg));
        }

        let val = formatted;

        let mut style = if is_col_selected {
            base_style
        } else if self.highlight_col_idx == Some(col_idx) {
            if let Some(ref sf) = self.simple_filter {
                if cell_matches_filter(val, sf) {
                    base_style.fg(Color::Yellow)
                } else {
                    base_style
                }
            } else {
                base_style
            }
        } else {
            base_style
        };

        if let Some(ref matcher) = self.search_matcher {
            if matcher.is_match(val) {
                if self.current_match_data_row == Some(data_row) {
                    style = Style::default()
                        .fg(self.search_match_fg)
                        .bg(self.search_match_bg)
                        .add_modifier(ratatui::style::Modifier::BOLD);
                } else {
                    style = style.fg(Color::Yellow).bg(self.search_match_fg);
                }
            }
        }

        (val.to_string(), style)
    }
}

pub struct TableView<'a> {
    app: &'a App,
}

impl<'a> TableView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for TableView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let tab = self.app.tab();
        let theme = &self.app.theme;
        let schema = match tab.data.schema.as_ref() {
            Some(s) => s,
            None => return,
        };
        let batch = match tab.data.current_batch.as_ref() {
            Some(b) => b,
            None => return,
        };

        if area.height < 2 || area.width < 4 {
            return;
        }

        // If the viewport is completely outside the buffer, show a loading indicator
        // instead of stale/empty data while the async fetch is in flight.
        let buf_end = tab.data.buffer_offset + batch.num_rows();
        let view_end = tab.viewport.view_start + tab.viewport.page_size;
        if tab.viewport.view_start >= buf_end || view_end <= tab.data.buffer_offset {
            let spinner = self.app.spinner_char();
            let msg = format!(" {} Loading... ", spinner);
            let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(
                x,
                y,
                &msg,
                ratatui::style::Style::default()
                    .fg(theme.dim)
                    .add_modifier(ratatui::style::Modifier::ITALIC),
            );
            return;
        }

        let view_off = self.app.view_offset_in_buffer();

        let col_select_active = tab.viewport.selection_mode == SelectionMode::Column;
        let selected_col = tab.viewport.selected_col;

        let simple_filter = tab
            .filter
            .active_filter
            .as_ref()
            .and_then(|f| parse_simple_filter(f));

        let highlight_col_idx: Option<usize> = simple_filter.as_ref().and_then(|sf| {
            schema
                .fields()
                .iter()
                .position(|f| f.name().eq_ignore_ascii_case(&sf.column))
        });

        let search_matcher = search_matcher_from_app(self.app);

        let selected_data_row = tab
            .viewport
            .selected_row
            .saturating_sub(tab.data.buffer_offset);

        // Compute the buffer-relative row of the current search match
        let current_match_data_row = tab
            .search
            .match_index
            .and_then(|idx| tab.search.match_rows.get(idx.wrapping_sub(1)).copied())
            .and_then(|abs_row| abs_row.checked_sub(tab.data.buffer_offset));

        let styler = NormalStyler {
            selected_data_row,
            col_select_active,
            selected_col,
            search_matcher,
            simple_filter,
            highlight_col_idx,
            current_match_data_row,
            selected_bg: theme.selected_bg,
            null_fg: theme.null_fg,
            search_match_fg: theme.search_match_fg,
            search_match_bg: theme.search_match_bg,
        };

        // Build per-column width overrides
        let overrides = &tab.viewport.col_width_overrides;
        let default_max: u16 = crate::ui::table_render::DEFAULT_MAX_COL_WIDTH;

        // For columns with positive override: set a minimum width so they actually expand.
        // For columns with negative override: cap reduces below default.
        let col_mins: Vec<(usize, u16)> = overrides
            .iter()
            .enumerate()
            .filter(|&(_, v)| *v > 0)
            .map(|(i, v)| (i, (default_max as i16 + v).max(4) as u16))
            .collect();

        let col_caps: Vec<u16> = if overrides.is_empty() {
            vec![default_max; schema.fields().len()]
        } else {
            overrides
                .iter()
                .map(|v| (default_max as i16 + v).max(4) as u16)
                .collect()
        };
        let global_max = col_caps.iter().copied().max().unwrap_or(default_max);

        let hidden = &tab.hidden_columns;

        let mut table = UnifiedTable::new(schema, batch, &styler)
            .scroll_offset(view_off)
            .column_offset(tab.viewport.column_offset)
            .sort_state(&tab.data.sort_state)
            .max_col_width(global_max)
            .col_width_caps(&col_caps)
            .row_numbers(tab.data.buffer_offset, tab.data.total_rows)
            .theme(theme);

        if !col_mins.is_empty() {
            table = table.col_width_mins(&col_mins);
        }
        if !hidden.is_empty() {
            table = table.hidden(hidden);
        }

        table.render(area, buf);
    }
}
