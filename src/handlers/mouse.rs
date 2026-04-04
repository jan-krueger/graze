use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::app::App;
use crate::mode::AppMode;
use crate::ui::table_render::{gutter_width, visible_columns_with_freeze, ROW_PREFIX_WIDTH};

pub(crate) fn handle_mouse(app: &mut App, event: MouseEvent) {
    // Only handle mouse in Normal mode
    if !matches!(app.mode, AppMode::Normal) {
        return;
    }

    match event.kind {
        MouseEventKind::ScrollDown => app.move_down(3),
        MouseEventKind::ScrollUp => app.move_up(3),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_left_click(app, event.column, event.row);
        }
        _ => {}
    }
}

fn handle_left_click(app: &mut App, x: u16, y: u16) {
    // Check tab bar click
    if let Some(tab_y) = app.tab_bar_y.get() {
        if y == tab_y && app.has_tabs() {
            handle_tab_click(app, x);
            return;
        }
    }

    let table_area = app.table_area.get();
    if table_area.width == 0 || table_area.height == 0 {
        return;
    }

    // Check if click is within table area
    if x < table_area.x
        || x >= table_area.x + table_area.width
        || y < table_area.y
        || y >= table_area.y + table_area.height
    {
        return;
    }

    let rel_y = y - table_area.y;

    if rel_y == 0 {
        // Header row click — cycle sort on clicked column
        if let Some(col_idx) = hit_test_column(app, x) {
            app.tab_mut().viewport.selected_col = col_idx;
            app.cycle_sort();
        }
    } else {
        // Data row click
        let data_row_offset = (rel_y as usize).saturating_sub(1);
        let clicked_row = app.tab().viewport.view_start + data_row_offset;
        let total = app.tab().data.total_rows;
        if total > 0 && clicked_row < total {
            app.tab_mut().viewport.selected_row = clicked_row;
            app.tab_mut().viewport.adjust_view();
            app.ensure_buffer();
        }
        // Also select the clicked column
        if let Some(col_idx) = hit_test_column(app, x) {
            app.tab_mut().viewport.selected_col = col_idx;
        }
    }
}

/// Determine which column index the x coordinate falls into.
fn hit_test_column(app: &App, x: u16) -> Option<usize> {
    let table_area = app.table_area.get();
    let tab = app.tab();

    let cache_ref = tab.col_width_cache.borrow();
    let cached = cache_ref.as_ref()?;

    let left_margin = gutter_width(tab.data.total_rows) + ROW_PREFIX_WIDTH;
    let hidden = &tab.hidden_columns;
    let vis_cols = visible_columns_with_freeze(
        &cached.col_widths,
        tab.viewport.terminal_width as usize,
        tab.viewport.column_offset,
        left_margin,
        if hidden.is_empty() { None } else { Some(hidden) },
        tab.viewport.frozen_cols,
    );

    // Walk through visible columns accumulating x positions
    let mut col_x = table_area.x + left_margin as u16;
    for &col_idx in &vis_cols {
        let col_total = cached.col_widths[col_idx] as u16 + 2; // 2 for padding
        if x >= col_x && x < col_x + col_total {
            return Some(col_idx);
        }
        col_x += col_total;
    }

    None
}

fn handle_tab_click(app: &mut App, x: u16) {
    let mut tab_x: u16 = 0;
    for (i, tab) in app.tabs.iter().enumerate() {
        let name = tab.data.file_name.as_deref().unwrap_or("loading...");
        let label_width = name.len() as u16 + 2; // " name "
        if x >= tab_x && x < tab_x + label_width {
            if i != app.active_tab {
                let delta = i as isize - app.active_tab as isize;
                app.switch_tab(delta);
            }
            return;
        }
        tab_x += label_width + 1; // +1 for separator
    }
}
