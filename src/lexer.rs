//! Tokenizer for Ash.

use crate::diag::{Diagnostic, Result, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Str(String),
    // keywords
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Return,
    True,
    False,
    // punctuation / operators
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semi,
    Arrow,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
    Bang,
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("identifier `{s}`"),
            Tok::Int(n) => format!("integer `{n}`"),
            Tok::Str(_) => "string literal".into(),
            Tok::Eof => "end of file".into(),
            t => format!("`{}`", t.text()),
        }
    }
    pub fn text(&self) -> &'static str {
        match self {
            Tok::Fn => "fn",
            Tok::Let => "let",
            Tok::Mut => "mut",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::Return => "return",
            Tok::True => "true",
            Tok::False => "false",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::Comma => ",",
            Tok::Colon => ":",
            Tok::Semi => ";",
            Tok::Arrow => "->",
            Tok::Assign => "=",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::EqEq => "==",
            Tok::Ne => "!=",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Bang => "!",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(src: &str) -> Result<Vec<Token>> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                return Err(Diagnostic::new(
                    "E001",
                    Span::new(start, start + 2),
                    "unterminated block comment",
                ));
            }
            continue;
        }
        let start = i;
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let tok = match word {
                "fn" => Tok::Fn,
                "let" => Tok::Let,
                "mut" => Tok::Mut,
                "if" => Tok::If,
                "else" => Tok::Else,
                "while" => Tok::While,
                "return" => Tok::Return,
                "true" => Tok::True,
                "false" => Tok::False,
                _ => Tok::Ident(word.to_string()),
            };
            out.push(Token {
                tok,
                span: Span::new(start, i),
            });
            continue;
        }
        if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                i += 1;
            }
            if i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_') {
                return Err(Diagnostic::new(
                    "E001",
                    Span::new(start, i + 1),
                    "invalid suffix on integer literal",
                ));
            }
            let digits: String = src[start..i].chars().filter(|c| *c != '_').collect();
            let n: i64 = digits.parse().map_err(|_| {
                Diagnostic::new(
                    "E001",
                    Span::new(start, i),
                    "integer literal is too large for i64",
                )
                .note("i64 ranges from -9223372036854775808 to 9223372036854775807")
            })?;
            out.push(Token {
                tok: Tok::Int(n),
                span: Span::new(start, i),
            });
            continue;
        }
        if c == b'"' {
            i += 1;
            let mut s = String::new();
            loop {
                match b.get(i) {
                    None | Some(b'\n') => {
                        return Err(Diagnostic::new(
                            "E001",
                            Span::new(start, i),
                            "unterminated string literal",
                        ));
                    }
                    Some(b'"') => {
                        i += 1;
                        break;
                    }
                    Some(b'\\') => {
                        let esc = b.get(i + 1).copied();
                        s.push(match esc {
                            Some(b'n') => '\n',
                            Some(b't') => '\t',
                            Some(b'\\') => '\\',
                            Some(b'"') => '"',
                            _ => {
                                return Err(Diagnostic::new(
                                    "E001",
                                    Span::new(i, i + 2),
                                    "unknown escape sequence",
                                )
                                .note("supported escapes: \\n \\t \\\\ \\\""));
                            }
                        });
                        i += 2;
                    }
                    Some(_) => {
                        let ch = src[i..].chars().next().unwrap();
                        s.push(ch);
                        i += ch.len_utf8();
                    }
                }
            }
            out.push(Token {
                tok: Tok::Str(s),
                span: Span::new(start, i),
            });
            continue;
        }
        let two = src.get(i..i + 2).unwrap_or("");
        let (tok, len) = match two {
            "->" => (Tok::Arrow, 2),
            "<=" => (Tok::Le, 2),
            ">=" => (Tok::Ge, 2),
            "==" => (Tok::EqEq, 2),
            "!=" => (Tok::Ne, 2),
            "&&" => (Tok::AndAnd, 2),
            "||" => (Tok::OrOr, 2),
            _ => (
                match c {
                    b'(' => Tok::LParen,
                    b')' => Tok::RParen,
                    b'{' => Tok::LBrace,
                    b'}' => Tok::RBrace,
                    b',' => Tok::Comma,
                    b':' => Tok::Colon,
                    b';' => Tok::Semi,
                    b'=' => Tok::Assign,
                    b'+' => Tok::Plus,
                    b'-' => Tok::Minus,
                    b'*' => Tok::Star,
                    b'/' => Tok::Slash,
                    b'%' => Tok::Percent,
                    b'<' => Tok::Lt,
                    b'>' => Tok::Gt,
                    b'!' => Tok::Bang,
                    _ => {
                        let ch = src[i..].chars().next().unwrap();
                        return Err(Diagnostic::new(
                            "E001",
                            Span::new(i, i + ch.len_utf8()),
                            format!("unexpected character `{ch}`"),
                        ));
                    }
                },
                1,
            ),
        };
        out.push(Token {
            tok,
            span: Span::new(i, i + len),
        });
        i += len;
    }
    out.push(Token {
        tok: Tok::Eof,
        span: Span::new(b.len(), b.len()),
    });
    Ok(out)
}
