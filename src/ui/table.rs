use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::app::App;
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

enum SearchMatcher {
    Plain(String),
    Regex(regex::Regex),
}

impl SearchMatcher {
    fn from_search_state(app: &App) -> Option<Self> {
        let tab = app.tab();
        let term = tab.search.active_search.as_ref()?;
        match tab.search.search_mode {
            SearchMode::Regex => {
                let re = regex::RegexBuilder::new(term)
                    .case_insensitive(true)
                    .build()
                    .ok()?;
                Some(SearchMatcher::Regex(re))
            }
            SearchMode::Plain => Some(SearchMatcher::Plain(term.to_lowercase())),
        }
    }

    fn is_match(&self, val: &str) -> bool {
        match self {
            SearchMatcher::Plain(term) => val.to_lowercase().contains(term.as_str()),
            SearchMatcher::Regex(re) => re.is_match(val),
        }
    }
}

/// Styler for the normal table view: row/column selection, search highlighting, filter highlighting.
struct NormalStyler {
    selected_row_in_view: usize,
    visible_count: usize,
    col_select_active: bool,
    selected_col: usize,
    search_matcher: Option<SearchMatcher>,
    simple_filter: Option<SimpleFilter>,
    highlight_col_idx: Option<usize>,
}

impl TableStyler for NormalStyler {
    fn col_header_style(&self, col_idx: usize, base: Style) -> Style {
        if self.col_select_active && col_idx == self.selected_col {
            base.bg(Color::DarkGray)
        } else {
            base
        }
    }

    fn row_prefix(&self, data_row: usize) -> (&str, Style) {
        let display_row = data_row; // data_row is relative to scroll_offset
        let is_selected = display_row < self.visible_count
            && display_row == self.selected_row_in_view;
        let show_highlight = is_selected && !self.col_select_active;
        if show_highlight {
            (">> ", Style::default().bg(Color::DarkGray).fg(Color::Yellow))
        } else {
            ("   ", Style::default())
        }
    }

    fn row_bg(&self, data_row: usize) -> Style {
        let display_row = data_row;
        let is_selected = display_row < self.visible_count
            && display_row == self.selected_row_in_view;
        let show_highlight = is_selected && !self.col_select_active;
        if show_highlight {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        }
    }

    fn cell(
        &self,
        col_idx: usize,
        _data_row: usize,
        formatted: &str,
        is_null: bool,
        base: Style,
    ) -> (String, Style) {
        let is_col_selected = self.col_select_active && col_idx == self.selected_col;
        let base_style = if is_col_selected {
            Style::default().bg(Color::DarkGray)
        } else {
            base
        };

        if is_null {
            return ("NULL".to_string(), base_style.fg(Color::DarkGray));
        }

        // We need the actual formatted value for filter/search matching
        // The formatted value is passed in, but for "?" fallback we use what's given
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
                style = style.fg(Color::Yellow).bg(Color::Black);
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

        let view_off = self.app.view_offset_in_buffer();
        let visible_count = self.app.visible_row_count();

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

        let search_matcher = SearchMatcher::from_search_state(self.app);

        let styler = NormalStyler {
            selected_row_in_view: tab.viewport.selected_row_in_view(),
            visible_count,
            col_select_active,
            selected_col,
            search_matcher,
            simple_filter,
            highlight_col_idx,
        };

        UnifiedTable::new(schema, batch, &styler)
            .scroll_offset(view_off)
            .column_offset(tab.viewport.column_offset)
            .sort_state(&tab.data.sort_state)
            .render(area, buf);
    }
}
