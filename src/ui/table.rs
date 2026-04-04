use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use std::collections::HashSet;

use crate::app::App;
use crate::search::SearchMatcher;
use crate::state::SelectionMode;
use crate::ui::table_render::{UnifiedTable, TableStyler};

#[derive(Debug)]
struct SimpleFilter {
    column: String,
    op: String,
    value: String,
    /// Pre-lowercased value, computed once at parse time for ILIKE.
    value_lower: String,
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
            let value_lower = value.to_lowercase();
            return Some(SimpleFilter {
                column,
                op: kw.to_string(),
                value,
                value_lower,
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
            let value_lower = value.to_lowercase();
            return Some(SimpleFilter {
                column,
                op: op.to_string(),
                value,
                value_lower,
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
        "LIKE" => sql_like_match(cell_text, &filter.value, &filter.value_lower, true),
        "ILIKE" => sql_like_match(cell_text, &filter.value, &filter.value_lower, false),
        // IS NULL for non-null cells always returns false; null cells are handled above.
        "IS" => false,
        "IS NOT" => {
            let upper_val = filter.value.to_uppercase();
            upper_val == "NULL"
        }
        _ => false,
    }
}

fn sql_like_match(text: &str, pattern: &str, pattern_lower: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        like_match(text.as_bytes(), pattern.as_bytes())
    } else {
        let text_lower = text.to_lowercase();
        like_match(text_lower.as_bytes(), pattern_lower.as_bytes())
    }
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
    SearchMatcher::new(term)
}

/// Styler for the normal table view: row/column selection, search highlighting, filter highlighting.
struct NormalStyler<'a> {
    /// Buffer-relative index of the selected row (selected_row - buffer_offset).
    selected_data_row: usize,
    col_select_active: bool,
    selected_col: usize,
    search_matcher: Option<SearchMatcher>,
    simple_filter: Option<SimpleFilter>,
    highlight_col_idx: Option<usize>,
    /// Buffer-relative row index of the current search match (for distinct highlighting).
    current_match_data_row: Option<usize>,
    /// Marked rows, keyed by buffer-relative row index.
    marked_rows: &'a HashSet<usize>,
    /// Buffer offset to convert buffer-relative rows to absolute rows.
    buffer_offset: usize,
    // Theme colors
    selected_bg: Color,
    mark_bg: Color,
    null_fg: Color,
    search_match_fg: Color,
    search_match_bg: Color,
    search_other_fg: Color,
    search_other_bg: Color,
}

impl TableStyler for NormalStyler<'_> {
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
        let abs_row = self.buffer_offset + data_row;
        let is_marked = self.marked_rows.contains(&abs_row);
        match (show_highlight, is_marked) {
            (true, true) => ("*> ", Style::default().bg(self.selected_bg).fg(Color::Yellow)),
            (true, false) => (">> ", Style::default().bg(self.selected_bg).fg(Color::Yellow)),
            (false, true) => (" * ", Style::default().fg(Color::Yellow)),
            (false, false) => ("   ", Style::default()),
        }
    }

    fn row_bg(&self, data_row: usize) -> Style {
        let is_selected = data_row == self.selected_data_row;
        let show_highlight = is_selected && !self.col_select_active;
        let abs_row = self.buffer_offset + data_row;
        let is_marked = self.marked_rows.contains(&abs_row);
        if show_highlight {
            Style::default().bg(self.selected_bg)
        } else if is_marked {
            Style::default().bg(self.mark_bg)
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
            if !is_col_selected && self.highlight_col_idx == Some(col_idx) {
                if let Some(ref sf) = self.simple_filter {
                    if sf.op == "IS" && sf.value.eq_ignore_ascii_case("NULL") {
                        return ("NULL".to_string(), base_style.fg(Color::Yellow));
                    }
                }
            }
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
                    style = style.fg(self.search_other_fg).bg(self.search_other_bg);
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
        self.app.table_area.set(area);
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
            marked_rows: &tab.marked_rows,
            buffer_offset: tab.data.buffer_offset,
            selected_bg: theme.selected_bg,
            mark_bg: theme.mark_bg,
            null_fg: theme.null_fg,
            search_match_fg: theme.search_match_fg,
            search_match_bg: theme.search_match_bg,
            search_other_fg: theme.search_other_fg,
            search_other_bg: theme.search_other_bg,
        };

        let hidden = &tab.hidden_columns;

        // Column width caching: compute sample range and check cache
        let data_area_height = (area.height as usize).saturating_sub(1);
        let sample_start = view_off;
        let sample_end = (view_off + data_area_height).min(batch.num_rows());
        let generation = tab.data.batch_generation;

        let cache_hit = {
            let cache_ref = tab.col_width_cache.borrow();
            cache_ref.as_ref().map_or(false, |c| {
                c.generation == generation
                    && c.sample_start == sample_start
                    && c.sample_end == sample_end
            })
        };

        if !cache_hit {
            use crate::ui::table_render::{build_formatters, compute_column_widths, sort_indicator_string, DEFAULT_MAX_COL_WIDTH};
            let formatters = build_formatters(batch);
            let sort_state = &tab.data.sort_state;
            let multi = sort_state.specs().len() > 1;
            let (headers, col_widths) = compute_column_widths(
                schema,
                batch,
                &formatters,
                &|_i, name, type_str| {
                    let sort_ind = sort_indicator_string(sort_state, name, multi);
                    format!("{} [{}]{}", name, type_str, sort_ind)
                },
                (sample_start, sample_end),
                DEFAULT_MAX_COL_WIDTH,
            );
            *tab.col_width_cache.borrow_mut() = Some(crate::state::ColWidthCache {
                generation,
                sample_start,
                sample_end,
                headers,
                col_widths,
            });
        }

        let cache_ref = tab.col_width_cache.borrow();
        let cached = cache_ref.as_ref().unwrap();

        let rows_rendered_cell = std::cell::Cell::new(0usize);

        let mut table = UnifiedTable::new(schema, batch, &styler)
            .scroll_offset(view_off)
            .column_offset(tab.viewport.column_offset)
            .sort_state(&tab.data.sort_state)
            .row_numbers(tab.data.buffer_offset, tab.data.total_rows)
            .theme(theme)
            .precomputed_widths(&cached.headers, &cached.col_widths)
            .wrap(self.app.wrap)
            .frozen_cols(tab.viewport.frozen_cols)
            .rows_rendered(&rows_rendered_cell);

        if !hidden.is_empty() {
            table = table.hidden(hidden);
        }

        table.render(area, buf);

        // When wrapping, report how many rows actually fit so page_size can be updated
        if self.app.wrap {
            let rendered = rows_rendered_cell.get();
            if rendered > 0 {
                tab.viewport.rendered_rows.set(rendered);
            }
        }
    }
}
