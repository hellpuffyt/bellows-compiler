//! Diagnostics with source spans, rendered rustc-style:
//!
//! ```text
//! error[E011]: type mismatch: expected i64, found bool
//!   --> examples/bad.ash:3:13
//!    |
//!  3 |     let x = true + 1;
//!    |             ^^^^ this has type bool
//! ```

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub label: String,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            message: message.into(),
            span,
            label: String::new(),
            notes: Vec::new(),
        }
    }
    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.notes.push(n.into());
        self
    }
}

/// Line/column lookup over a source text.
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> Source<'a> {
    pub fn new(name: &'a str, text: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Source {
            name,
            text,
            line_starts,
        }
    }

    /// 1-based (line, column) of a byte offset.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset);
        let start = self.line_starts[line - 1];
        let col = self.text[start..offset].chars().count() + 1;
        (line, col)
    }

    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .map_or(self.text.len(), |e| e - 1);
        self.text[start..end].trim_end_matches('\r')
    }

    pub fn render(&self, d: &Diagnostic, color: bool) -> String {
        let (red, bold, blue, reset) = if color {
            ("\x1b[31;1m", "\x1b[1m", "\x1b[34;1m", "\x1b[0m")
        } else {
            ("", "", "", "")
        };
        let (line, col) = self.position(d.span.start);
        let (eline, ecol) = self.position(d.span.end.max(d.span.start));
        let width = (d.span.end.max(d.span.start + 1) - d.span.start).max(1);
        let carets = if eline == line {
            ecol.saturating_sub(col).max(1)
        } else {
            width.min(self.line_text(line).len().saturating_sub(col - 1).max(1))
        };
        let gutter = line.to_string().len();
        let mut s = format!(
            "{red}error[{}]{reset}{bold}: {}{reset}\n",
            d.code, d.message
        );
        s += &format!(
            "{:>gutter$}{blue}-->{reset} {}:{}:{}\n",
            "",
            self.name,
            line,
            col,
            gutter = gutter + 1
        );
        s += &format!("{:>gutter$} {blue}|{reset}\n", "", gutter = gutter + 1);
        s += &format!(
            "{blue}{line:>gutter$} |{reset} {}\n",
            self.line_text(line),
            gutter = gutter + 1
        );
        s += &format!(
            "{:>gutter$} {blue}|{reset} {}{red}{}{reset}{}\n",
            "",
            " ".repeat(col - 1),
            "^".repeat(carets),
            if d.label.is_empty() {
                String::new()
            } else {
                format!(" {red}{}{reset}", d.label)
            },
            gutter = gutter + 1
        );
        for n in &d.notes {
            s += &format!(
                "{:>gutter$} {blue}={reset} {bold}note{reset}: {n}\n",
                "",
                gutter = gutter + 1
            );
        }
        s
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)
    }
}

pub type Result<T> = std::result::Result<T, Diagnostic>;
