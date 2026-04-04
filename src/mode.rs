use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};

use crate::app::App;
use crate::ui::sql_pad::SQL_PAD_HEIGHT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Input(InputVariant),
    Sql,
    Overlay(OverlayVariant),
    Quitting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputVariant {
    Search,
    Filter,
    GoToRow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayVariant {
    Stats,
    Help,
    ColumnPicker,
    ColumnJump,
}

impl AppMode {
    pub fn badge(&self) -> &'static str {
        match self {
            AppMode::Normal => " NORMAL ",
            AppMode::Input(InputVariant::Search) => " SEARCH ",
            AppMode::Input(InputVariant::Filter) => " FILTER ",
            AppMode::Input(InputVariant::GoToRow) => " GOTO ",
            AppMode::Sql => " SQL ",
            AppMode::Overlay(OverlayVariant::Stats) => " STATS ",
            AppMode::Overlay(OverlayVariant::Help) => " HELP ",
            AppMode::Overlay(OverlayVariant::ColumnPicker) => " COLS ",
            AppMode::Overlay(OverlayVariant::ColumnJump) => " JUMP ",
            AppMode::Quitting => " QUIT ",
        }
    }

    pub fn badge_style(&self) -> Style {
        let bold = Modifier::BOLD;
        match self {
            AppMode::Normal => Style::default().bg(Color::Blue).fg(Color::White).add_modifier(bold),
            AppMode::Input(InputVariant::Search) => Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(bold),
            AppMode::Input(InputVariant::Filter) => Style::default().bg(Color::Green).fg(Color::Black).add_modifier(bold),
            AppMode::Input(InputVariant::GoToRow) => Style::default().bg(Color::Blue).fg(Color::White).add_modifier(bold),
            AppMode::Sql => Style::default().bg(Color::Magenta).fg(Color::White).add_modifier(bold),
            AppMode::Overlay(OverlayVariant::Stats) => Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(bold),
            AppMode::Overlay(OverlayVariant::Help) => Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(bold),
            AppMode::Overlay(OverlayVariant::ColumnPicker) => Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(bold),
            AppMode::Overlay(OverlayVariant::ColumnJump) => Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(bold),
            AppMode::Quitting => Style::default().bg(Color::Red).fg(Color::White).add_modifier(bold),
        }
    }

    pub fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            AppMode::Normal => &[
                ("?", "help"),
                ("q", "quit"),
                ("/", "search"),
                ("f", "filter"),
                ("s", "sort"),
                (":", "goto"),
                ("r", "reset"),
                ("Tab", "col mode"),
            ],
            AppMode::Input(InputVariant::Search) => &[("Enter", "apply"), ("Esc", "cancel")],
            AppMode::Input(InputVariant::Filter) => &[
                ("Enter", "apply"),
                ("Esc", "cancel"),
                ("$col", "autocomplete"),
                ("Tab", "complete"),
            ],
            AppMode::Input(InputVariant::GoToRow) => &[("Enter", "go"), ("Esc", "cancel")],
            AppMode::Sql => &[("F5/Ctrl-e", "execute"), ("Ctrl-t", "pin tab"), ("Esc", "cancel")],
            AppMode::Overlay(OverlayVariant::Stats) => &[("j/k", "scroll"), ("g/G", "top/bottom"), ("Esc", "close")],
            AppMode::Overlay(OverlayVariant::Help) => &[("Esc", "close")],
            AppMode::Overlay(OverlayVariant::ColumnPicker) => &[
                ("Space", "toggle"),
                ("Enter", "confirm"),
                ("Esc", "cancel"),
            ],
            AppMode::Overlay(OverlayVariant::ColumnJump) => &[
                ("Enter", "jump"),
                ("Esc", "cancel"),
            ],
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
            AppMode::Sql => vec![
                Constraint::Min(3),
                Constraint::Length(SQL_PAD_HEIGHT),
                Constraint::Length(1),
            ],
            AppMode::Input(_) => vec![
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ],
            AppMode::Overlay(OverlayVariant::ColumnPicker)
            | AppMode::Overlay(OverlayVariant::Help)
            | AppMode::Overlay(OverlayVariant::Stats)
            | AppMode::Overlay(OverlayVariant::ColumnJump) => {
                vec![Constraint::Min(3), Constraint::Length(1)]
            }
            _ => vec![Constraint::Min(3), Constraint::Length(1)],
        }
    }

    pub fn cursor_position(&self, app: &App, area_height: u16) -> Option<(u16, u16)> {
        match self {
            AppMode::Input(_) => {
                use unicode_width::UnicodeWidthStr;
                let input_y = area_height - 2;
                let ti = &app.text_input;
                let cursor_x = crate::ui::INPUT_PROMPT_WIDTH + ti.input[..ti.cursor_byte_pos()].width() as u16;
                Some((cursor_x, input_y))
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

    pub fn is_input(&self) -> bool {
        matches!(self, AppMode::Input(_))
    }

    pub fn is_overlay(&self) -> bool {
        matches!(self, AppMode::Overlay(_))
    }
}
