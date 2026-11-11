//! Stratum V1 protocol state machine. Transport agnostic: feed it received
//! lines, send the lines it returns.

use serde_json::{Map, Value};

use crate::work::{self, Job};

pub use crate::share_reason::RejectReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    /// Send `authorize()` (and `suggest_difficulty()`) now.
    Subscribed,
    /// Refused (e.g. user agent not allowed), see `last_error()`.
    SubscribeFailed,
    Authorized,
    AuthFailed,
    /// `job()` changed; its `clean` flag tells if old work must be dropped.
    NewJob,
    /// `difficulty()` changed; applies to the next NewJob.
    Difficulty,
    ShareAccepted,
    ShareRejected,
    Error,
}

/// A share found by a miner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Share {
    pub job_id: String,
    pub extranonce2: String,
    pub ntime: u32,
    pub nonce: u32,
    /// Difficulty of this hash.
    pub difficulty: f64,
    pub version: u32,
    pub version_mask: u32,
    /// Exact SV2 target recheck if the pool changes target before submission.
    pub hash: [u8; 32],
    /// Target assigned to the work, distinct from this hash's difficulty.
    pub pool_difficulty: f64,
}

/// Longest job id kept (and so the longest a pool may send).
pub const MAX_JOB_ID: usize = 63;

/// A share submission waiting for (or matched with) the pool's answer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShareAck {
    pub id: u32,
    /// Assigned target when submitted.
    pub difficulty: f64,
    pub reason: RejectReason,
}

impl ShareAck {
    pub fn new(id: u32, difficulty: f64) -> Self {
        ShareAck { id, difficulty, reason: RejectReason::Unknown }
    }
}

/// Bits a version-rolling pool may grant (BIP320).
pub const VERSION_ROLLING_MASK: u32 = 0x1fff_e000;
const ID_SUBSCRIBE: u32 = 1;
const ID_AUTHORIZE: u32 = 2;
const ID_SUGGEST: u32 = 3;
const ID_CONFIGURE: u32 = 4;
const FIRST_SUBMIT_ID: u32 = 10;
pub const MAX_PENDING_SHARES: usize = 32;

fn json_string(s: &str) -> String {
    Value::String(s.to_owned()).to_string()
}

fn json_number(d: f64) -> String {
    serde_json::Number::from_f64(d).map(|n| n.to_string()).unwrap_or_else(|| "null".into())
}

fn parse_hex32(value: &Value) -> Option<u32> {
    work::from_hex_array::<4>(value.as_str()?).map(u32::from_be_bytes)
}

/// extranonce2 sizes that can be serialized (up to 16 bytes).
fn valid_extranonce2_size(n: i64) -> bool {
    (1..=16).contains(&n)
}

fn is_uint32(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn as_int(v: &Value) -> Option<i64> {
    v.as_i64()
}

#[derive(Debug, Clone)]
pub struct Protocol {
    user: String,
    password: String,
    job: Job,
    // Session updates apply to the next notify, not to work already issued.
    extranonce1: Vec<u8>,
    extranonce2_size: usize,
    difficulty: f64,
    has_job: bool,
    authorized: bool,
    version_mask: u32,
    configure_requested: bool,
    next_id: u32,
    pending: Vec<ShareAck>,
    acknowledgment: ShareAck,
    submission_id: u32,
    last_error: String,
}

impl Default for Protocol {
    fn default() -> Self {
        Protocol {
            user: String::new(),
            password: String::new(),
            job: Job::default(),
            extranonce1: Vec::new(),
            extranonce2_size: 4,
            difficulty: 1.0,
            has_job: false,
            authorized: false,
            version_mask: 0,
            configure_requested: false,
            next_id: FIRST_SUBMIT_ID,
            pending: Vec::with_capacity(MAX_PENDING_SHARES),
            acknowledgment: ShareAck::default(),
            submission_id: 0,
            last_error: String::new(),
        }
    }
}

impl Protocol {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the session; the credentials stay.
    pub fn reset(&mut self) {
        let (user, password) = (std::mem::take(&mut self.user), std::mem::take(&mut self.password));
        *self = Protocol { user, password, ..Default::default() };
    }

    pub fn set_credentials(&mut self, user: &str, password: &str) {
        self.user = user.to_owned();
        self.password = password.to_owned();
    }

    pub fn configure_version_rolling(&mut self) -> String {
        self.configure_requested = true;
        format!(
            "{{\"id\":{ID_CONFIGURE},\"method\":\"mining.configure\",\"params\":[[\"version-rolling\"],{{\"version-rolling.mask\":\"1fffe000\",\"version-rolling.min-bit-count\":8}}]}}\n"
        )
    }

    pub fn subscribe(&self, agent: &str) -> String {
        format!("{{\"id\":{ID_SUBSCRIBE},\"method\":\"mining.subscribe\",\"params\":[{}]}}\n", json_string(agent))
    }

    pub fn authorize(&self) -> String {
        format!(
            "{{\"id\":{ID_AUTHORIZE},\"method\":\"mining.authorize\",\"params\":[{},{}]}}\n",
            json_string(&self.user),
            json_string(&self.password)
        )
    }

    pub fn suggest_difficulty(&self, diff: f64) -> String {
        format!("{{\"id\":{ID_SUGGEST},\"method\":\"mining.suggest_difficulty\",\"params\":[{}]}}\n", json_number(diff))
    }

    pub fn pending_shares(&self) -> usize {
        self.pending.len()
    }
    pub fn can_submit(&self) -> bool {
        self.pending.len() < MAX_PENDING_SHARES
    }
    pub fn submission_id(&self) -> u32 {
        self.submission_id
    }
    pub fn acknowledgment(&self) -> &ShareAck {
        &self.acknowledgment
    }
    pub fn job(&self) -> &Job {
        &self.job
    }
    pub fn has_job(&self) -> bool {
        self.has_job
    }
    pub fn authorized(&self) -> bool {
        self.authorized
    }
    pub fn difficulty(&self) -> f64 {
        self.difficulty
    }
    pub fn last_error(&self) -> &str {
        &self.last_error
    }

    /// The `mining.submit` line, or `None` when the share cannot be sent
    /// (too many pending, or found with another version mask).
    pub fn submit(&mut self, share: &Share) -> Option<String> {
        if !self.can_submit() || share.version_mask != self.version_mask {
            return None;
        }
        let id = loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if self.next_id < FIRST_SUBMIT_ID {
                self.next_id = FIRST_SUBMIT_ID;
            }
            if !self.pending.iter().any(|p| p.id == id) {
                break id;
            }
        };
        let mut line = format!(
            "{{\"id\":{id},\"method\":\"mining.submit\",\"params\":[{},{},{},\"{:08x}\",\"{:08x}\"",
            json_string(&self.user),
            json_string(&share.job_id),
            json_string(&share.extranonce2),
            share.ntime,
            share.nonce
        );
        if self.version_mask != 0 {
            line += &format!(",\"{:08x}\"", share.version & self.version_mask);
        }
        line += "]}\n";
        let assigned = if share.pool_difficulty > 0.0 { share.pool_difficulty } else { self.job.difficulty };
        self.pending.push(ShareAck::new(id, assigned));
        self.submission_id = id;
        Some(line)
    }

    fn invalid(&mut self, message: &str) -> Event {
        self.last_error = message.to_owned();
        Event::Error
    }

    pub fn handle_line(&mut self, line: &str) -> Event {
        let doc: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return self.invalid("bad json"),
        };
        let Some(obj) = doc.as_object() else { return Event::None };
        if let Some(method) = obj.get("method").and_then(Value::as_str) {
            let empty = Vec::new();
            let params = obj.get("params").and_then(Value::as_array).unwrap_or(&empty);
            return self.handle_method(method, params);
        }
        self.handle_response(obj)
    }

    fn handle_method(&mut self, method: &str, params: &[Value]) -> Event {
        let param = |i: usize| params.get(i).unwrap_or(&Value::Null);
        match method {
            "mining.notify" => self.handle_notify(params),
            "mining.set_version_mask" => {
                let mask = parse_hex32(param(0));
                match mask {
                    Some(mask)
                        if self.configure_requested && self.version_mask != 0 && mask & !VERSION_ROLLING_MASK == 0 =>
                    {
                        self.version_mask = mask;
                        if self.has_job {
                            self.job.version_mask = mask;
                            self.job.clean = true;
                            return Event::NewJob;
                        }
                        Event::None
                    }
                    _ => self.invalid("invalid version rolling mask"),
                }
            }
            "mining.set_difficulty" => match param(0).as_f64() {
                Some(d) if d.is_finite() && d > 0.0 => {
                    self.difficulty = d;
                    if !self.has_job {
                        self.job.difficulty = d;
                    }
                    Event::Difficulty
                }
                _ => self.invalid("invalid difficulty"),
            },
            "mining.set_extranonce" => {
                let en1 = param(0).as_str().and_then(work::from_hex);
                let size = as_int(param(1)).filter(|&n| param(1).is_i64() && valid_extranonce2_size(n));
                match (en1, size) {
                    (Some(en1), Some(size)) => {
                        self.extranonce1 = en1;
                        self.extranonce2_size = size as usize;
                        if !self.has_job {
                            self.job.extranonce1 = self.extranonce1.clone();
                            self.job.extranonce2_size = self.extranonce2_size;
                        }
                        Event::None
                    }
                    _ => self.invalid("invalid extranonce parameters"),
                }
            }
            _ => Event::None, // client.reconnect, client.show_message...
        }
    }

    fn handle_notify(&mut self, params: &[Value]) -> Event {
        let id = params.first().and_then(Value::as_str);
        let id_ok = matches!(id, Some(s) if !s.is_empty() && s.len() <= MAX_JOB_ID);
        if params.len() < 9
            || !id_ok
            || !params[1].is_string()
            || !params[2].is_string()
            || !params[3].is_string()
            || !params[4].is_array()
            || !params[8].is_boolean()
        {
            return self.invalid("invalid job parameters");
        }
        let mut j = Job { id: id.unwrap_or_default().to_owned(), ..Default::default() };
        // prevhash: eight 32-bit words, each byte-swapped vs header order.
        let Some(prev) = work::from_hex_array::<32>(params[1].as_str().unwrap_or_default()) else {
            return self.invalid("invalid previous block hash");
        };
        for w in 0..8 {
            for b in 0..4 {
                j.prev_hash[4 * w + b] = prev[4 * w + 3 - b];
            }
        }
        match (
            work::from_hex(params[2].as_str().unwrap_or_default()),
            work::from_hex(params[3].as_str().unwrap_or_default()),
        ) {
            (Some(c1), Some(c2)) => {
                j.coinb1 = c1;
                j.coinb2 = c2;
            }
            _ => return self.invalid("invalid coinbase hex"),
        }
        let branches = params[4].as_array().map(Vec::as_slice).unwrap_or_default();
        if branches.len() > 32 {
            return self.invalid("too many merkle branches");
        }
        for br in branches {
            match br.as_str().and_then(work::from_hex_array::<32>) {
                Some(b) => j.branches.push(b),
                None => return self.invalid("invalid merkle branch"),
            }
        }
        match (parse_hex32(&params[5]), parse_hex32(&params[6]), parse_hex32(&params[7])) {
            (Some(v), Some(b), Some(t)) => {
                j.version = v;
                j.nbits = b;
                j.ntime = t;
            }
            _ => return self.invalid("invalid block header hex"),
        }
        if work::target_from_bits(j.nbits).is_none() {
            return self.invalid("invalid network target");
        }
        j.clean = params[8].as_bool().unwrap_or(false);
        j.version_mask = self.version_mask;
        j.extranonce1 = self.extranonce1.clone();
        j.extranonce2_size = self.extranonce2_size;
        j.difficulty = self.difficulty;
        self.job = j;
        self.has_job = true;
        Event::NewJob
    }

    fn handle_response(&mut self, obj: &Map<String, Value>) -> Event {
        // Only unsigned integer ids are ours.
        let Some(id) = obj.get("id").and_then(is_uint32) else { return Event::None };
        let mut pending_index = None;
        if id >= FIRST_SUBMIT_ID {
            pending_index = self.pending.iter().position(|p| p.id == id);
            if pending_index.is_none() {
                return Event::None;
            }
        }
        let null = Value::Null;
        let result = obj.get("result").unwrap_or(&null);
        let error = obj.get("error").unwrap_or(&null);
        let mut ok = !result.is_null() && result != &Value::Bool(false) && error.is_null();
        if !error.is_null() {
            self.last_error.clear();
            if let Some(arr) = error.as_array() {
                self.last_error = arr.get(1).and_then(Value::as_str).unwrap_or("error").to_owned();
                // [code, message, detail]: public-pool names the refused value
                // in the detail (", <address>"). Kept, bounded.
                let detail = arr.get(2).and_then(Value::as_str).unwrap_or("");
                let detail = detail.trim_start_matches([',', ' ']);
                if !detail.is_empty() {
                    let mut text: String = detail.chars().take(96).collect();
                    if text.chars().count() == 96 {
                        text += "...";
                    }
                    self.last_error += &format!(" ({text})");
                }
            } else {
                self.last_error = error.to_string();
            }
        }

        match id {
            ID_CONFIGURE => {
                if !self.configure_requested {
                    return Event::None;
                }
                let rolling = ok && result.get("version-rolling").and_then(Value::as_bool) == Some(true);
                let mut mask = 0;
                if rolling {
                    match result.get("version-rolling.mask").and_then(parse_hex32) {
                        Some(m) if m & !VERSION_ROLLING_MASK == 0 => mask = m,
                        _ => return self.invalid("invalid configured version mask"),
                    }
                }
                self.version_mask = mask;
                if self.has_job {
                    self.job.version_mask = mask;
                    self.job.clean = true;
                    return Event::NewJob;
                }
                Event::None
            }
            ID_SUBSCRIBE => {
                // [[subscriptions...], extranonce1, extranonce2_size]. Some
                // pools put a refusal message in "result" with a null error.
                let Some(arr) = result.as_array().filter(|_| ok) else {
                    if let Some(s) = result.as_str() {
                        self.last_error = s.to_owned();
                    } else if self.last_error.is_empty() {
                        self.last_error = "subscribe refused".into();
                    }
                    return Event::SubscribeFailed;
                };
                let en1 = arr.get(1).and_then(Value::as_str).and_then(work::from_hex);
                let size = arr.get(2).filter(|v| v.is_i64()).and_then(as_int);
                let (Some(en1), Some(size), true) = (en1, size, arr.first().is_some_and(Value::is_array)) else {
                    self.last_error = "invalid subscription parameters".into();
                    return Event::SubscribeFailed;
                };
                if !valid_extranonce2_size(size) {
                    self.last_error = "unsupported extranonce2 size".into();
                    return Event::SubscribeFailed;
                }
                self.extranonce1 = en1;
                self.extranonce2_size = size as usize;
                if !self.has_job {
                    self.job.extranonce1 = self.extranonce1.clone();
                    self.job.extranonce2_size = self.extranonce2_size;
                }
                Event::Subscribed
            }
            ID_AUTHORIZE => {
                ok = ok && result.as_bool() == Some(true);
                if ok && self.authorized {
                    return Event::None; // duplicate replies do not restart mining work
                }
                if !ok && error.is_null() {
                    self.last_error =
                        if result.is_boolean() { "authorization refused" } else { "invalid authorization response" }
                            .into();
                }
                self.authorized = ok;
                if ok {
                    Event::Authorized
                } else {
                    Event::AuthFailed
                }
            }
            ID_SUGGEST => Event::None,
            _ => {
                let Some(index) = pending_index else { return Event::None };
                if !result.is_boolean() && error.is_null() {
                    return self.invalid("invalid share response");
                }
                self.acknowledgment = self.pending[index];
                // A bare `false` carries no reason: never classify it with the
                // message of an earlier, unrelated error.
                if !ok && error.is_null() {
                    self.last_error = "share rejected".into();
                }
                if !ok {
                    let code = error.as_array().and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
                    self.acknowledgment.reason = RejectReason::classify(code, &self.last_error);
                }
                self.pending.swap_remove(index);
                if ok {
                    Event::ShareAccepted
                } else {
                    Event::ShareRejected
                }
            }
        }
    }
}
