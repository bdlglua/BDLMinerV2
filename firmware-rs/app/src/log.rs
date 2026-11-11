//! Log lines go to the console and to a ring buffer shown by the dashboard
//! (/api/log), so the miner can be diagnosed without a cable.

use std::collections::VecDeque;
use std::sync::Mutex;

const RING: usize = 6144;
const LINE: usize = 255;

pub struct Log {
    ring: Mutex<VecDeque<u8>>,
    echo: Box<dyn Fn(&str) + Send + Sync>,
}

impl Log {
    /// `echo` receives every line (serial console, stderr).
    pub fn new(echo: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Log { ring: Mutex::new(VecDeque::with_capacity(RING)), echo: Box::new(echo) }
    }

    /// Adds a line with an "hh:mm:ss " uptime prefix.
    pub fn line(&self, uptime_ms: u32, text: &str) {
        let s = uptime_ms / 1000;
        let mut line = format!("{:02}:{:02}:{:02} ", s / 3600 % 100, s / 60 % 60, s % 60);
        let mut body: String = text.trim_end_matches('\n').chars().collect();
        while line.len() + body.len() + 1 > LINE {
            body.pop();
        }
        (self.echo)(&body);
        line += &body;
        line.push('\n');
        let mut ring = self.ring.lock().unwrap();
        for b in line.bytes() {
            if ring.len() == RING {
                ring.pop_front();
            }
            ring.push_back(b);
        }
    }

    /// The buffered lines, oldest first, starting at a whole line.
    pub fn text(&self) -> String {
        let ring = self.ring.lock().unwrap();
        let bytes: Vec<u8> = ring.iter().copied().collect();
        let start = if ring.len() == RING { bytes.iter().position(|&b| b == b'\n').map_or(0, |p| p + 1) } else { 0 };
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    }
}

/// `logf!(log, clock, "...", args)`.
#[macro_export]
macro_rules! logf {
    ($ctx:expr, $($arg:tt)*) => {
        $ctx.log(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_whole_lines() {
        let log = Log::new(|_| {});
        for i in 0..2000 {
            log.line(3_723_000, &format!("[test] line {i}"));
        }
        let text = log.text();
        assert!(text.len() <= RING);
        assert!(text.starts_with("01:02:03 [test] line "));
        assert!(text.ends_with("[test] line 1999\n"));
        log.line(0, &"x".repeat(1000));
        assert!(log.text().lines().last().unwrap().len() < LINE);
    }
}
