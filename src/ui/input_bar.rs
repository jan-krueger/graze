use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

pub struct InputBar<'a> {
    prompt: &'a str,
    prompt_color: Color,
    input: &'a str,
}

impl<'a> InputBar<'a> {
    pub fn new(prompt: &'a str, prompt_color: Color, input: &'a str) -> Self {
        Self {
            prompt,
            prompt_color,
            input,
        }
    }
}

impl Widget for InputBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default().fg(Color::White).bg(Color::Black);
        buf.set_string(
            area.x,
            area.y,
            " ".repeat(area.width as usize),
            style,
        );
        buf.set_string(area.x, area.y, self.prompt, style.fg(self.prompt_color));
        buf.set_string(
            area.x + self.prompt.len() as u16,
            area.y,
            self.input,
            style,
        );
    }
}
