//! Request checks shared with tests: backups, Host/Origin headers (DNS
//! rebinding) and local-network origins.

use serde_json::Value;

fn nonnegative(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(|d| d.is_finite() && d >= 0.0)
}

fn is_u32(v: Option<&Value>) -> bool {
    v.and_then(Value::as_u64).is_some_and(|n| n <= u32::MAX as u64)
}

/// Checks a backup completely before anything is written to flash.
pub fn backup_error(backup: &Value, max_top_shares: usize) -> Option<&'static str> {
    // Settings-only exports from earlier firmware have no format marker.
    let marker = backup.get("bdlminer_backup")?;
    if marker.as_u64() != Some(1) {
        return Some("Unsupported backup version");
    }
    if !backup.get("settings").is_some_and(Value::is_object) {
        return Some("Invalid backup settings");
    }
    let Some(st) = backup.get("stats").filter(|s| s.is_object()) else { return Some("Missing backup statistics") };
    if !nonnegative(st.get("best"))
        || !st.get("hashes").is_some_and(Value::is_u64)
        || !is_u32(st.get("shares"))
        || !is_u32(st.get("blocks"))
        || !is_u32(st.get("seconds"))
        || !st.get("top").is_some_and(Value::is_array)
    {
        return Some("Invalid or incomplete backup statistics");
    }
    let top = st["top"].as_array().unwrap();
    if top.len() > max_top_shares {
        return Some("Too many best shares in backup");
    }
    for entry in top {
        if !entry.is_object()
            || !nonnegative(entry.get("d"))
            || entry["d"].as_f64().unwrap_or(0.0) <= 0.0
            || !is_u32(entry.get("t"))
        {
            return Some("Invalid best share in backup");
        }
    }
    None
}

fn local_suffix(host: &str) -> bool {
    const LOCAL: [&str; 6] = [".local", ".lan", ".home", ".home.arpa", ".internal", ".localdomain"];
    let lower = host.to_ascii_lowercase();
    LOCAL.iter().any(|s| lower.len() > s.len() && lower.ends_with(s))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    Refused,
    Local,
    OwnNamed,
}

/// The Host header of a request that changes the miner or reads secrets: an
/// IP address, a name without a dot or a local name are Local; the miner's
/// own name under another domain is OwnNamed; anything else is Refused.
pub fn host_kind(host: Option<&str>, own_name: &str) -> HostKind {
    let Some(host) = host.filter(|h| !h.is_empty() && !h.starts_with('[')) else { return HostKind::Local };
    let mut name = host.split(':').next().unwrap_or("");
    name = name.strip_suffix('.').unwrap_or(name);
    if name.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return HostKind::Local;
    }
    let Some(dot) = name.find('.') else { return HostKind::Local };
    if local_suffix(name) {
        return HostKind::Local;
    }
    if !own_name.is_empty() && name[..dot].eq_ignore_ascii_case(own_name) {
        return HostKind::OwnNamed;
    }
    HostKind::Refused
}

/// After a few refused names, the miner's name under a foreign domain is
/// refused as well for an hour (DNS rebinding needs to guess the name).
#[derive(Clone, Debug, Default)]
pub struct RebindingGuard {
    misses: u8,
    last_miss: u32,
}

impl RebindingGuard {
    pub const MISSES: u8 = 3;
    pub const LOCK_MS: u32 = 60 * 60 * 1000;

    pub fn allowed(&mut self, host: Option<&str>, own_name: &str, now: u32) -> bool {
        let kind = host_kind(host, own_name);
        if kind == HostKind::Local {
            return true;
        }
        if self.misses != 0 && now.wrapping_sub(self.last_miss) >= Self::LOCK_MS {
            self.misses = 0;
        }
        if kind == HostKind::Refused {
            self.misses = (self.misses + 1).min(Self::MISSES);
            self.last_miss = now;
            return false;
        }
        self.misses < Self::MISSES
    }
}

/// The Origin header, when sent, must be this very miner.
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin.filter(|o| !o.is_empty()) else { return true };
    let Some(rest) = origin.strip_prefix("http://") else { return false };
    host.is_some_and(|h| h.eq_ignore_ascii_case(rest))
}

/// A private, link-local, loopback or carrier-grade (VPN) IPv4 address.
pub fn private_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let mut p = [0u32; 4];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        p[i] = part.parse().unwrap_or(256);
        if p[i] > 255 {
            return false;
        }
    }
    p[0] == 10
        || p[0] == 127
        || (p[0] == 172 && (16..=31).contains(&p[1]))
        || (p[0] == 192 && p[1] == 168)
        || (p[0] == 169 && p[1] == 254)
        || (p[0] == 100 && (64..=127).contains(&p[1]))
}

/// An Origin on the local network: another miner's dashboard, a home server.
pub fn origin_local(origin: Option<&str>) -> bool {
    let Some(host) = origin.and_then(|o| o.strip_prefix("http://")) else { return false };
    if host.starts_with('[') {
        return false;
    }
    let mut name = host.split([':', '/']).next().unwrap_or("");
    name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() {
        return false;
    }
    if name.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return private_ipv4(name);
    }
    !name.contains('.') || local_suffix(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_origins() {
        let own = "bdlminer-1a2b";
        assert_eq!(host_kind(Some("192.168.1.20"), own), HostKind::Local);
        assert_eq!(host_kind(Some("bdlminer-1a2b.local"), own), HostKind::Local);
        assert_eq!(host_kind(Some("miner"), own), HostKind::Local);
        assert_eq!(host_kind(Some("BDLMINER-1A2B.fritz.box:80"), own), HostKind::OwnNamed);
        assert_eq!(host_kind(Some("evil.example.com"), own), HostKind::Refused);
        assert_eq!(host_kind(None, own), HostKind::Local);
        let mut guard = RebindingGuard::default();
        assert!(guard.allowed(Some("bdlminer-1a2b.fritz.box"), own, 0));
        for _ in 0..3 {
            assert!(!guard.allowed(Some("evil.example.com"), own, 10));
        }
        assert!(!guard.allowed(Some("bdlminer-1a2b.fritz.box"), own, 20));
        assert!(guard.allowed(Some("10.0.0.2"), own, 20));
        assert!(guard.allowed(Some("bdlminer-1a2b.fritz.box"), own, 10 + RebindingGuard::LOCK_MS));
        assert!(origin_allowed(None, Some("x")) && origin_allowed(Some("http://10.0.0.2"), Some("10.0.0.2")));
        assert!(!origin_allowed(Some("https://10.0.0.2"), Some("10.0.0.2")));
        assert!(origin_local(Some("http://192.168.1.4:8080")) && origin_local(Some("http://nas.lan")));
        assert!(!origin_local(Some("http://8.8.8.8")) && !origin_local(Some("http://example.com")));
        assert!(private_ipv4("100.64.0.1") && !private_ipv4("100.128.0.1") && !private_ipv4("1.2.3"));
    }

    #[test]
    fn backups() {
        let ok: Value = serde_json::from_str(
            r#"{"bdlminer_backup":1,"settings":{},"stats":{"best":1.5,"hashes":10,"shares":1,"blocks":0,"seconds":5,"top":[{"d":1.5,"t":0}]}}"#,
        )
        .unwrap();
        assert_eq!(backup_error(&ok, 10), None);
        assert_eq!(backup_error(&serde_json::json!({"pool":"x"}), 10), None);
        let mut bad = ok.clone();
        bad["stats"]["top"][0]["d"] = 0.into();
        assert_eq!(backup_error(&bad, 10), Some("Invalid best share in backup"));
        bad["bdlminer_backup"] = 2.into();
        assert_eq!(backup_error(&bad, 10), Some("Unsupported backup version"));
        assert_eq!(backup_error(&ok, 0), Some("Too many best shares in backup"));
    }
}
