use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::App;
use crate::mode::{AppMode, InputVariant};

pub struct InputBar<'a> {
    app: &'a App,
    prompt: &'a str,
    input: &'a str,
}

impl<'a> InputBar<'a> {
    pub fn new(app: &'a App, prompt: &'a str, input: &'a str) -> Self {
        Self { app, prompt, input }
    }
}

impl Widget for InputBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.app.theme;
        let style = Style::default().fg(theme.input_fg).bg(theme.input_bg);
        let prompt_color = match &self.app.mode {
            AppMode::Input(InputVariant::Filter) => ratatui::style::Color::Green,
            AppMode::Input(InputVariant::GoToRow) => ratatui::style::Color::Blue,
            _ => ratatui::style::Color::Yellow,
        };
        buf.set_string(
            area.x,
            area.y,
            " ".repeat(area.width as usize),
            style,
        );
        buf.set_string(area.x, area.y, self.prompt, style.fg(prompt_color));
        buf.set_string(
            area.x + self.prompt.len() as u16,
            area.y,
            self.input,
            style,
        );
    }
}
