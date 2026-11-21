//! Predictive local echo, in the spirit of mosh: printable keys typed into the
//! active terminal are drawn at once (underlined) and dropped as soon as the
//! program's own echo shows the same character, or when anything suggests the
//! guess was wrong (control keys, cursor jumps, timeouts).

use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const EXPIRE: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug)]
pub struct Pred {
    pub row: u16,
    pub col: u16,
    pub ch: char,
    at: Instant,
}

pub struct Predictor {
    pub enabled: bool,
    items: Vec<Pred>,
}

impl Predictor {
    pub fn new(enabled: bool) -> Predictor {
        Predictor {
            enabled,
            items: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[Pred] {
        &self.items
    }

    /// Where the cursor should be drawn while predictions are pending.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        self.items
            .last()
            .map(|p| (p.row, p.col + p.ch.width().unwrap_or(1) as u16))
    }

    /// Bytes the user sent to the program.
    pub fn typed(&mut self, bytes: &[u8], screen: &vt100::Screen) {
        if !self.enabled || screen.hide_cursor() {
            self.items.clear();
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.items.clear();
            return;
        };
        let cols = screen.size().1.get();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Arrows, function keys, alt+key: whole sequences, never text.
                // They usually move the cursor, so old guesses are off too.
                self.items.clear();
                match chars.next() {
                    Some('[') => {
                        for c in chars.by_ref() {
                            if ('\x40'..='\x7e').contains(&c) {
                                break;
                            }
                        }
                    }
                    Some('O') => {
                        chars.next();
                    }
                    _ => {}
                }
                continue;
            }
            match ch {
                '\x7f' | '\x08' => {
                    self.items.pop();
                }
                c if c.is_control() => self.items.clear(),
                c => {
                    let w = c.width().unwrap_or(0) as u16;
                    if w == 0 {
                        continue;
                    }
                    let (row, col) = self.cursor().unwrap_or_else(|| screen.cursor_position());
                    if col + w > cols {
                        // Wrapping is the program's business; don't guess.
                        continue;
                    }
                    self.items.push(Pred {
                        row,
                        col,
                        ch: c,
                        at: Instant::now(),
                    });
                }
            }
        }
    }

    /// The program's screen changed: confirm what it now shows, drop stale guesses.
    pub fn reconcile(&mut self, screen: &vt100::Screen) {
        if self.items.is_empty() {
            return;
        }
        if screen.hide_cursor() {
            self.items.clear();
            return;
        }
        let now = Instant::now();
        self.items.retain(|p| now.duration_since(p.at) < EXPIRE);
        // The newest guess the screen already shows confirms it and everything before it.
        let shown = self.items.iter().rposition(|p| {
            screen
                .cell(p.row, p.col)
                .is_some_and(|c| c.contents().starts_with(p.ch))
        });
        if let Some(i) = shown {
            self.items.drain(..=i);
        }
        if let Some(first) = self.items.first() {
            let (row, col) = screen.cursor_position();
            // Echo goes where the cursor is: a cursor elsewhere means our guess is off.
            if row != first.row || col > first.col {
                self.items.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::nz;

    fn parser(rows: u16, cols: u16) -> vt100::Parser {
        vt100::Parser::new(nz(rows), nz(cols), 0)
    }

    #[test]
    fn guesses_follow_the_cursor_and_vanish_when_echoed() {
        let mut parser = parser(5, 20);
        parser.process(b"$ ");
        let mut p = Predictor::new(true);
        p.typed(b"ls", parser.screen());
        assert_eq!(p.items().len(), 2);
        assert_eq!((p.items()[0].row, p.items()[0].col), (0, 2));
        assert_eq!(p.cursor(), Some((0, 4)));
        parser.process(b"l");
        p.reconcile(parser.screen());
        assert_eq!(p.items().len(), 1);
        parser.process(b"s");
        p.reconcile(parser.screen());
        assert!(p.items().is_empty());
    }

    #[test]
    fn control_keys_and_backspace() {
        let parser = parser(5, 20);
        let mut p = Predictor::new(true);
        p.typed(b"abc\x7f", parser.screen());
        assert_eq!(p.items().len(), 2);
        p.typed(b"\r", parser.screen());
        assert!(p.items().is_empty());
    }

    #[test]
    fn escape_sequences_are_not_text() {
        let parser = parser(5, 20);
        let mut p = Predictor::new(true);
        p.typed(b"\x1b[A", parser.screen());
        assert!(p.items().is_empty());
        p.typed(b"\x1bOB", parser.screen());
        assert!(p.items().is_empty());
        p.typed(b"ab\x1b[Dc", parser.screen());
        assert_eq!(p.items().len(), 1);
        assert_eq!(p.items()[0].ch, 'c');
        p.typed(b"\x1b[1;5C", parser.screen());
        assert!(p.items().is_empty());
    }

    #[test]
    fn disabled_predicts_nothing() {
        let parser = parser(5, 20);
        let mut p = Predictor::new(false);
        p.typed(b"abc", parser.screen());
        assert!(p.items().is_empty());
    }
}
