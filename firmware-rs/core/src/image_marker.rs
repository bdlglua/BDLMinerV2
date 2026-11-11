//! Finds a text marker in a firmware image as it is downloaded, chunk by
//! chunk (Knuth-Morris-Pratt: a marker cut by a chunk boundary is found).
//! Every release image holds its board ("<env>\0") and its version
//! ("BDLMinerV1/<version>\0"); the updater requires both.

#[derive(Clone, Debug, Default)]
pub struct Scanner {
    pattern: Vec<u8>,
    fallback: Vec<usize>,
    matched: usize,
    found: bool,
}

impl Scanner {
    /// The marker is `prefix` + `text` + NUL. False when empty or too long.
    pub fn set(&mut self, prefix: &str, text: &str) -> bool {
        *self = Scanner::default();
        if text.is_empty() || prefix.len() + text.len() + 1 > 96 {
            return false;
        }
        self.pattern = [prefix.as_bytes(), text.as_bytes(), &[0]].concat();
        self.fallback = vec![0; self.pattern.len()];
        let mut k = 0;
        for i in 1..self.pattern.len() {
            while k > 0 && self.pattern[i] != self.pattern[k] {
                k = self.fallback[k - 1];
            }
            if self.pattern[i] == self.pattern[k] {
                k += 1;
            }
            self.fallback[i] = k;
        }
        true
    }

    pub fn feed(&mut self, data: &[u8]) {
        if self.pattern.is_empty() || self.found {
            return;
        }
        for &b in data {
            while self.matched > 0 && b != self.pattern[self.matched] {
                self.matched = self.fallback[self.matched - 1];
            }
            if b == self.pattern[self.matched] {
                self.matched += 1;
                if self.matched == self.pattern.len() {
                    self.found = true;
                    return;
                }
            }
        }
    }

    pub fn found(&self) -> bool {
        self.found
    }
}
