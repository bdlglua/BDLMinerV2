//! Bounded, transport-independent Stratum V2 extended-channel mining protocol.

use crate::stratum::{RejectReason, Share, ShareAck};
use crate::work::{self, Job};

pub const MAX_PAYLOAD: usize = 16384;
const MAX_PENDING: usize = 32;
const JOB_SLOTS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub extension: u16,
    pub kind: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(extension: u16, kind: u8, payload: Vec<u8>) -> Self {
        Frame { extension, kind, payload }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    Setup,
    Opened,
    Job,
    Target,
    Accepted,
    Rejected,
    Error,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    ok: bool,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, p: 0, ok: true }
    }
    fn number(&mut self, n: usize) -> u32 {
        if self.p + n > self.b.len() {
            self.ok = false;
            return 0;
        }
        let v = (0..n).fold(0u32, |v, i| v | (self.b[self.p + i] as u32) << (8 * i));
        self.p += n;
        v
    }
    fn vector(&mut self, n: usize) -> Vec<u8> {
        if self.p + n > self.b.len() {
            self.ok = false;
            return Vec::new();
        }
        let v = self.b[self.p..self.p + n].to_vec();
        self.p += n;
        v
    }
    fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        if self.p + N > self.b.len() {
            self.ok = false;
        } else {
            out.copy_from_slice(&self.b[self.p..self.p + N]);
            self.p += N;
        }
        out
    }
    fn text(&mut self) -> String {
        let n = self.number(1) as usize;
        String::from_utf8_lossy(&self.vector(n)).into_owned()
    }
    fn end(&self) -> bool {
        self.ok && self.p == self.b.len()
    }
}

fn put(b: &mut Vec<u8>, value: u32, n: usize) {
    b.extend_from_slice(&value.to_le_bytes()[..n]);
}

fn text(b: &mut Vec<u8>, s: &str) -> bool {
    if s.len() > 255 {
        return false;
    }
    b.push(s.len() as u8);
    b.extend_from_slice(s.as_bytes());
    true
}

#[derive(Clone, Debug, Default)]
struct Slot {
    id: u32,
    /// Arrival order, to replace the oldest.
    order: u32,
    valid: bool,
    future: bool,
    dynamic_target: bool,
    job: Job,
}

#[derive(Debug, Default)]
pub struct Protocol {
    jobs: [Slot; JOB_SLOTS],
    order: u32,
    pending: Vec<ShareAck>,
    acknowledgments: Vec<ShareAck>,
    sequence: u32,
    channel: u32,
    group: u32,
    target: [u8; 32],
    previous: [u8; 32],
    ntime: u32,
    nbits: u32,
    prefix: Vec<u8>,
    extranonce_size: u16,
    have_previous: bool,
    have_current: bool,
    require_version_rolling: bool,
    fixed_version: bool,
    version_rolling_refused: bool,
    state: u8,
    current: Job,
    error: String,
}

impl Protocol {
    pub fn new() -> Self {
        Self::default()
    }

    /// SetupConnection. `hardware` names the device ("ESP32",
    /// "Bitaxe Gamma 601 / BM1370"...). `require_version_rolling`: the ASIC
    /// backends need rolling bits to avoid exhausting their nonce space.
    pub fn setup(
        &mut self,
        host: &str,
        port: u16,
        agent: &str,
        hardware: &str,
        firmware: &str,
        require_version_rolling: bool,
    ) -> Option<Frame> {
        self.require_version_rolling = require_version_rolling;
        let mut b = Vec::new();
        put(&mut b, 0, 1);
        put(&mut b, 2, 2);
        put(&mut b, 2, 2);
        // Request bit 2 is REQUIRES_VERSION_ROLLING.
        put(&mut b, if require_version_rolling { 4 } else { 0 }, 4);
        if !text(&mut b, host) {
            return None;
        }
        put(&mut b, port as u32, 2);
        for t in [agent, hardware, firmware, ""] {
            if !text(&mut b, t) {
                return None;
            }
        }
        Some(Frame::new(0, 0x00, b))
    }

    /// OpenExtendedMiningChannel.
    pub fn open(&mut self, user: &str, hashrate: f32) -> Option<Frame> {
        if self.state != 1 || !hashrate.is_finite() || hashrate < 0.0 {
            return None;
        }
        let mut b = Vec::new();
        put(&mut b, 1, 4);
        if !text(&mut b, user) {
            return None;
        }
        put(&mut b, hashrate.to_bits(), 4);
        // Maximum target; the pool remains free to assign a harder one.
        b.extend_from_slice(&[255; 32]);
        // At least 8 bytes of unique local extranonce space.
        put(&mut b, 8, 2);
        Some(Frame::new(0, 0x13, b))
    }

    pub fn job(&self) -> &Job {
        &self.current
    }
    pub fn has_job(&self) -> bool {
        self.have_current
    }
    pub fn opened(&self) -> bool {
        self.state == 2
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    /// The answers carried by the last Accepted/Rejected event.
    pub fn acknowledgments(&self) -> &[ShareAck] {
        &self.acknowledgments
    }
    pub fn submission_id(&self) -> u32 {
        self.sequence.wrapping_sub(1)
    }
    pub fn difficulty(&self) -> f64 {
        work::hash_difficulty(&self.target)
    }
    pub fn error(&self) -> &str {
        &self.error
    }
    pub fn version_rolling_refused(&self) -> bool {
        self.version_rolling_refused
    }

    fn fail(&mut self, message: &str) -> Event {
        self.error = message.to_owned();
        Event::Error
    }

    fn fail_version_rolling(&mut self) -> Event {
        self.version_rolling_refused = true;
        self.fail("SV2 pool does not support required version rolling")
    }

    fn activate(&mut self, index: usize, time: u32, clean: bool) {
        let slot = &mut self.jobs[index];
        self.current = slot.job.clone();
        self.current.ntime = time;
        self.current.nbits = self.nbits;
        self.current.clean = clean;
        self.current.prev_hash = self.previous;
        self.have_current = true;
        slot.future = false;
    }

    /// An empty slot, else the oldest job that is neither the current one
    /// nor a future job (a pool may announce the next block's job long
    /// before its SetNewPrevHash).
    fn free_slot(&self) -> usize {
        let mut best = None::<usize>;
        let older = |i: usize, best: Option<usize>| {
            best.is_none_or(|b| (self.jobs[i].order.wrapping_sub(self.jobs[b].order) as i32) < 0)
        };
        for (i, s) in self.jobs.iter().enumerate() {
            if !s.valid {
                return i;
            }
            if !s.future && !(self.have_current && s.job.id == self.current.id) && older(i, best) {
                best = Some(i);
            }
        }
        if let Some(b) = best {
            return b;
        }
        for (i, s) in self.jobs.iter().enumerate() {
            if s.future && older(i, best) {
                best = Some(i);
            }
        }
        best.unwrap_or(0)
    }

    fn pending_index(&self, sequence: u32) -> Option<usize> {
        self.pending.iter().position(|p| p.id == sequence)
    }

    /// SubmitSharesExtended, or `None` for an obsolete or invalid share.
    pub fn submit(&mut self, s: &Share) -> Option<Frame> {
        if !self.opened() || self.pending.len() >= MAX_PENDING || !self.have_current {
            return None;
        }
        let slot = self.jobs.iter().find(|slot| {
            slot.valid
                && !slot.future
                && slot.job.id == s.job_id
                && (slot.job.version ^ s.version) & !slot.job.version_mask == 0
        })?;
        if !work::meets_target(&s.hash, &slot.job.share_target) {
            return None;
        }
        let (id, assigned) = (slot.id, slot.job.difficulty);
        let extranonce = work::from_hex(&s.extranonce2)?;
        if extranonce.len() != self.extranonce_size as usize {
            return None;
        }
        let mut b = Vec::new();
        for v in [self.channel, self.sequence, id, s.nonce, s.ntime, s.version] {
            put(&mut b, v, 4);
        }
        b.push(extranonce.len() as u8);
        b.extend_from_slice(&extranonce);
        self.pending.push(ShareAck::new(self.sequence, assigned));
        self.sequence = self.sequence.wrapping_add(1);
        Some(Frame::new(0x8000, 0x1b, b))
    }

    fn ours(&self, channel: u32) -> bool {
        channel == self.channel || channel == self.group
    }

    pub fn handle(&mut self, f: &Frame) -> Event {
        self.acknowledgments.clear();
        if f.extension & 0x7fff != 0 || f.payload.len() > MAX_PAYLOAD {
            return self.fail("Unsupported SV2 frame");
        }
        let mut r = Reader::new(&f.payload);
        match f.kind {
            0x02 => {
                let unsupported = r.number(4);
                self.error = r.text();
                if !r.end() {
                    return self.fail("Invalid SV2 setup error");
                }
                if self.require_version_rolling && unsupported & 4 != 0 {
                    return self.fail_version_rolling();
                }
                return Event::Error;
            }
            0x01 => {
                let (version, flags) = (r.number(2), r.number(4));
                if !r.end() || version != 2 || self.state != 0 || flags & !3 != 0 {
                    return self.fail("Invalid SV2 setup response");
                }
                // Success bit 0 is REQUIRES_FIXED_VERSION.
                self.fixed_version = flags & 1 != 0;
                if self.require_version_rolling && self.fixed_version {
                    return self.fail_version_rolling();
                }
                self.state = 1;
                return Event::Setup;
            }
            0x12 => {
                let request = r.number(4);
                self.error = r.text();
                if !r.end() || request != 1 {
                    return self.fail("Invalid SV2 channel error");
                }
                return Event::Error;
            }
            0x14 => {
                let (request, channel) = (r.number(4), r.number(4));
                let target: [u8; 32] = r.array();
                let size = r.number(2) as u16;
                let n = r.number(1) as usize;
                let prefix = r.vector(n);
                let group = r.number(4);
                if !r.end()
                    || self.state != 1
                    || request != 1
                    || !(8..=32).contains(&size)
                    || prefix.len() > 32
                    || channel == group
                    || target == [0; 32]
                {
                    return self.fail("Invalid SV2 channel parameters");
                }
                (self.channel, self.group, self.prefix, self.extranonce_size) = (channel, group, prefix, size);
                self.target = target;
                self.state = 2;
                return Event::Opened;
            }
            _ => {}
        }
        if !self.opened() {
            return self.fail("SV2 channel not open");
        }
        match f.kind {
            0x1f => self.new_extended_job(&mut r),
            0x20 => {
                let (channel, id) = (r.number(4), r.number(4));
                let prev: [u8; 32] = r.array();
                let (time, bits) = (r.number(4), r.number(4));
                if !r.end() || !self.ours(channel) || time == 0 || work::target_from_bits(bits).is_none() {
                    return self.fail("Invalid SV2 previous hash");
                }
                let Some(chosen) = self.jobs.iter().position(|s| s.valid && s.future && s.id == id) else {
                    return self.fail("Unknown SV2 future job");
                };
                (self.previous, self.ntime, self.nbits, self.have_previous) = (prev, time, bits, true);
                for (i, s) in self.jobs.iter_mut().enumerate() {
                    if i != chosen {
                        *s = Slot::default();
                    }
                }
                self.activate(chosen, time, true);
                Event::Job
            }
            0x21 => {
                let channel = r.number(4);
                let target: [u8; 32] = r.array();
                if !r.end() || !self.ours(channel) || target == [0; 32] {
                    return self.fail("Invalid SV2 target");
                }
                self.target = target;
                let difficulty = self.difficulty();
                // Explicit-ntime jobs retain the target assigned when received;
                // future jobs follow the channel target even after activation.
                for slot in self.jobs.iter_mut().filter(|s| s.valid && s.dynamic_target) {
                    slot.job.share_target = target;
                    slot.job.difficulty = difficulty;
                    if self.have_current && slot.job.id == self.current.id {
                        self.current.share_target = target;
                        self.current.difficulty = difficulty;
                    }
                }
                if self.have_current {
                    self.current.clean = false;
                }
                Event::Target
            }
            0x1c => {
                let (channel, last, count) = (r.number(4), r.number(4), r.number(4));
                r.number(4);
                r.number(4); // new_shares_sum: U64, accounting only
                if !r.end() || channel != self.channel {
                    return self.fail("Invalid SV2 share acknowledgment");
                }
                if self.pending_index(last).is_none() {
                    return Event::None; // duplicate/unknown acknowledgment
                }
                let eligible = self.pending.iter().filter(|p| (p.id.wrapping_sub(last) as i32) <= 0).count();
                if count == 0 || count as usize != eligible {
                    return self.fail("Invalid SV2 share acknowledgment count");
                }
                let (done, keep): (Vec<ShareAck>, Vec<ShareAck>) =
                    self.pending.iter().partition(|p| (p.id.wrapping_sub(last) as i32) <= 0);
                self.pending = keep;
                self.acknowledgments = done;
                Event::Accepted
            }
            0x1d => {
                let (channel, sequence) = (r.number(4), r.number(4));
                let error = r.text();
                if !r.end() || channel != self.channel {
                    return self.fail("Invalid SV2 share rejection");
                }
                let Some(index) = self.pending_index(sequence) else { return Event::None };
                let mut ack = self.pending.remove(index);
                ack.reason = RejectReason::classify(0, &error);
                self.acknowledgments.push(ack);
                self.error = error;
                Event::Rejected
            }
            0x18 => {
                let channel = r.number(4);
                let reason = r.text();
                if !r.end() {
                    return self.fail("Invalid SV2 channel closure");
                }
                if !self.ours(channel) {
                    return Event::None;
                }
                self.error = format!("SV2 channel closed: {reason}");
                Event::Error
            }
            0x19 => {
                let channel = r.number(4);
                let n = r.number(1) as usize;
                let prefix = r.vector(n);
                if !r.end() || prefix.len() > 32 {
                    return self.fail("Invalid SV2 extranonce prefix");
                }
                // Applies only to subsequently received jobs; group prefixes ignored.
                if channel == self.channel {
                    self.prefix = prefix;
                }
                Event::None
            }
            0x25 => {
                let (group, count) = (r.number(4), r.number(2) as usize);
                if !r.ok || count > (f.payload.len() - r.p) / 4 {
                    return self.fail("Invalid SV2 channel group");
                }
                let mut member = false;
                for _ in 0..count {
                    member |= r.number(4) == self.channel;
                }
                if !r.end() || group == self.channel {
                    return self.fail("Invalid SV2 channel group");
                }
                if member {
                    self.group = group;
                }
                Event::None
            }
            _ => Event::None, // optional messages do not mutate the current job
        }
    }

    fn new_extended_job(&mut self, r: &mut Reader) -> Event {
        let (channel, id, option) = (r.number(4), r.number(4), r.number(1));
        let time = if option == 1 { r.number(4) } else { 0 };
        let mut slot = Slot { valid: true, id, future: option == 0, ..Default::default() };
        slot.dynamic_target = slot.future;
        slot.job.exact_share_target = true;
        slot.job.share_target = self.target;
        slot.job.difficulty = self.difficulty();
        slot.job.extranonce1 = self.prefix.clone();
        slot.job.extranonce2_size = self.extranonce_size as usize;
        slot.job.id = id.to_string();
        slot.job.version = r.number(4);
        let (rolling, branches) = (r.number(1), r.number(1));
        slot.job.version_mask = if rolling != 0 { 0x1fff_e000 } else { 0 };
        if !r.ok || option > 1 || rolling > 1 || branches > 32 {
            return self.fail("Invalid SV2 job fields");
        }
        for _ in 0..branches {
            let b: [u8; 32] = r.array();
            slot.job.branches.push(b);
        }
        let n = r.number(2) as usize;
        slot.job.coinb1 = r.vector(n);
        let n = r.number(2) as usize;
        slot.job.coinb2 = r.vector(n);
        if !r.end() || !self.ours(channel) || (!slot.future && (!self.have_previous || time < self.ntime)) {
            return self.fail("Invalid SV2 mining job");
        }
        if self.require_version_rolling && rolling == 0 {
            return self.fail_version_rolling();
        }
        if self.fixed_version && rolling != 0 {
            return self.fail("SV2 job violates fixed-version requirement");
        }
        let index = self.free_slot();
        slot.order = self.order;
        self.order = self.order.wrapping_add(1);
        let mut bytes = slot.job.coinb1.len() + slot.job.coinb2.len();
        for (i, old) in self.jobs.iter().enumerate().filter(|(_, s)| s.valid) {
            if old.id == id {
                return self.fail("Duplicate SV2 job identifier");
            }
            if i != index {
                bytes += old.job.coinb1.len() + old.job.coinb2.len();
            }
        }
        if bytes > 32768 {
            return self.fail("SV2 pending job memory limit exceeded");
        }
        let future = slot.future;
        self.jobs[index] = slot;
        if !future {
            self.activate(index, time, false);
            return Event::Job;
        }
        Event::None
    }
}
