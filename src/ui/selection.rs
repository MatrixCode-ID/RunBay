//! Character-based output selection, independent of rendering and ANSI styling.

use std::collections::VecDeque;

/// A position in the run, stable when earlier rows leave the on-screen buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Position {
    pub line: u64,
    pub column: usize,
}

/// Cached plain lines and the selection belonging to one output view.
#[derive(Default)]
pub(super) struct Selection {
    pub first: u64,
    pub lines: VecDeque<String>,
    anchor: Option<Position>,
    focus: Option<Position>,
}

impl Selection {
    pub fn reset(&mut self, first: u64) {
        *self = Self {
            first,
            ..Self::default()
        };
    }

    pub fn next(&self) -> u64 {
        self.first + self.lines.len() as u64
    }

    fn length(&self, line: u64) -> usize {
        self.lines
            .get(line.saturating_sub(self.first) as usize)
            .map_or(0, |text| text.chars().count())
    }

    fn clamp(&self, mut position: Position) -> Position {
        position.line = position
            .line
            .clamp(self.first, self.next().saturating_sub(1).max(self.first));
        position.column = position.column.min(self.length(position.line));
        position
    }

    pub fn position(&self, row: i32, column: i32) -> Position {
        self.clamp(Position {
            line: self.first + row.max(0) as u64,
            column: column.max(0) as usize,
        })
    }

    pub fn start(&mut self, position: Position) {
        let position = self.clamp(position);
        self.anchor = Some(position);
        self.focus = Some(position);
    }

    pub fn extend(&mut self, position: Position) {
        let position = self.clamp(position);
        self.anchor.get_or_insert(position);
        self.focus = Some(position);
    }

    pub fn clear(&mut self) {
        // Keep the caret so Shift+arrows work after a plain click or Escape.
        self.anchor = self.focus;
    }

    pub fn range(&self) -> Option<(Position, Position)> {
        let (anchor, focus) = (self.anchor?, self.focus?);
        (anchor != focus).then_some((anchor.min(focus), anchor.max(focus)))
    }

    pub fn columns(&self, line: u64) -> (i32, i32) {
        let Some((start, end)) = self.range() else {
            return (-1, -1);
        };
        if line < start.line || line > end.line || (line == end.line && end.column == 0) {
            return (-1, -1);
        }
        let from = if line == start.line { start.column } else { 0 };
        let to = if line == end.line {
            end.column
        } else {
            self.length(line)
        };
        (from as i32, to as i32)
    }

    pub fn word(&mut self, position: Position) {
        let position = self.clamp(position);
        let chars: Vec<char> = self
            .lines
            .get((position.line - self.first) as usize)
            .map(|line| line.chars().collect())
            .unwrap_or_default();
        if chars.is_empty() {
            self.start(position);
            return;
        }
        let column = position.column.min(chars.len() - 1);
        let is_word = |c: char| c.is_alphanumeric() || "_-./\\:".contains(c);
        let mut start = column;
        let mut end = column + 1;
        if is_word(chars[column]) {
            while start > 0 && is_word(chars[start - 1]) {
                start -= 1;
            }
            while end < chars.len() && is_word(chars[end]) {
                end += 1;
            }
        }
        self.start(Position {
            column: start,
            ..position
        });
        self.extend(Position {
            column: end,
            ..position
        });
    }

    pub fn line(&mut self, position: Position) {
        let position = self.clamp(position);
        self.start(Position {
            column: 0,
            ..position
        });
        self.extend(Position {
            column: self.length(position.line),
            ..position
        });
    }

    pub fn all(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.start(Position {
            line: self.first,
            column: 0,
        });
        let line = self.next() - 1;
        self.extend(Position {
            line,
            column: self.length(line),
        });
    }

    pub fn key(&mut self, key: &str) {
        if self.lines.is_empty() {
            return;
        }
        let mut position = self.focus.unwrap_or(Position {
            line: self.first,
            column: 0,
        });
        self.anchor.get_or_insert(position);
        match key {
            "left" if position.column > 0 => position.column -= 1,
            "left" if position.line > self.first => {
                position.line -= 1;
                position.column = self.length(position.line);
            }
            "right" if position.column < self.length(position.line) => position.column += 1,
            "right" if position.line + 1 < self.next() => {
                position.line += 1;
                position.column = 0;
            }
            "up" => position.line = position.line.saturating_sub(1),
            "down" => position.line = position.line.saturating_add(1),
            "home" => position.column = 0,
            "end" => position.column = self.length(position.line),
            _ => {}
        }
        self.focus = Some(self.clamp(position));
    }

    pub fn focus_row(&self) -> i32 {
        self.focus
            .map_or(-1, |p| p.line.saturating_sub(self.first) as i32)
    }

    pub fn trim(&mut self, limit: usize) {
        while self.lines.len() > limit {
            self.lines.pop_front();
            self.first += 1;
        }
        if self.lines.is_empty() {
            self.anchor = None;
            self.focus = None;
            return;
        }
        // Endpoints removed by trimming land at the start of the first surviving line.
        for anchor in [true, false] {
            let endpoint = if anchor { self.anchor } else { self.focus };
            let endpoint = endpoint.map(|p| {
                self.clamp(if p.line < self.first {
                    Position {
                        line: self.first,
                        column: 0,
                    }
                } else {
                    p
                })
            });
            if anchor {
                self.anchor = endpoint;
            } else {
                self.focus = endpoint;
            }
        }
    }

    pub fn text(&self) -> String {
        let Some((start, end)) = self.range() else {
            return String::new();
        };
        let last = if end.column == 0 {
            end.line.saturating_sub(1)
        } else {
            end.line
        };
        (start.line..=last)
            .map(|line| {
                let from = if line == start.line { start.column } else { 0 };
                let to = if line == end.line {
                    end.column
                } else {
                    self.length(line)
                };
                self.lines[(line - self.first) as usize]
                    .chars()
                    .skip(from)
                    .take(to.saturating_sub(from))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> Selection {
        Selection {
            first: 40,
            lines: ["alpha βeta", "C:\\src\\my-app.rs ready", "done"]
                .map(String::from)
                .into(),
            ..Selection::default()
        }
    }

    #[test]
    fn multiline_and_reversed_ranges_use_character_columns_and_crlf() {
        let mut selection = output();
        selection.start(selection.position(0, 6));
        selection.extend(selection.position(2, 2));
        assert_eq!(selection.text(), "βeta\r\nC:\\src\\my-app.rs ready\r\ndo");
        let expected = selection.text();
        let (start, end) = selection.range().unwrap();
        selection.start(end);
        selection.extend(start);
        assert_eq!(selection.text(), expected);
        assert_eq!(selection.columns(41), (0, 22));
    }

    #[test]
    fn column_zero_end_does_not_append_an_empty_line() {
        let mut selection = output();
        selection.start(selection.position(0, 0));
        selection.extend(selection.position(1, 0));
        assert_eq!(selection.text(), "alpha βeta");
        assert_eq!(selection.columns(41), (-1, -1));
    }

    #[test]
    fn word_line_all_and_clear() {
        let mut selection = output();
        selection.word(selection.position(1, 9));
        assert_eq!(selection.text(), "C:\\src\\my-app.rs");
        selection.line(selection.position(0, 3));
        assert_eq!(selection.text(), "alpha βeta");
        selection.all();
        assert_eq!(
            selection.text(),
            "alpha βeta\r\nC:\\src\\my-app.rs ready\r\ndone"
        );
        selection.clear();
        assert_eq!(selection.text(), "");
    }

    #[test]
    fn clamps_positions_and_retains_stable_run_indices_on_trim() {
        let mut selection = output();
        selection.start(selection.position(-1, -1));
        selection.extend(selection.position(999, 999));
        selection.trim(2);
        let (start, end) = selection.range().unwrap();
        assert_eq!(
            start,
            Position {
                line: 41,
                column: 0
            }
        );
        assert_eq!(
            end,
            Position {
                line: 42,
                column: 4
            }
        );
        assert_eq!(selection.text(), "C:\\src\\my-app.rs ready\r\ndone");
        selection.trim(0);
        assert!(selection.range().is_none());
    }

    #[test]
    fn shift_keys_cross_line_boundaries_and_clamp_short_lines() {
        let mut selection = output();
        selection.start(selection.position(0, 9));
        selection.key("right");
        selection.key("right");
        assert_eq!(
            selection.focus.unwrap(),
            Position {
                line: 41,
                column: 0
            }
        );
        selection.key("left");
        assert_eq!(selection.focus.unwrap().column, 10);
        selection.key("down");
        selection.key("down");
        assert_eq!(selection.focus.unwrap().column, 4);
        selection.key("home");
        assert_eq!(selection.focus.unwrap().column, 0);
        selection.key("end");
        assert_eq!(selection.focus.unwrap().column, 4);
        selection.reset(0);
        selection.all();
        selection.key("up");
        assert_eq!(selection.text(), "");
    }
}
