//! A small, strict JSON parser.
//!
//! Parses a complete document into a [`Json`] tree. Objects keep their
//! members in source order. Malformed input gives an error naming the byte
//! offset; nothing here panics.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Deepest nesting of arrays and objects that is accepted.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Object member by key (the first one if the key is repeated).
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Array element by index.
    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(items) => items.get(i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(members) => Some(members.as_slice()),
            _ => None,
        }
    }
}

/// Parses one JSON document. Anything but whitespace after it is an error.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser { text, s: text.as_bytes(), i: 0 };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(p.err("unexpected trailing data"));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, what: &str) -> String {
        format!("json: {} at byte {}", what, self.i)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') = self.peek() {
            self.i += 1;
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(value)
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        if depth >= MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        self.i += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        if depth >= MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        self.i += 1; // {
        let mut members = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected string key"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.err("expected ':'"));
            }
            self.i += 1;
            self.skip_ws();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(members));
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = match self.peek() {
                Some(c @ b'0'..=b'9') => c - b'0',
                Some(c @ b'a'..=b'f') => c - b'a' + 10,
                Some(c @ b'A'..=b'F') => c - b'A' + 10,
                _ => return Err(self.err("invalid \\u escape")),
            };
            v = v << 4 | d as u32;
            self.i += 1;
        }
        Ok(v)
    }

    /// Parses a string starting at its opening quote.
    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // "
        let mut out = String::new();
        let mut run = self.i; // start of the pending unescaped run
        loop {
            let c = match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(c) => c,
            };
            if c != b'"' && c != b'\\' {
                if c < 0x20 {
                    return Err(self.err("control character in string"));
                }
                self.i += 1;
                continue;
            }
            // `run..i` starts and ends next to ASCII bytes, so it is a
            // whole number of characters.
            match self.text.get(run..self.i) {
                Some(part) => out.push_str(part),
                None => return Err(self.err("invalid string")),
            }
            self.i += 1;
            if c == b'"' {
                return Ok(out);
            }
            let e = match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(e) => e,
            };
            self.i += 1;
            match e {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let mut cp = self.hex4()?;
                    if (0xD800..0xDC00).contains(&cp) {
                        // A high surrogate needs a low one right after it.
                        let save = self.i;
                        let mut paired = false;
                        if self.s[self.i..].starts_with(b"\\u") {
                            self.i += 2;
                            let lo = self.hex4()?;
                            if (0xDC00..0xE000).contains(&lo) {
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                paired = true;
                            }
                        }
                        if !paired {
                            self.i = save;
                            cp = 0xFFFD;
                        }
                    }
                    // Lone surrogates become the replacement character.
                    out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                }
                _ => {
                    self.i -= 1;
                    return Err(self.err("invalid escape"));
                }
            }
            run = self.i;
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while let Some(b'0'..=b'9') = self.peek() {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.err("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return Err(self.err("invalid number"));
            }
        }
        if let Some(b'e') | Some(b'E') = self.peek() {
            self.i += 1;
            if let Some(b'+') | Some(b'-') = self.peek() {
                self.i += 1;
            }
            if self.digits() == 0 {
                return Err(self.err("invalid number"));
            }
        }
        let parsed = self.text.get(start..self.i).and_then(|t| t.parse::<f64>().ok());
        match parsed {
            Some(n) => Ok(Json::Num(n)),
            None => {
                self.i = start;
                Err(self.err("invalid number"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars() {
        assert_eq!(parse("null"), Ok(Json::Null));
        assert_eq!(parse(" true "), Ok(Json::Bool(true)));
        assert_eq!(parse("false"), Ok(Json::Bool(false)));
        assert_eq!(parse("0"), Ok(Json::Num(0.0)));
        assert_eq!(parse("-12.5"), Ok(Json::Num(-12.5)));
        assert_eq!(parse("1e3"), Ok(Json::Num(1000.0)));
        assert_eq!(parse("2.5E-2"), Ok(Json::Num(0.025)));
        assert_eq!(parse("-0.0e+0"), Ok(Json::Num(0.0)));
        assert_eq!(parse("78.32000000000001"), Ok(Json::Num(78.32000000000001)));
        assert_eq!(parse("\"hi\""), Ok(Json::Str("hi".into())));
    }

    #[test]
    fn string_escapes() {
        let v = parse(r#""a\"b\\c\/d\b\f\n\r\t\u0041\u00e9\u20AC""#).unwrap();
        assert_eq!(v.as_str(), Some("a\"b\\c/d\u{8}\u{c}\n\r\tA\u{e9}\u{20ac}"));
        // Surrogate pair, and raw multi-byte text passed through.
        let v = parse(r#""\ud83d\ude00 héllo 😀""#).unwrap();
        assert_eq!(v.as_str(), Some("\u{1F600} héllo 😀"));
        // Lone surrogates turn into U+FFFD instead of failing.
        assert_eq!(parse(r#""\ud83dx""#).unwrap().as_str(), Some("\u{FFFD}x"));
        assert_eq!(parse(r#""\ude00""#).unwrap().as_str(), Some("\u{FFFD}"));
        assert_eq!(parse(r#""\ud83d\u0041""#).unwrap().as_str(), Some("\u{FFFD}A"));
    }

    #[test]
    fn nested_and_accessors() {
        let v = parse(
            "{ \"a\" : [1, 2 ,{\"b\":null}],\r\n\t\"s\":\"x\", \"t\":true, \"o\":{}, \"e\":[] }",
        )
        .unwrap();
        assert_eq!(v.as_obj().unwrap().len(), 5);
        assert_eq!(v.as_obj().unwrap()[1].0, "s");
        let a = v.get("a").unwrap();
        assert_eq!(a.as_arr().unwrap().len(), 3);
        assert_eq!(a.at(1).and_then(Json::as_f64), Some(2.0));
        assert_eq!(a.at(2).and_then(|o| o.get("b")), Some(&Json::Null));
        assert!(a.at(3).is_none());
        assert_eq!(v.get("s").and_then(Json::as_str), Some("x"));
        assert_eq!(v.get("t").and_then(Json::as_bool), Some(true));
        assert_eq!(v.get("o"), Some(&Json::Obj(Vec::new())));
        assert_eq!(v.get("e"), Some(&Json::Arr(Vec::new())));
        assert!(v.get("missing").is_none());
        // Accessors of the wrong kind give None.
        assert!(a.get("a").is_none());
        assert!(v.at(0).is_none());
        assert!(v.as_f64().is_none() && v.as_str().is_none() && v.as_bool().is_none());
        assert!(v.as_arr().is_none() && a.as_obj().is_none());
    }

    #[test]
    fn errors_name_the_offset() {
        let e = parse("[1, 2, x]").unwrap_err();
        assert!(e.contains("byte 7"), "{}", e);
        let e = parse("{\"a\":1} 2").unwrap_err();
        assert!(e.contains("byte 8"), "{}", e);
        for bad in [
            "", " ", "[", "]", "{", "[1,]", "[1 2]", "{\"a\"}", "{\"a\":}", "{a:1}", "{\"a\":1,}",
            "01", "-", "1.", ".5", "1e", "1e+", "+1", "nul", "tru", "falsey", "\"abc", "\"\\x\"",
            "\"\\u12g4\"", "\"\\u12", "\"a\nb\"", "\"\\", "'a'", "NaN",
        ] {
            assert!(parse(bad).is_err(), "should reject {:?}", bad);
        }
    }

    #[test]
    fn depth_is_capped() {
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&ok).is_ok());
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert!(parse(&deep).unwrap_err().contains("deep"));
        // Far past the cap must fail cleanly rather than overflow the stack.
        assert!(parse(&"[".repeat(200_000)).is_err());
        assert!(parse(&"{\"a\":".repeat(200_000)).is_err());
    }
}
