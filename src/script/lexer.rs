//! Turning script text into tokens.
//!
//! Comments (`//`, `/* */`, nested) and attributes are dropped by the
//! parser; keywords are plain `Ident`s. Every token knows its line and
//! column. Strings, raw strings, characters, byte characters, lifetimes
//! (kept for loop labels), integers (decimal, `0x`, `0o`, `0b`, with `_` and
//! a type suffix) and floats are understood.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    /// `'a`: a lifetime or a loop label.
    Life(String),
    /// Punctuation.
    Sym(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

/// Longest first, so that `..=` is not read as `..` and `=`.
const SYMS: &[&str] = &[
    "<<=", ">>=", "...", "..=", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<", ">>", "..", "+", "-", "*", "/", "%", "^", "!", "&", "|", "=", "<", ">", "@", ".", ",", ";", ":", "#", "$", "?", "(", ")", "[", "]", "{", "}", "~",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Lexer {
    c: Vec<char>,
    i: usize,
    line: u32,
    line_start: usize,
    out: Vec<Token>,
}

impl Lexer {
    fn col(&self, at: usize) -> u32 {
        (at.saturating_sub(self.line_start) + 1) as u32
    }

    fn err<T>(&self, at: usize, msg: &str) -> Result<T, LexError> {
        Err(LexError { line: self.line, col: self.col(at), msg: String::from(msg) })
    }

    fn peek(&self, k: usize) -> char {
        self.c.get(self.i + k).copied().unwrap_or('\0')
    }

    fn newline(&mut self) {
        self.line += 1;
        self.line_start = self.i + 1;
    }

    fn push(&mut self, tok: Tok, line: u32, col: u32) {
        self.out.push(Token { tok, line, col });
    }

    /// After a backslash (`i` is just past it): the character it stands for.
    fn escape(&mut self) -> Result<char, LexError> {
        let at = self.i;
        let e = self.peek(0);
        self.i += 1;
        Ok(match e {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'x' => {
                let hex: String = (0..2).map(|k| self.peek(k)).collect();
                self.i += 2;
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(c) if (c as u32) < 0x80 => c,
                    _ => return self.err(at, "a \\x escape must be two hex digits up to 7f"),
                }
            }
            'u' => {
                if self.peek(0) != '{' {
                    return self.err(at, "a \\u escape looks like \\u{1F600}");
                }
                self.i += 1;
                let mut hex = String::new();
                while self.peek(0) != '}' && self.peek(0) != '\0' && hex.len() < 8 {
                    if self.peek(0) != '_' {
                        hex.push(self.peek(0));
                    }
                    self.i += 1;
                }
                if self.peek(0) != '}' {
                    return self.err(at, "a \\u escape looks like \\u{1F600}");
                }
                self.i += 1;
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(c) => c,
                    None => return self.err(at, "not a character: \\u escape"),
                }
            }
            _ => return self.err(at, "unknown escape in a string or character"),
        })
    }

    fn string(&mut self) -> Result<(), LexError> {
        let (line, col) = (self.line, self.col(self.i));
        self.i += 1;
        let mut s = String::new();
        loop {
            if self.i >= self.c.len() {
                return Err(LexError { line, col, msg: String::from("this string is never closed") });
            }
            let ch = self.c[self.i];
            self.i += 1;
            match ch {
                '"' => break,
                '\\' => {
                    if self.peek(0) == '\n' || (self.peek(0) == '\r' && self.peek(1) == '\n') {
                        // A backslash before a line break joins the lines.
                        while self.peek(0) == ' ' || self.peek(0) == '\t' || self.peek(0) == '\r' || self.peek(0) == '\n' {
                            if self.peek(0) == '\n' {
                                self.newline();
                            }
                            self.i += 1;
                        }
                    } else {
                        let e = self.escape()?;
                        s.push(e);
                    }
                }
                '\n' => {
                    s.push('\n');
                    self.line += 1;
                    self.line_start = self.i;
                }
                c => s.push(c),
            }
        }
        self.push(Tok::Str(s), line, col);
        Ok(())
    }

    /// `r"..."`, `r#"..."#`: `i` is at the `r`.
    fn raw_string(&mut self) -> Result<bool, LexError> {
        let mut k = 1;
        let mut hashes = 0;
        while self.peek(k) == '#' {
            hashes += 1;
            k += 1;
        }
        if self.peek(k) != '"' {
            return Ok(false);
        }
        let (line, col) = (self.line, self.col(self.i));
        self.i += k + 1;
        let mut s = String::new();
        loop {
            if self.i >= self.c.len() {
                return Err(LexError { line, col, msg: String::from("this raw string is never closed") });
            }
            let ch = self.c[self.i];
            if ch == '"' && (0..hashes).all(|h| self.peek(1 + h) == '#') {
                self.i += 1 + hashes;
                break;
            }
            if ch == '\n' {
                s.push('\n');
                self.line += 1;
                self.line_start = self.i + 1;
            } else {
                s.push(ch);
            }
            self.i += 1;
        }
        self.push(Tok::Str(s), line, col);
        Ok(true)
    }

    fn quote(&mut self) -> Result<(), LexError> {
        let (line, col, at) = (self.line, self.col(self.i), self.i);
        if self.peek(1) == '\\' {
            self.i += 2;
            let ch = self.escape()?;
            if self.peek(0) != '\'' {
                return self.err(at, "a character literal holds one character");
            }
            self.i += 1;
            self.push(Tok::Char(ch), line, col);
        } else if self.peek(2) == '\'' && self.peek(1) != '\'' {
            let ch = self.peek(1);
            self.i += 3;
            self.push(Tok::Char(ch), line, col);
        } else if is_ident_start(self.peek(1)) {
            self.i += 1;
            let mut name = String::new();
            while is_ident_char(self.peek(0)) {
                name.push(self.peek(0));
                self.i += 1;
            }
            self.push(Tok::Life(name), line, col);
        } else {
            return self.err(at, "a character literal holds one character");
        }
        Ok(())
    }

    fn number(&mut self, after_dot: bool) -> Result<(), LexError> {
        let (line, col, at) = (self.line, self.col(self.i), self.i);
        let radix = if self.peek(0) == '0' && !after_dot {
            match self.peek(1) {
                'x' => 16,
                'o' => 8,
                'b' => 2,
                _ => 10,
            }
        } else {
            10
        };
        if radix != 10 {
            self.i += 2;
            let mut digits = String::new();
            while self.peek(0).is_digit(radix) || self.peek(0) == '_' {
                if self.peek(0) != '_' {
                    digits.push(self.peek(0));
                }
                self.i += 1;
            }
            // A type suffix: u8, i64, usize...
            if self.peek(0) == 'u' || self.peek(0) == 'i' {
                while is_ident_char(self.peek(0)) {
                    self.i += 1;
                }
            }
            return match u64::from_str_radix(&digits, radix) {
                Ok(v) if v <= i64::MAX as u64 => {
                    self.push(Tok::Int(v as i64), line, col);
                    Ok(())
                }
                _ => self.err(at, "this number does not fit in 64 bits"),
            };
        }
        let mut text = String::new();
        let mut float = false;
        while self.peek(0).is_ascii_digit() || self.peek(0) == '_' {
            if self.peek(0) != '_' {
                text.push(self.peek(0));
            }
            self.i += 1;
        }
        if !after_dot {
            if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
                float = true;
                text.push('.');
                self.i += 1;
                while self.peek(0).is_ascii_digit() || self.peek(0) == '_' {
                    if self.peek(0) != '_' {
                        text.push(self.peek(0));
                    }
                    self.i += 1;
                }
            } else if self.peek(0) == '.' && self.peek(1) != '.' && !is_ident_start(self.peek(1)) {
                // `1.`
                float = true;
                self.i += 1;
            }
            let e = self.peek(0);
            let sign = self.peek(1);
            if (e == 'e' || e == 'E') && (sign.is_ascii_digit() || ((sign == '+' || sign == '-') && self.peek(2).is_ascii_digit())) {
                float = true;
                text.push('e');
                self.i += 1;
                if self.peek(0) == '+' || self.peek(0) == '-' {
                    text.push(self.peek(0));
                    self.i += 1;
                }
                while self.peek(0).is_ascii_digit() || self.peek(0) == '_' {
                    if self.peek(0) != '_' {
                        text.push(self.peek(0));
                    }
                    self.i += 1;
                }
            }
            if is_ident_start(self.peek(0)) {
                let mut suffix = String::new();
                while is_ident_char(self.peek(0)) {
                    suffix.push(self.peek(0));
                    self.i += 1;
                }
                if suffix.starts_with('f') {
                    float = true;
                } else if !(suffix.starts_with('u') || suffix.starts_with('i')) {
                    return self.err(at, "unexpected letters after a number");
                }
            }
        }
        if float {
            match text.parse::<f64>() {
                Ok(v) => self.push(Tok::Float(v), line, col),
                Err(_) => return self.err(at, "this is not a number"),
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => self.push(Tok::Int(v), line, col),
                Err(_) => return self.err(at, "this number does not fit in 64 bits"),
            }
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), LexError> {
        while self.i < self.c.len() {
            let ch = self.c[self.i];
            let (line, col) = (self.line, self.col(self.i));
            if ch == '\n' {
                self.newline();
                self.i += 1;
                continue;
            }
            if ch.is_whitespace() {
                self.i += 1;
                continue;
            }
            if ch == '/' && self.peek(1) == '/' {
                while self.i < self.c.len() && self.c[self.i] != '\n' {
                    self.i += 1;
                }
                continue;
            }
            if ch == '/' && self.peek(1) == '*' {
                let mut depth = 1;
                self.i += 2;
                while depth > 0 {
                    if self.i >= self.c.len() {
                        return Err(LexError { line, col, msg: String::from("this /* comment is never closed") });
                    }
                    if self.c[self.i] == '/' && self.peek(1) == '*' {
                        depth += 1;
                        self.i += 2;
                    } else if self.c[self.i] == '*' && self.peek(1) == '/' {
                        depth -= 1;
                        self.i += 2;
                    } else {
                        if self.c[self.i] == '\n' {
                            self.newline();
                        }
                        self.i += 1;
                    }
                }
                continue;
            }
            if ch == '#' && self.peek(1) == '!' && self.i == 0 && self.peek(2) != '[' {
                // A #! line at the very top.
                while self.i < self.c.len() && self.c[self.i] != '\n' {
                    self.i += 1;
                }
                continue;
            }
            if is_ident_start(ch) {
                if ch == 'r' && (self.peek(1) == '"' || (self.peek(1) == '#' && (self.peek(2) == '"' || self.peek(2) == '#'))) && self.raw_string()? {
                    continue;
                }
                if ch == 'b' && self.peek(1) == '\'' {
                    // b'a': a byte.
                    self.i += 1;
                    let at = self.i;
                    let value = if self.peek(1) == '\\' {
                        self.i += 2;
                        self.escape()?
                    } else {
                        let v = self.peek(1);
                        self.i += 2;
                        v
                    };
                    if self.peek(0) != '\'' {
                        return self.err(at, "a byte literal holds one character");
                    }
                    self.i += 1;
                    self.push(Tok::Int(value as i64), line, col);
                    continue;
                }
                let mut name = String::new();
                while is_ident_char(self.peek(0)) {
                    name.push(self.peek(0));
                    self.i += 1;
                }
                if name == "_" {
                    self.push(Tok::Sym("_"), line, col);
                } else {
                    self.push(Tok::Ident(name), line, col);
                }
                continue;
            }
            if ch.is_ascii_digit() {
                let after_dot = matches!(self.out.last(), Some(Token { tok: Tok::Sym("."), .. }));
                self.number(after_dot)?;
                continue;
            }
            if ch == '"' {
                self.string()?;
                continue;
            }
            if ch == '\'' {
                self.quote()?;
                continue;
            }
            let mut found = None;
            for s in SYMS.iter() {
                let n = s.len();
                if (0..n).all(|k| self.peek(k) == s.as_bytes()[k] as char) {
                    found = Some(*s);
                    break;
                }
            }
            match found {
                Some(s) => {
                    self.i += s.len();
                    self.push(Tok::Sym(s), line, col);
                }
                None => return self.err(self.i, "unexpected character"),
            }
        }
        Ok(())
    }
}

/// Split a program into tokens (ending with `Tok::Eof`).
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let mut l = Lexer { c: src.chars().collect(), i: 0, line: 1, line_start: 0, out: Vec::new() };
    l.run()?;
    let (line, col) = (l.line, l.col(l.i));
    l.push(Tok::Eof, line, col);
    Ok(l.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn id(s: &str) -> Tok {
        Tok::Ident(String::from(s))
    }

    #[test]
    fn words_numbers_and_symbols() {
        assert_eq!(toks("let x = 42;"), [id("let"), id("x"), Tok::Sym("="), Tok::Int(42), Tok::Sym(";"), Tok::Eof]);
        assert_eq!(toks("a..=b"), [id("a"), Tok::Sym("..="), id("b"), Tok::Eof]);
        assert_eq!(toks("a::b->c"), [id("a"), Tok::Sym("::"), id("b"), Tok::Sym("->"), id("c"), Tok::Eof]);
    }

    #[test]
    fn number_forms() {
        assert_eq!(toks("1_000 0xff 0b101 0o17 7u8 3i64"), [Tok::Int(1000), Tok::Int(255), Tok::Int(5), Tok::Int(15), Tok::Int(7), Tok::Int(3), Tok::Eof]);
        assert_eq!(toks("1.5 2e3 0.25f32 3f64 1."), [Tok::Float(1.5), Tok::Float(2000.0), Tok::Float(0.25), Tok::Float(3.0), Tok::Float(1.0), Tok::Eof]);
    }

    #[test]
    fn ranges_and_methods_are_not_floats() {
        assert_eq!(toks("0..10"), [Tok::Int(0), Tok::Sym(".."), Tok::Int(10), Tok::Eof]);
        assert_eq!(toks("1.max(2)"), [Tok::Int(1), Tok::Sym("."), id("max"), Tok::Sym("("), Tok::Int(2), Tok::Sym(")"), Tok::Eof]);
    }

    #[test]
    fn a_tuple_index_chain_is_two_integers() {
        assert_eq!(toks("t.0.1"), [id("t"), Tok::Sym("."), Tok::Int(0), Tok::Sym("."), Tok::Int(1), Tok::Eof]);
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(toks("\"a\\nb\\\"c\\u{41}\""), [Tok::Str(String::from("a\nb\"cA")), Tok::Eof]);
        assert_eq!(toks("r#\"x \"y\" z\"#"), [Tok::Str(String::from("x \"y\" z")), Tok::Eof]);
        assert_eq!(toks("\"one \\\n    two\""), [Tok::Str(String::from("one two")), Tok::Eof]);
    }

    #[test]
    fn chars_lifetimes_and_labels() {
        assert_eq!(toks("'a' '\\n' b'x'"), [Tok::Char('a'), Tok::Char('\n'), Tok::Int(120), Tok::Eof]);
        assert_eq!(toks("'outer: loop"), [Tok::Life(String::from("outer")), Tok::Sym(":"), id("loop"), Tok::Eof]);
        assert_eq!(toks("&'a str"), [Tok::Sym("&"), Tok::Life(String::from("a")), id("str"), Tok::Eof]);
    }

    #[test]
    fn comments_are_skipped_and_lines_counted() {
        let t = lex("a // x\n/* b /* nested */ c */ d\n\"s\ntwo\" e").unwrap();
        let names: Vec<(Tok, u32)> = t.into_iter().map(|t| (t.tok, t.line)).collect();
        assert_eq!(names, [(id("a"), 1), (id("d"), 2), (Tok::Str(String::from("s\ntwo")), 3), (id("e"), 4), (Tok::Eof, 4)]);
    }

    #[test]
    fn errors_say_where() {
        let e = lex("let s = \"open").unwrap_err();
        assert_eq!((e.line, e.col), (1, 9));
        assert!(e.msg.contains("never closed"));
        assert!(lex("/* open").is_err());
        assert!(lex("let x = `;").is_err());
        assert!(lex("'ab'").is_err());
        assert!(lex("99999999999999999999").is_err());
    }

    #[test]
    fn the_underscore_is_a_symbol() {
        assert_eq!(toks("_ _x"), [Tok::Sym("_"), id("_x"), Tok::Eof]);
    }
}
