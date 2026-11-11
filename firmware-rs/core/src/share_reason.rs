//! Why a pool refused a share, from its error code and message.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RejectReason {
    #[default]
    Unknown = 0,
    Stale,
    Duplicate,
    LowDifficulty,
    Authorization,
    Invalid,
}

impl RejectReason {
    pub const COUNT: usize = 6;
    pub const ALL: [RejectReason; 6] = [
        RejectReason::Unknown,
        RejectReason::Stale,
        RejectReason::Duplicate,
        RejectReason::LowDifficulty,
        RejectReason::Authorization,
        RejectReason::Invalid,
    ];

    pub fn name(self) -> &'static str {
        ["unknown", "stale", "duplicate", "low_difficulty", "authorization", "invalid"][self as usize]
    }

    pub fn classify(code: i64, message: &str) -> RejectReason {
        match code {
            21 => return RejectReason::Stale,
            22 => return RejectReason::Duplicate,
            23 => return RejectReason::LowDifficulty,
            24 | 25 => return RejectReason::Authorization,
            _ => {}
        }
        let text: String = message.chars().take(95).collect::<String>().to_ascii_lowercase();
        let has = |list: &[&str]| list.iter().any(|w| text.contains(w));
        if has(&["stale", "job-not-found", "invalid-job-id"]) {
            RejectReason::Stale
        } else if has(&["duplicate"]) {
            RejectReason::Duplicate
        } else if has(&["difficulty-too-low", "low difficulty", "low-difficulty"]) {
            RejectReason::LowDifficulty
        } else if has(&["unauthor", "not-subscribed", "invalid-channel"]) {
            RejectReason::Authorization
        } else if has(&["invalid", "bad-"]) {
            RejectReason::Invalid
        } else {
            RejectReason::Unknown
        }
    }
}
