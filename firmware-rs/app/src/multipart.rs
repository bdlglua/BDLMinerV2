//! A multipart/form-data body read as it arrives (POST /update): the file
//! is far larger than the free memory, so its bytes are handed on chunk by
//! chunk, never gathered.

/// What the parser found.
#[derive(Debug, PartialEq)]
pub enum Event<'a> {
    /// A part begins; `filename` is empty for a plain form field.
    Start {
        filename: String,
    },
    Data(&'a [u8]),
    /// The part ended (its closing delimiter was read).
    End,
}

#[derive(Debug, PartialEq)]
enum State {
    Preamble,
    Headers,
    Body,
    AfterDelimiter,
    Done,
}

pub struct Parser {
    /// "\r\n--<boundary>"
    delimiter: Vec<u8>,
    buf: Vec<u8>,
    state: State,
}

const MAX_HEADERS: usize = 2048;

/// The boundary of a `multipart/form-data; boundary=...` content type.
pub fn boundary(content_type: &str) -> Option<&str> {
    let (kind, params) = content_type.split_once(';')?;
    if !kind.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    params.split(';').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        let v = v.trim().trim_matches('"');
        (k.trim().eq_ignore_ascii_case("boundary") && !v.is_empty() && v.len() <= 70).then_some(v)
    })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn filename(headers: &[u8]) -> String {
    let text = String::from_utf8_lossy(headers);
    for line in text.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else { continue };
        if !name.trim().eq_ignore_ascii_case("content-disposition") {
            continue;
        }
        for param in value.split(';') {
            if let Some((k, v)) = param.split_once('=') {
                if k.trim().eq_ignore_ascii_case("filename") {
                    return v.trim().trim_matches('"').to_owned();
                }
            }
        }
    }
    String::new()
}

impl Parser {
    pub fn new(content_type: &str) -> Option<Self> {
        let b = boundary(content_type)?;
        // The body starts with "--<boundary>": seen as if a line break came
        // before it, every delimiter is the same "\r\n--<boundary>".
        Some(Parser { delimiter: format!("\r\n--{b}").into_bytes(), buf: b"\r\n".to_vec(), state: State::Preamble })
    }

    /// True once the closing delimiter was read.
    pub fn finished(&self) -> bool {
        self.state == State::Done
    }

    pub fn feed(&mut self, data: &[u8], on: &mut dyn FnMut(Event) -> Result<(), String>) -> Result<(), String> {
        if self.state == State::Done {
            return Ok(());
        }
        self.buf.extend_from_slice(data);
        loop {
            match self.state {
                State::Preamble => match find(&self.buf, &self.delimiter) {
                    Some(at) => {
                        self.buf.drain(..at + self.delimiter.len());
                        self.state = State::AfterDelimiter;
                    }
                    None => {
                        let keep = self.buf.len().saturating_sub(self.delimiter.len());
                        self.buf.drain(..keep);
                        return Ok(());
                    }
                },
                State::AfterDelimiter => {
                    if self.buf.len() < 2 {
                        return Ok(());
                    }
                    if &self.buf[..2] == b"--" {
                        self.state = State::Done;
                        self.buf.clear();
                        return Ok(());
                    }
                    if &self.buf[..2] != b"\r\n" {
                        return Err("malformed multipart body".into());
                    }
                    self.buf.drain(..2);
                    self.state = State::Headers;
                }
                State::Headers => match find(&self.buf, b"\r\n\r\n") {
                    Some(at) => {
                        let filename = filename(&self.buf[..at]);
                        self.buf.drain(..at + 4);
                        self.state = State::Body;
                        on(Event::Start { filename })?;
                    }
                    None if self.buf.len() > MAX_HEADERS => return Err("multipart headers too long".into()),
                    None => return Ok(()),
                },
                State::Body => match find(&self.buf, &self.delimiter) {
                    Some(at) => {
                        if at > 0 {
                            on(Event::Data(&self.buf[..at]))?;
                        }
                        self.buf.drain(..at + self.delimiter.len());
                        self.state = State::AfterDelimiter;
                        on(Event::End)?;
                    }
                    None => {
                        // Keep what could be the start of a delimiter.
                        let safe = self.buf.len().saturating_sub(self.delimiter.len() - 1);
                        if safe > 0 {
                            on(Event::Data(&self.buf[..safe]))?;
                            self.buf.drain(..safe);
                        }
                        return Ok(());
                    }
                },
                State::Done => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &[u8], chunk: usize) -> (Vec<String>, Vec<Vec<u8>>, bool) {
        let mut p = Parser::new("multipart/form-data; boundary=----WebKitFormBoundaryX").unwrap();
        let (mut names, mut parts) = (Vec::new(), Vec::new());
        for c in body.chunks(chunk) {
            p.feed(c, &mut |e| {
                match e {
                    Event::Start { filename } => {
                        names.push(filename);
                        parts.push(Vec::new());
                    }
                    Event::Data(d) => parts.last_mut().unwrap().extend_from_slice(d),
                    Event::End => {}
                }
                Ok(())
            })
            .unwrap();
        }
        (names, parts, p.finished())
    }

    #[test]
    fn streams_a_file() {
        let file: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).chain(*b"\r\n--x\r\n").collect();
        let mut body = b"------WebKitFormBoundaryX\r\nContent-Disposition: form-data; name=\"update\"; filename=\"m5stickc-plus2-rust-firmware.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
        body.extend_from_slice(&file);
        body.extend_from_slice(b"\r\n------WebKitFormBoundaryX--\r\n");
        for chunk in [1, 3, 17, 512, body.len()] {
            let (names, parts, done) = parse(&body, chunk);
            assert_eq!(names, ["m5stickc-plus2-rust-firmware.bin"]);
            assert_eq!(parts[0], file);
            assert!(done);
        }
    }

    #[test]
    fn boundaries() {
        assert_eq!(boundary("multipart/form-data; boundary=\"abc\""), Some("abc"));
        assert_eq!(boundary("application/json"), None);
        let (names, _, done) = parse(b"------WebKitFormBoundaryX\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n------WebKitFormBoundaryX--", 4);
        assert_eq!(names, [""]);
        assert!(done);
    }
}
