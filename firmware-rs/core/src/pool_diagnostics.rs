//! Why mining on the current pool does (or does not) produce shares.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Issue {
    Ready,
    Disconnected,
    WaitingWork,
    RollingRequired,
    HighDifficulty,
    InvalidDifficulty,
}

impl Issue {
    pub fn code(self) -> &'static str {
        match self {
            Issue::Ready => "ready",
            Issue::WaitingWork => "waiting_work",
            Issue::RollingRequired => "version_rolling_required",
            Issue::HighDifficulty => "infrequent_shares",
            Issue::InvalidDifficulty => "invalid_difficulty",
            Issue::Disconnected => "disconnected",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Input<'a> {
    pub connected: bool,
    pub work: bool,
    pub mask: u32,
    pub required_bits: u32,
    pub difficulty: f64,
    pub hashrate: f64,
    pub accepted_work: f64,
    pub seconds: f64,
    pub accepted: u32,
    pub status: &'a str,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub issue: Issue,
    pub rolling_bits: u32,
    pub compatible: bool,
    pub expected_share_seconds: f64,
    pub share_based_hashrate: f64,
}

pub fn diagnose(input: &Input) -> Report {
    let mut r = Report {
        issue: Issue::Disconnected,
        rolling_bits: input.mask.count_ones(),
        compatible: false,
        expected_share_seconds: f64::NAN,
        share_based_hashrate: f64::NAN,
    };
    if input.difficulty.is_finite() && input.difficulty > 0.0 && input.hashrate.is_finite() && input.hashrate > 0.0 {
        let value = input.difficulty * 4294967296.0 / input.hashrate;
        if value.is_finite() {
            r.expected_share_seconds = value;
        }
    }
    if input.accepted >= 3 && input.seconds >= 60.0 && input.accepted_work.is_finite() && input.accepted_work > 0.0 {
        r.share_based_hashrate = input.accepted_work * 4294967296.0 / input.seconds;
    }
    r.issue = if input.required_bits != 0
        && ((input.work && r.rolling_bits < input.required_bits)
            || (!input.work && input.status.contains("version rolling")))
    {
        Issue::RollingRequired
    } else if !input.connected {
        Issue::Disconnected
    } else if !input.work {
        Issue::WaitingWork
    } else if !input.difficulty.is_finite() || input.difficulty <= 0.0 {
        Issue::InvalidDifficulty
    } else {
        r.compatible = true;
        if r.expected_share_seconds > 3600.0 {
            Issue::HighDifficulty
        } else {
            Issue::Ready
        }
    };
    r
}
