use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::App;
use super::popup::Popup;

const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigation",
        &[
            ("j/k, Up/Down", "Move row up/down"),
            ("h/l, Left/Right", "Scroll columns"),
            ("Ctrl-d / Ctrl-u", "Half-page down/up"),
            ("PgUp/PgDn", "Full page up/down"),
            ("g / G", "Jump to first/last row"),
            (":", "Go to row number"),
            ("0", "Jump to first column"),
            ("Ctrl-f", "Jump to column"),
            ("Tab", "Toggle ROW/COL mode"),
        ],
    ),
    (
        "Search & Filter",
        &[
            ("/", "Search (regex)"),
            ("n / N", "Next/previous match"),
            ("f", "Filter (SQL WHERE)"),
            ("Esc", "Clear search & filter"),
        ],
    ),
    (
        "Display",
        &[
            ("s", "Cycle sort on column"),
            ("H", "Show/hide columns"),
            ("r", "Reset view"),
            ("W", "Toggle line wrap"),
            ("F", "Freeze/unfreeze columns"),
            ("S", "Statistics overlay"),
        ],
    ),
    (
        "Tools",
        &[
            ("m/M", "Mark row / clear marks"),
            ("{/}", "Next/prev marked row"),
            ("e", "SQL scratchpad"),
            ("[/]", "Switch tabs"),
            ("?", "This help screen"),
            ("q", "Quit"),
        ],
    ),
];

pub struct HelpOverlay<'a> {
    app: &'a App,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let _ = self.app;

        // Calculate content dimensions
        let mut total_lines = 0usize;
        let mut max_key_width = 0usize;
        let mut max_desc_width = 0usize;
        for (_title, entries) in SECTIONS {
            total_lines += 1; // section title
            total_lines += entries.len();
            total_lines += 1; // blank line after section
            for (key, desc) in *entries {
                max_key_width = max_key_width.max(key.len());
                max_desc_width = max_desc_width.max(desc.len());
            }
        }
        total_lines = total_lines.saturating_sub(1); // no trailing blank

        let title = "Keybindings";
        let footer = "Press Esc or ? to close";
        let content_width = max_key_width + 3 + max_desc_width + 4; // padding

        let theme = &self.app.theme;
        let popup = Popup::new(title, footer, content_width, total_lines, theme);
        let inner = popup.render_frame(area, buf);

        let section_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let key_style = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(theme.fg);

        let mut y = inner.y;
        let max_y = inner.y + inner.height;

        for (section_idx, (section_title, entries)) in SECTIONS.iter().enumerate() {
            if y >= max_y {
                break;
            }

            // Section title
            let padded = format!(" {:<width$}", section_title, width = inner.width as usize - 1);
            buf.set_string(inner.x, y, &padded, section_style);
            y += 1;

            for (key, desc) in *entries {
                if y >= max_y {
                    break;
                }

                let key_display = format!("  {:>width$}", key, width = max_key_width);
                buf.set_string(inner.x, y, &key_display, key_style);
                let desc_x = inner.x + key_display.len() as u16;
                let desc_padded = format!(
                    "  {:<width$}",
                    desc,
                    width = (inner.width as usize).saturating_sub(key_display.len() + 2)
                );
                buf.set_string(desc_x, y, &desc_padded, desc_style);
                y += 1;
            }

            // Blank line between sections (except last)
            if section_idx + 1 < SECTIONS.len() && y < max_y {
                buf.set_string(
                    inner.x,
                    y,
                    &" ".repeat(inner.width as usize),
                    Style::default(),
                );
                y += 1;
            }
        }
    }
}
