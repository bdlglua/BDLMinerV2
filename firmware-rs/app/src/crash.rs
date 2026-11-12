//! Crash reports, as the C++ firmware's crashlog.cpp: ESP-IDF writes a core
//! dump to flash when the firmware crashes (a Rust panic included); the next
//! boot finds it, keeps a one-line summary and offers the dump at
//! /api/coredump. The last crashes stay listed ("time|version|summary" lines
//! in NVS) after the dump is replaced or erased.

use std::sync::Mutex;

use serde_json::{json, Value};

use crate::app::App;
use crate::platform::FW_VERSION;

const HISTORY: usize = 5;
const NS: &str = "crash";

/// The dump in flash (ESP-IDF's core dump partition).
pub trait CoreDump: Send {
    fn size(&self) -> usize;
    fn read(&self, offset: usize, out: &mut [u8]) -> bool;
    fn erase(&mut self) -> bool;
}

#[derive(Default)]
struct State {
    summary: String,
    history: String,
    time_pending: bool,
    dump: Option<Box<dyn CoreDump>>,
}

#[derive(Default)]
pub struct Crashes {
    state: Mutex<State>,
}

/// FNV-1a, the C++ firmware's report fingerprint.
pub fn fnv(data: &[u8], mut h: u32) -> u32 {
    for &b in data {
        h = (h ^ b as u32).wrapping_mul(16_777_619);
    }
    h
}

pub const FNV_START: u32 = 2_166_136_261;

impl Crashes {
    fn save_history(app: &App, history: &str) {
        let mut store = app.storage.lock().unwrap();
        if history.is_empty() {
            store.remove(NS, "hist");
        } else {
            store.set(NS, "hist", history.as_bytes());
        }
    }

    /// At boot: the report found (`fingerprint` 0 and no dump when none).
    /// The crash happened in the previous run, so its version is the one
    /// that run had.
    pub fn begin(&self, app: &App, fingerprint: u32, summary: &str, dump: Option<Box<dyn CoreDump>>) {
        let text = |key: &str| {
            app.storage
                .lock()
                .unwrap()
                .get(NS, key)
                .map(|v| String::from_utf8_lossy(&v).into_owned())
                .unwrap_or_default()
        };
        let previous = text("ver");
        let mut history = text("hist");
        let saved_fp = app
            .storage
            .lock()
            .unwrap()
            .get(NS, "fp")
            .and_then(|v| v.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        app.storage.lock().unwrap().set(NS, "ver", FW_VERSION.as_bytes());
        let fresh = fingerprint != 0 && saved_fp != fingerprint;
        let mut state = self.state.lock().unwrap();
        if fresh {
            app.storage.lock().unwrap().set(NS, "fp", &fingerprint.to_le_bytes());
            let line =
                format!("0|{}|{}", if previous.is_empty() { "?" } else { &previous }, summary.replace('\n', " "));
            history = if history.is_empty() { line } else { format!("{line}\n{history}") };
            history = history.lines().take(HISTORY).collect::<Vec<_>>().join("\n");
            Self::save_history(app, &history);
            state.time_pending = true;
        }
        state.history = history;
        state.summary = summary.to_owned();
        if let Some(d) = &dump {
            app.log(&format!("[crash] last crash: {summary}; report {} bytes at /api/coredump", d.size()));
        }
        state.dump = dump;
    }

    pub fn present(&self) -> bool {
        self.state.lock().unwrap().dump.is_some()
    }

    pub fn size(&self) -> usize {
        self.state.lock().unwrap().dump.as_ref().map_or(0, |d| d.size())
    }

    pub fn read(&self, offset: usize, out: &mut [u8]) -> bool {
        let state = self.state.lock().unwrap();
        match &state.dump {
            Some(d) if offset <= d.size() && out.len() <= d.size() - offset => d.read(offset, out),
            _ => false,
        }
    }

    pub fn erase(&self, app: &App) -> bool {
        let mut state = self.state.lock().unwrap();
        if let Some(d) = state.dump.as_mut() {
            if !d.erase() {
                return false;
            }
        }
        state.dump = None;
        state.summary.clear();
        state.history.clear();
        state.time_pending = false;
        Self::save_history(app, "");
        drop(state);
        app.log("[crash] report and history erased");
        true
    }

    /// Dates the report found at boot once the clock is set.
    pub fn poll(&self, app: &App) {
        let mut state = self.state.lock().unwrap();
        if !state.time_pending {
            return;
        }
        let Some(now) = app.clock.unix_time() else { return };
        state.time_pending = false;
        // Found at boot: the crash happened just before this run started.
        let stamp = now.saturating_sub(app.clock.millis() / 1000);
        if let Some(rest) = state.history.strip_prefix("0|") {
            state.history = format!("{stamp}|{rest}");
            Self::save_history(app, &state.history);
        }
    }

    /// The dashboard's "crash" object, None without report or history.
    pub fn json(&self) -> Option<Value> {
        let state = self.state.lock().unwrap();
        if state.dump.is_none() && state.history.is_empty() {
            return None;
        }
        let history: Vec<Value> = state
            .history
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '|');
                let t: u32 = parts.next()?.parse().ok()?;
                let version = parts.next()?;
                let summary = parts.next()?;
                Some(json!({"t": t, "version": version, "summary": summary}))
            })
            .collect();
        let mut j = json!({"history": history});
        if let Some(d) = &state.dump {
            j["summary"] = state.summary.clone().into();
            j["size"] = d.size().into();
        }
        Some(j)
    }
}
