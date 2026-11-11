//! Pool identity: protocol and Stratum V2 authority, from settings or a URL.

/// Trims ASCII whitespace.
pub fn trim_identity(text: &str) -> String {
    text.trim_matches(|c: char| c.is_ascii_whitespace()).to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolIdentity {
    /// 0 Stratum V1, 1 Stratum V2 + Noise.
    pub protocol: u8,
    pub authority: String,
}

/// `stratum2+tcp://host:port/<authority>` selects Stratum V2 and carries the
/// authority key; other URLs keep the configured protocol and key.
pub fn pool_identity(host: &str, protocol: u8, authority: &str) -> PoolIdentity {
    let mut result = PoolIdentity { protocol, authority: trim_identity(authority) };
    let url = trim_identity(host);
    const SCHEME: &str = "stratum2+tcp://";
    if let Some(rest) = url.strip_prefix(SCHEME) {
        result.protocol = 1;
        if let Some(path) = rest.find('/') {
            if path + 1 < rest.len() {
                result.authority = trim_identity(&rest[path + 1..]);
            }
        }
    }
    result
}
