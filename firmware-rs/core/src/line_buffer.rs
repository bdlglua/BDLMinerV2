//! Bounds network input and discards a whole oversized frame before resuming.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Push {
    More,
    Complete,
    Oversized,
}

#[derive(Debug, Clone)]
pub struct LineBuffer {
    buffer: Vec<u8>,
    limit: usize,
    dropping: bool,
}

impl LineBuffer {
    pub fn new(limit: usize) -> Self {
        LineBuffer { buffer: Vec::with_capacity(512.min(limit)), limit, dropping: false }
    }

    pub fn push(&mut self, c: u8) -> Push {
        if c == b'\n' {
            if self.dropping {
                self.dropping = false;
                self.buffer.clear();
                return Push::Oversized;
            }
            return Push::Complete;
        }
        if !self.dropping {
            if c == 0 || self.buffer.len() >= self.limit {
                self.dropping = true;
                self.buffer.clear();
            } else {
                self.buffer.push(c);
            }
        }
        Push::More
    }

    /// The completed line (lossy UTF-8: JSON parsing rejects what is invalid).
    pub fn line(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.buffer)
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(b: &mut LineBuffer, s: &str) -> Vec<Push> {
        s.bytes().map(|c| b.push(c)).filter(|r| *r != Push::More).collect()
    }

    #[test]
    fn drops_oversized_and_resumes() {
        let mut b = LineBuffer::new(8);
        assert_eq!(feed(&mut b, "short\n"), [Push::Complete]);
        assert_eq!(b.line(), "short");
        b.clear();
        assert_eq!(feed(&mut b, "0123456789abc\n"), [Push::Oversized]);
        assert!(b.is_empty());
        assert_eq!(feed(&mut b, "next\n"), [Push::Complete]);
        assert_eq!(b.line(), "next");
        b.clear();
        assert_eq!(feed(&mut b, "a\0b\nok\n"), [Push::Oversized, Push::Complete]);
        assert_eq!(b.line(), "ok");
    }
}
