use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use vte::{Params, Parser, Perform};

#[derive(Default)]
struct StyledText {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    pending: String,
    fg: Option<Color>,
    bg: Option<Color>,
    modifiers: Modifier,
}

impl StyledText {
    fn style(&self) -> Style {
        let mut style = Style::default().add_modifier(self.modifiers);
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let style = self.style();
            self.spans
                .push(Span::styled(std::mem::take(&mut self.pending), style));
        }
    }

    fn end_line(&mut self) {
        self.flush();
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
    }

    fn sgr(&mut self, params: &Params) {
        self.flush();
        let values: Vec<u16> = params
            .iter()
            .flat_map(|part| part.iter().copied())
            .collect();
        let mut i = 0;
        while i < values.len() {
            match values[i] {
                0 => {
                    self.fg = None;
                    self.bg = None;
                    self.modifiers = Modifier::empty();
                }
                1 => self.modifiers.insert(Modifier::BOLD),
                2 => self.modifiers.insert(Modifier::DIM),
                3 => self.modifiers.insert(Modifier::ITALIC),
                4 => self.modifiers.insert(Modifier::UNDERLINED),
                7 => self.modifiers.insert(Modifier::REVERSED),
                9 => self.modifiers.insert(Modifier::CROSSED_OUT),
                22 => self.modifiers.remove(Modifier::BOLD | Modifier::DIM),
                23 => self.modifiers.remove(Modifier::ITALIC),
                24 => self.modifiers.remove(Modifier::UNDERLINED),
                27 => self.modifiers.remove(Modifier::REVERSED),
                29 => self.modifiers.remove(Modifier::CROSSED_OUT),
                30..=37 | 90..=97 => self.fg = basic_color(values[i]),
                40..=47 | 100..=107 => self.bg = basic_color(values[i] - 10),
                39 => self.fg = None,
                49 => self.bg = None,
                38 | 48 => {
                    let target = if values[i] == 38 {
                        &mut self.fg
                    } else {
                        &mut self.bg
                    };
                    match values.get(i + 1) {
                        Some(5) if values.get(i + 2).is_some_and(|n| *n <= 255) => {
                            *target = Some(Color::Indexed(values[i + 2] as u8));
                            i += 2;
                        }
                        Some(2)
                            if values.get(i + 4).is_some()
                                && values[i + 2..=i + 4].iter().all(|n| *n <= 255) =>
                        {
                            *target = Some(Color::Rgb(
                                values[i + 2] as u8,
                                values[i + 3] as u8,
                                values[i + 4] as u8,
                            ));
                            i += 4;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

impl Perform for StyledText {
    fn print(&mut self, c: char) {
        self.pending.push(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.end_line(),
            b'\t' => self.pending.push('\t'),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if action == 'm' && intermediates.is_empty() && !ignore {
            self.sgr(params);
        }
    }
}

fn basic_color(code: u16) -> Option<Color> {
    Some(match code {
        30 => Color::Black,
        31 => Color::Red,
        32 => Color::Green,
        33 => Color::Yellow,
        34 => Color::Blue,
        35 => Color::Magenta,
        36 => Color::Cyan,
        37 => Color::Gray,
        90 => Color::DarkGray,
        91 => Color::LightRed,
        92 => Color::LightGreen,
        93 => Color::LightYellow,
        94 => Color::LightBlue,
        95 => Color::LightMagenta,
        96 => Color::LightCyan,
        97 => Color::White,
        _ => return None,
    })
}

pub(super) fn styled_lines(output: &[u8]) -> Vec<Line<'static>> {
    let mut parser = Parser::new();
    let mut text = StyledText::default();
    parser.advance(&mut text, output);
    text.flush();
    if !text.spans.is_empty() {
        text.lines.push(Line::from(text.spans));
    }
    text.lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_colors_modifiers_and_resets_across_lines() {
        let lines = styled_lines(
            b"\x1b[1;31mred\nnext\x1b[0m plain\n\x1b[38;5;42mgreen\x1b[48;2;1;2;3m!\x1b[0m",
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "red");
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Red));
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[0].style.fg, Some(Color::Red));
        assert_eq!(lines[1].spans[1].style.fg, None);
        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Indexed(42)));
        assert_eq!(lines[2].spans[1].style.bg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn ignores_non_sgr_controls_and_partial_escapes() {
        let lines = styled_lines(b"a\x1b]52;c;secret\x07b\x1b[2Jc\x1b[31");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, "abc");
    }
}
