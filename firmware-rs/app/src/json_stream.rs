//! A JSON body read as it arrives, value by value: each scalar is handed on
//! with its path ("fastestFee", "[0].extras.pool.name"), so that a large
//! answer (mempool.space's block list, ~30 KB) is never held in memory.

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Scalar<'a> {
    Str(&'a str),
    Num(f64),
    Bool(bool),
    Null,
}

impl Scalar<'_> {
    /// A number, or a number written as text (some pool APIs do that).
    pub fn number(&self) -> Option<f64> {
        match *self {
            Scalar::Num(n) => Some(n),
            Scalar::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
}

enum Frame {
    Object(Option<String>),
    Array(usize),
}

#[derive(PartialEq, Clone, Copy)]
enum Expect {
    Value,
    ValueOrEnd,
    Key,
    KeyOrEnd,
    Colon,
    CommaOrEnd,
    Done,
}

pub struct Parser {
    stack: Vec<Frame>,
    expect: Expect,
    in_string: bool,
    escape: u8,
    unicode: u32,
    text: Vec<u8>,
    literal: String,
}

const MAX_DEPTH: usize = 24;
const MAX_TEXT: usize = 512;

impl Default for Parser {
    fn default() -> Self {
        Parser {
            stack: Vec::new(),
            expect: Expect::Value,
            in_string: false,
            escape: 0,
            unicode: 0,
            text: Vec::new(),
            literal: String::new(),
        }
    }
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once the top-level value is complete.
    pub fn done(&self) -> bool {
        self.expect == Expect::Done
    }

    fn path(&self) -> String {
        let mut p = String::new();
        for frame in &self.stack {
            match frame {
                Frame::Object(Some(k)) => {
                    if !p.is_empty() {
                        p.push('.');
                    }
                    p.push_str(k);
                }
                Frame::Object(None) => {}
                Frame::Array(i) => {
                    p.push('[');
                    p.push_str(&i.to_string());
                    p.push(']');
                }
            }
        }
        p
    }

    fn after_value(&mut self) {
        self.expect = if self.stack.is_empty() { Expect::Done } else { Expect::CommaOrEnd };
    }

    fn scalar(&mut self, value: Scalar, on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
        if !matches!(self.expect, Expect::Value | Expect::ValueOrEnd) {
            return Err("unexpected value".into());
        }
        on(&self.path(), value);
        self.after_value();
        Ok(())
    }

    fn end_literal(&mut self, on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
        if self.literal.is_empty() {
            return Ok(());
        }
        let literal = std::mem::take(&mut self.literal);
        let value = match literal.as_str() {
            "true" => Scalar::Bool(true),
            "false" => Scalar::Bool(false),
            "null" => Scalar::Null,
            n => Scalar::Num(n.parse().map_err(|_| format!("invalid JSON value {n}"))?),
        };
        self.scalar(value, on)
    }

    pub fn feed(&mut self, data: &[u8], on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
        for &b in data {
            if self.in_string {
                self.string_byte(b, on)?;
                continue;
            }
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.') {
                if !matches!(self.expect, Expect::Value | Expect::ValueOrEnd) {
                    return Err("unexpected value".into());
                }
                if self.literal.len() >= 64 {
                    return Err("JSON value too long".into());
                }
                self.literal.push(b as char);
                continue;
            }
            self.end_literal(on)?;
            match b {
                b' ' | b'\t' | b'\r' | b'\n' => {}
                b'"' => {
                    if !matches!(self.expect, Expect::Value | Expect::ValueOrEnd | Expect::Key | Expect::KeyOrEnd) {
                        return Err("unexpected string".into());
                    }
                    self.in_string = true;
                    self.text.clear();
                }
                b'{' | b'[' => {
                    if !matches!(self.expect, Expect::Value | Expect::ValueOrEnd) {
                        return Err("unexpected container".into());
                    }
                    if self.stack.len() >= MAX_DEPTH {
                        return Err("JSON nested too deeply".into());
                    }
                    if b == b'{' {
                        self.stack.push(Frame::Object(None));
                        self.expect = Expect::KeyOrEnd;
                    } else {
                        self.stack.push(Frame::Array(0));
                        self.expect = Expect::ValueOrEnd;
                    }
                }
                b'}' | b']' => {
                    let ok = match (self.stack.last(), b) {
                        (Some(Frame::Object(_)), b'}') => matches!(self.expect, Expect::KeyOrEnd | Expect::CommaOrEnd),
                        (Some(Frame::Array(_)), b']') => {
                            matches!(self.expect, Expect::ValueOrEnd | Expect::CommaOrEnd)
                        }
                        _ => false,
                    };
                    if !ok {
                        return Err("unexpected end of container".into());
                    }
                    self.stack.pop();
                    self.after_value();
                }
                b':' => {
                    if self.expect != Expect::Colon {
                        return Err("unexpected ':'".into());
                    }
                    self.expect = Expect::Value;
                }
                b',' => {
                    if self.expect != Expect::CommaOrEnd {
                        return Err("unexpected ','".into());
                    }
                    match self.stack.last_mut() {
                        Some(Frame::Array(i)) => {
                            *i += 1;
                            self.expect = Expect::Value;
                        }
                        Some(Frame::Object(_)) => self.expect = Expect::Key,
                        None => return Err("unexpected ','".into()),
                    }
                }
                _ => return Err(format!("unexpected byte {b:#04x}")),
            }
        }
        Ok(())
    }

    /// Ends the body: a top-level number has no delimiter after it.
    pub fn finish(&mut self, on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
        self.end_literal(on)?;
        if self.done() {
            Ok(())
        } else {
            Err("incomplete JSON".into())
        }
    }

    fn string_byte(&mut self, b: u8, on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
        let push = |text: &mut Vec<u8>, c: char| {
            if text.len() < MAX_TEXT {
                text.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
        };
        match self.escape {
            0 => match b {
                b'\\' => self.escape = 1,
                b'"' => {
                    self.in_string = false;
                    let text = String::from_utf8_lossy(&std::mem::take(&mut self.text)).into_owned();
                    if matches!(self.expect, Expect::Key | Expect::KeyOrEnd) {
                        if let Some(Frame::Object(key)) = self.stack.last_mut() {
                            *key = Some(text);
                        }
                        self.expect = Expect::Colon;
                    } else {
                        self.scalar(Scalar::Str(&text), on)?;
                    }
                }
                // UTF-8 bytes are kept as they come, decoded with the string.
                _ => {
                    if self.text.len() < MAX_TEXT {
                        self.text.push(b);
                    }
                }
            },
            1 => {
                self.escape = 0;
                let c = match b {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'u' => {
                        self.escape = 2;
                        self.unicode = 0;
                        return Ok(());
                    }
                    other => other as char,
                };
                push(&mut self.text, c);
            }
            n => {
                let digit = (b as char).to_digit(16).ok_or("invalid \\u escape")?;
                self.unicode = self.unicode << 4 | digit;
                if n == 5 {
                    self.escape = 0;
                    push(&mut self.text, char::from_u32(self.unicode).unwrap_or('\u{fffd}'));
                } else {
                    self.escape = n + 1;
                }
            }
        }
        Ok(())
    }
}

/// Parses `data` whole (tests, small answers).
pub fn parse_all(data: &[u8], on: &mut dyn FnMut(&str, Scalar)) -> Result<(), String> {
    let mut p = Parser::new();
    p.feed(data, on)?;
    p.finish(on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(data: &[u8], chunk: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut p = Parser::new();
        let mut on = |path: &str, v: Scalar| out.push((path.to_owned(), format!("{v:?}")));
        for c in data.chunks(chunk) {
            p.feed(c, &mut on).unwrap();
        }
        p.finish(&mut on).unwrap();
        out
    }

    #[test]
    fn paths_and_values() {
        let body = br#"[{"height":900001,"timestamp":1750000000,"tx_count":3120,"extras":{"pool":{"id":1,"name":"Foundry \"USA\" \u00e9"},"fees":[1,2]}},{"height":900000,"extras":{"pool":{"name":"AntPool"}},"ok":true,"none":null}]"#;
        for chunk in [1, 5, body.len()] {
            let v = collect(body, chunk);
            assert!(v.contains(&("[0].height".into(), "Num(900001.0)".into())));
            assert!(v.contains(&("[0].extras.pool.name".into(), "Str(\"Foundry \\\"USA\\\" é\")".into())));
            assert!(v.contains(&("[0].extras.fees[1]".into(), "Num(2.0)".into())));
            assert!(v.contains(&("[1].extras.pool.name".into(), "Str(\"AntPool\")".into())));
            assert!(v.contains(&("[1].ok".into(), "Bool(true)".into())));
            assert!(v.contains(&("[1].none".into(), "Null".into())));
        }
        assert_eq!(collect(b"{\"fastestFee\":12,\"hourFee\":-1.5e1}", 3)[1], ("hourFee".into(), "Num(-15.0)".into()));
        assert_eq!(collect(b"42", 1), [("".to_string(), "Num(42.0)".to_string())]);
    }

    #[test]
    fn refuses_broken_json() {
        let mut on = |_: &str, _: Scalar| {};
        assert!(parse_all(b"{\"a\" 1}", &mut on).is_err());
        assert!(parse_all(b"[1,,2]", &mut on).is_err());
        assert!(parse_all(b"[1", &mut on).is_err());
        assert!(parse_all(b"<html>", &mut on).is_err());
    }
}
