use duckdb::arrow::datatypes::DataType;
use ratatui::style::Color;
use terminal_colorsaurus::{color_palette, QueryOptions};

#[derive(Debug, Clone)]
pub struct Theme {
    // Base
    pub fg: Color,
    pub bg: Color,
    pub dim: Color,
    pub selected_bg: Color,

    // Data type colors
    pub type_number: Color,
    pub type_string: Color,
    pub type_bool: Color,
    pub type_temporal: Color,
    pub type_binary: Color,
    pub type_other: Color,

    // Semantic
    pub null_fg: Color,
    pub gutter_fg: Color,
    pub header_fg: Color,
    pub search_match_fg: Color,
    pub search_match_bg: Color,

    // Bars
    pub status_bg: Color,
    pub status_fg: Color,
    pub input_bg: Color,
    pub input_fg: Color,
    pub tab_inactive_bg: Color,
    pub tab_inactive_fg: Color,
    pub tab_active_bg: Color,
    pub tab_active_fg: Color,

    // Diff
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_changed: Color,
    pub diff_header_bg: Color,
    pub diff_header_fg: Color,

    // Popups / overlays
    pub popup_border: Color,
    pub popup_title: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            fg: Color::White,
            bg: Color::Black,
            dim: Color::DarkGray,
            selected_bg: Color::DarkGray,

            type_number: Color::Green,
            type_string: Color::Yellow,
            type_bool: Color::Magenta,
            type_temporal: Color::Blue,
            type_binary: Color::Red,
            type_other: Color::White,

            null_fg: Color::DarkGray,
            gutter_fg: Color::DarkGray,
            header_fg: Color::Cyan,
            search_match_fg: Color::Black,
            search_match_bg: Color::Yellow,

            status_bg: Color::DarkGray,
            status_fg: Color::White,
            input_bg: Color::Black,
            input_fg: Color::White,
            tab_inactive_bg: Color::DarkGray,
            tab_inactive_fg: Color::White,
            tab_active_bg: Color::Blue,
            tab_active_fg: Color::White,

            diff_added: Color::Green,
            diff_removed: Color::Red,
            diff_changed: Color::Yellow,
            diff_header_bg: Color::Cyan,
            diff_header_fg: Color::Black,

            popup_border: Color::Cyan,
            popup_title: Color::Cyan,
        }
    }

    pub fn light() -> Self {
        Self {
            fg: Color::Black,
            bg: Color::White,
            dim: Color::Gray,
            selected_bg: Color::Rgb(220, 220, 240),

            type_number: Color::Green,
            type_string: Color::Rgb(180, 142, 0),
            type_bool: Color::Magenta,
            type_temporal: Color::Blue,
            type_binary: Color::Red,
            type_other: Color::Black,

            null_fg: Color::Gray,
            gutter_fg: Color::Gray,
            header_fg: Color::Rgb(0, 139, 139),
            search_match_fg: Color::Black,
            search_match_bg: Color::Yellow,

            status_bg: Color::Gray,
            status_fg: Color::Black,
            input_bg: Color::White,
            input_fg: Color::Black,
            tab_inactive_bg: Color::Gray,
            tab_inactive_fg: Color::Black,
            tab_active_bg: Color::Blue,
            tab_active_fg: Color::White,

            diff_added: Color::Green,
            diff_removed: Color::Red,
            diff_changed: Color::Rgb(180, 142, 0),
            diff_header_bg: Color::Rgb(0, 139, 139),
            diff_header_fg: Color::White,

            popup_border: Color::Rgb(0, 139, 139),
            popup_title: Color::Rgb(0, 139, 139),
        }
    }

    pub fn detect() -> Self {
        match color_palette(QueryOptions::default()) {
            Ok(palette) => {
                let luma = (palette.background.r as f64 * 0.299
                    + palette.background.g as f64 * 0.587
                    + palette.background.b as f64 * 0.114)
                    / 65535.0;
                if luma > 0.5 {
                    Self::light()
                } else {
                    Self::dark()
                }
            }
            Err(_) => Self::dark(),
        }
    }

    pub fn type_color(&self, data_type: &DataType) -> Color {
        match data_type {
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _) => self.type_number,

            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => self.type_string,

            DataType::Boolean => self.type_bool,

            DataType::Date32
            | DataType::Date64
            | DataType::Timestamp(_, _)
            | DataType::Time32(_)
            | DataType::Time64(_)
            | DataType::Duration(_)
            | DataType::Interval(_) => self.type_temporal,

            DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => {
                self.type_binary
            }

            _ => self.type_other,
        }
    }
}
