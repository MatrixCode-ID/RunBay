//! ANSI SGR parsing: turns a line with escape sequences into styled segments.
//!
//! Supported: the 16 basic colors (30–37, 90–97, and backgrounds are ignored), 256-color
//! `38;5;n`, 24-bit `38;2;r;g;b`, bold (1), and resets (0, 22, 39). Every other escape
//! sequence is dropped.

use vte::{Params, Parser, Perform};

/// A foreground color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// One of the 256 palette colors (0–15 are the basic colors).
    Indexed(u8),
    /// A 24-bit color.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Resolves the color to RGB using the xterm 256-color palette for indexed colors.
    /// `dark` picks a palette for the basic colors that reads well on a dark background.
    pub fn to_rgb(self, dark: bool) -> (u8, u8, u8) {
        match self {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(i) if i < 16 => {
                const LIGHT: [(u8, u8, u8); 16] = [
                    (0x1f, 0x1f, 0x1f),
                    (0xc0, 0x1c, 0x28),
                    (0x26, 0x80, 0x2c),
                    (0x9a, 0x6a, 0x00),
                    (0x1a, 0x5f, 0xb4),
                    (0x81, 0x3d, 0x9c),
                    (0x00, 0x7c, 0x89),
                    (0x5e, 0x5c, 0x64),
                    (0x55, 0x55, 0x55),
                    (0xe0, 0x1b, 0x24),
                    (0x2e, 0xa0, 0x43),
                    (0xb5, 0x83, 0x00),
                    (0x35, 0x84, 0xe4),
                    (0x9c, 0x4d, 0xb8),
                    (0x00, 0x96, 0xa5),
                    (0x30, 0x30, 0x30),
                ];
                const DARK: [(u8, u8, u8); 16] = [
                    (0x9a, 0x99, 0x96),
                    (0xf6, 0x61, 0x51),
                    (0x57, 0xe3, 0x89),
                    (0xf8, 0xe4, 0x5c),
                    (0x62, 0xa0, 0xea),
                    (0xc0, 0x61, 0xcb),
                    (0x4f, 0xd6, 0xbe),
                    (0xde, 0xdd, 0xda),
                    (0x9a, 0x99, 0x96),
                    (0xff, 0x7b, 0x63),
                    (0x8f, 0xf0, 0xa4),
                    (0xf9, 0xf0, 0x6b),
                    (0x99, 0xc1, 0xf1),
                    (0xdc, 0x8a, 0xdd),
                    (0x93, 0xdd, 0xc2),
                    (0xff, 0xff, 0xff),
                ];
                if dark {
                    DARK[i as usize]
                } else {
                    LIGHT[i as usize]
                }
            }
            Color::Indexed(i) if i < 232 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                (level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            Color::Indexed(i) => {
                let v = 8 + (i - 232) * 10;
                (v, v, v)
            }
        }
    }
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The text, without escape sequences.
    pub text: String,
    /// Foreground color; `None` uses the default text color.
    pub fg: Option<Color>,
    /// Bold.
    pub bold: bool,
}

/// Current SGR state, carried from line to line by [`AnsiParser`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    fg: Option<Color>,
    bold: bool,
}

struct Collector {
    style: Style,
    segments: Vec<Segment>,
}

impl Collector {
    fn push_char(&mut self, c: char) {
        match self.segments.last_mut() {
            Some(last) if last.fg == self.style.fg && last.bold == self.style.bold => {
                last.text.push(c)
            }
            _ => self.segments.push(Segment {
                text: c.to_string(),
                fg: self.style.fg,
                bold: self.style.bold,
            }),
        }
    }

    fn sgr(&mut self, params: &Params) {
        let values: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        if values.is_empty() {
            self.style = Style::default();
            return;
        }
        let mut i = 0;
        while i < values.len() {
            match values[i] {
                0 => self.style = Style::default(),
                1 => self.style.bold = true,
                22 => self.style.bold = false,
                39 => self.style.fg = None,
                v @ 30..=37 => self.style.fg = Some(Color::Indexed((v - 30) as u8)),
                v @ 90..=97 => self.style.fg = Some(Color::Indexed((v - 90 + 8) as u8)),
                38 => match values.get(i + 1) {
                    Some(5) => {
                        if let Some(&n) = values.get(i + 2) {
                            self.style.fg = Some(Color::Indexed(n.min(255) as u8));
                        }
                        i += 2;
                    }
                    Some(2) => {
                        if let (Some(&r), Some(&g), Some(&b)) =
                            (values.get(i + 2), values.get(i + 3), values.get(i + 4))
                        {
                            self.style.fg = Some(Color::Rgb(r as u8, g as u8, b as u8));
                        }
                        i += 4;
                    }
                    _ => {}
                },
                // Background colors take extra parameters that must be skipped.
                48 => match values.get(i + 1) {
                    Some(5) => i += 2,
                    Some(2) => i += 4,
                    _ => {}
                },
                _ => {}
            }
            i += 1;
        }
    }
}

impl Perform for Collector {
    fn print(&mut self, c: char) {
        self.push_char(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\t' {
            for _ in 0..4 {
                self.push_char(' ');
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _: &[u8], _: bool, action: char) {
        if action == 'm' {
            self.sgr(params);
        }
    }
}

/// Parses lines one after another, keeping the SGR state between lines as a terminal does.
#[derive(Default)]
pub struct AnsiParser {
    parser: Parser,
    style: Style,
}

impl AnsiParser {
    /// Creates a parser with the default style.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses one line (without its line break) into segments.
    pub fn parse_line(&mut self, line: &str) -> Vec<Segment> {
        let mut collector = Collector {
            style: self.style,
            segments: Vec::new(),
        };
        self.parser.advance(&mut collector, line.as_bytes());
        self.style = collector.style;
        collector.segments
    }
}

/// Removes every escape sequence from `line`, keeping only the text.
pub fn strip(line: &str) -> String {
    let mut parser = AnsiParser::new();
    parser
        .parse_line(line)
        .into_iter()
        .map(|s| s.text)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_one_segment() {
        let segs = AnsiParser::new().parse_line("hello world");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hello world");
        assert_eq!(segs[0].fg, None);
    }

    #[test]
    fn basic_and_bright_colors_and_bold() {
        let segs = AnsiParser::new().parse_line("\x1b[31mred\x1b[0m \x1b[1;92mok\x1b[22m!");
        assert_eq!(segs[0].text, "red");
        assert_eq!(segs[0].fg, Some(Color::Indexed(1)));
        assert_eq!(segs[1].text, " ");
        assert_eq!(segs[1].fg, None);
        assert_eq!(segs[2].text, "ok");
        assert_eq!(segs[2].fg, Some(Color::Indexed(10)));
        assert!(segs[2].bold);
        assert_eq!(segs[3].text, "!");
        assert!(!segs[3].bold);
    }

    #[test]
    fn colors_256_and_truecolor() {
        let segs = AnsiParser::new().parse_line("\x1b[38;5;208ma\x1b[38;2;1;2;3mb\x1b[48;5;1mc");
        assert_eq!(segs[0].fg, Some(Color::Indexed(208)));
        assert_eq!(segs[1].fg, Some(Color::Rgb(1, 2, 3)));
        // Background is ignored and does not disturb the foreground.
        assert_eq!(segs[1].text, "bc");
    }

    #[test]
    fn other_escapes_are_dropped() {
        assert_eq!(strip("\x1b[2K\x1b[1Gdone\x1b]0;title\x07"), "done");
    }

    #[test]
    fn state_carries_across_lines() {
        let mut p = AnsiParser::new();
        p.parse_line("\x1b[33mstart");
        let segs = p.parse_line("next");
        assert_eq!(segs[0].fg, Some(Color::Indexed(3)));
    }

    #[test]
    fn palette_conversion() {
        assert_eq!(Color::Indexed(16).to_rgb(true), (0, 0, 0));
        assert_eq!(Color::Indexed(231).to_rgb(true), (255, 255, 255));
        assert_eq!(Color::Indexed(232).to_rgb(false), (8, 8, 8));
    }
}
