use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::theme::Theme;

/// A centered popup frame with title, footer, and bordered content area.
///
/// Usage:
/// ```ignore
/// let popup = Popup::new("Title", "footer text", content_width, content_height);
/// let inner = popup.render_frame(area, buf);
/// // render your content into `inner` — each row already has borders on both sides
/// ```
pub struct Popup<'a> {
    title: &'a str,
    footer: &'a str,
    inner_width: usize,
    inner_height: usize,
    border_color: Color,
    title_color: Color,
    dim_color: Color,
}

impl<'a> Popup<'a> {
    pub fn new(
        title: &'a str,
        footer: &'a str,
        inner_width: usize,
        inner_height: usize,
        theme: &Theme,
    ) -> Self {
        Self {
            title,
            footer,
            inner_width,
            inner_height,
            border_color: theme.popup_border,
            title_color: theme.popup_title,
            dim_color: theme.dim,
        }
    }

    /// Render the popup frame (clear background, borders, title, footer).
    ///
    /// Returns the inner `Rect` where content should be rendered.
    /// The inner rect has width = `inner_width` and is positioned inside the borders.
    /// Callers should use `render_row()` to draw individual content rows with side borders.
    pub fn render_frame(&self, area: Rect, buf: &mut Buffer) -> Rect {
        let border_style = Style::new().fg(self.border_color);
        let title_style = Style::new().fg(self.title_color).add_modifier(Modifier::BOLD);
        let dim_style = Style::new().fg(self.dim_color);

        let popup_width = self
            .inner_width
            .max(self.title.width() + 4)
            .max(self.footer.width() + 4)
            .min(area.width as usize - 2);

        let max_content_height = (area.height as usize).saturating_sub(4);
        let content_height = self.inner_height.min(max_content_height).max(1);
        let popup_height = content_height + 4; // top border + blank + content + blank/footer

        let popup_x = area.x + (area.width.saturating_sub(popup_width as u16 + 2)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height as u16)) / 2;

        // Clear background
        let clear_width = (popup_width + 2).min(area.width as usize);
        for row in 0..popup_height as u16 {
            let y = popup_y + row;
            if y < area.y + area.height {
                buf.set_string(popup_x, y, &" ".repeat(clear_width), Style::default());
            }
        }

        // Top border: ┌─ Title ───┐
        let top_border = format!(
            "\u{250c}\u{2500} {} {}\u{2510}",
            self.title,
            "\u{2500}".repeat(popup_width.saturating_sub(self.title.width() + 3))
        );
        buf.set_string(popup_x, popup_y, &top_border, border_style);
        buf.set_string(popup_x + 3, popup_y, self.title, title_style);

        // Blank line after title
        let bordered_blank = format!("\u{2502}{}\u{2502}", " ".repeat(popup_width));
        buf.set_string(popup_x, popup_y + 1, &bordered_blank, border_style);

        // Content rows get side borders
        for i in 0..content_height {
            let y = popup_y + 2 + i as u16;
            buf.set_string(popup_x, y, "\u{2502}", border_style);
            let right_x = popup_x + popup_width as u16 + 1;
            if right_x < area.x + area.width {
                buf.set_string(right_x, y, "\u{2502}", border_style);
            }
        }

        // Blank line before footer
        let footer_blank_y = popup_y + 2 + content_height as u16;
        buf.set_string(popup_x, footer_blank_y, &bordered_blank, border_style);

        // Bottom border: └ footer ───┘
        let bottom_y = footer_blank_y + 1;
        let footer_padded = format!(
            "\u{2514} {} {}\u{2518}",
            self.footer,
            "\u{2500}".repeat(popup_width.saturating_sub(self.footer.width() + 2))
        );
        buf.set_string(popup_x, bottom_y, &footer_padded, border_style);
        buf.set_string(popup_x + 2, bottom_y, self.footer, dim_style);

        // Return inner content rect (inside the borders, below the blank line)
        Rect {
            x: popup_x + 1,
            y: popup_y + 2,
            width: popup_width as u16,
            height: content_height as u16,
        }
    }
}
