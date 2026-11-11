//! Supported SHA-256d chains. A pool supplies the actual chain's jobs:
//! choosing a profile cannot change the chain served by an arbitrary endpoint.

/// Stored as a number: existing records keep BTC = 0 and BCH = 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Coin {
    #[default]
    Btc = 0,
    Bch = 1,
    Bsv = 2,
    Xec = 3,
    Dgb = 4,
    Ppc = 5,
}

pub const COUNT: u8 = 6;

impl Coin {
    pub const ALL: [Coin; 6] = [Coin::Btc, Coin::Bch, Coin::Bsv, Coin::Xec, Coin::Dgb, Coin::Ppc];

    pub fn from_id(id: u32) -> Option<Coin> {
        Self::ALL.get(id as usize).copied()
    }
    pub fn id(self) -> u8 {
        self as u8
    }
    pub fn ticker(self) -> &'static str {
        ["BTC", "BCH", "BSV", "XEC", "DGB", "PPC"][self as usize]
    }
    pub fn name(self) -> &'static str {
        ["Bitcoin", "Bitcoin Cash", "Bitcoin SV", "eCash", "DigiByte", "Peercoin"][self as usize]
    }
    pub fn from_ticker(value: &str) -> Option<Coin> {
        Self::ALL.into_iter().find(|c| c.ticker() == value)
    }
    pub fn bitcoin_data(self) -> bool {
        self == Coin::Btc
    }
    pub fn halving_interval(self) -> u32 {
        if matches!(self, Coin::Dgb | Coin::Ppc) {
            0
        } else {
            210_000
        }
    }
    /// CashAddr prefix for network 0 main, 1 test, 2 regtest.
    pub fn cash_prefix(self, network: usize) -> Option<&'static str> {
        const BCH: [&str; 3] = ["bitcoincash", "bchtest", "bchreg"];
        const XEC: [&str; 3] = ["ecash", "ectest", "ecregtest"];
        match self {
            Coin::Bch => BCH.get(network).copied(),
            Coin::Xec => XEC.get(network).copied(),
            _ => None,
        }
    }
    /// DigiByte Core: BLOCK_VERSION_ALGO = 15 << 8, BLOCK_VERSION_SHA256D =
    /// 2 << 8 (zero is Scrypt). Algorithm bits must also stay out of rolling.
    pub fn job_compatible(self, version: u32, rolling_mask: u32) -> bool {
        self != Coin::Dgb || (version & 0x0000_0f00 == 0x0000_0200 && rolling_mask & 0x0000_0f00 == 0)
    }
    /// Rejects payout addresses that clearly belong to another chain. Legacy
    /// encodings can overlap and pools may use account names: those pass.
    pub fn address_compatible(self, user: &str) -> bool {
        let user = user.split('.').next().unwrap_or("").to_ascii_lowercase();
        for cash in [Coin::Bch, Coin::Xec] {
            for net in 0..3 {
                let prefix = format!("{}:", cash.cash_prefix(net).unwrap_or_default());
                if user.starts_with(&prefix) {
                    return self == cash;
                }
            }
        }
        // Peercoin signet shares Bitcoin's tb HRP.
        if user.starts_with("tb1") {
            return matches!(self, Coin::Btc | Coin::Ppc);
        }
        let starts = |list: &[&str]| list.iter().any(|p| user.starts_with(p));
        if starts(&["bc1", "bcrt1"]) {
            return self == Coin::Btc;
        }
        if starts(&["dgb1", "dgbt1", "dgbrt1"]) {
            return self == Coin::Dgb;
        }
        if starts(&["pc1", "tpc1", "pcrt1"]) {
            return self == Coin::Ppc;
        }
        true
    }
    /// Rejects known single-chain services when changing chain. Custom
    /// endpoints remain the operator's responsibility.
    pub fn pool_compatible(self, endpoint: &str) -> bool {
        let mut host = endpoint;
        if let Some(i) = host.find("://") {
            host = &host[i + 3..];
        }
        let host = host.split([':', '/']).next().unwrap_or("");
        let mut host: String =
            host.chars().filter(|c| !c.is_ascii_whitespace()).collect::<String>().to_ascii_lowercase();
        if host.ends_with('.') {
            host.pop();
        }
        const ENDPOINTS: [(&str, Coin); 10] = [
            ("b4c.f2pool.com", Coin::Bch),
            ("bch.hmpool.io", Coin::Bch),
            ("xec.hmpool.io", Coin::Xec),
            ("bch.solopool.org", Coin::Bch),
            ("xec.solopool.org", Coin::Xec),
            ("bsv.solopool.org", Coin::Bsv),
            ("dgb.solopool.org", Coin::Dgb),
            ("bsv.molepool.com", Coin::Bsv),
            ("xec.molepool.com", Coin::Xec),
            ("peermining.org", Coin::Ppc),
        ];
        if let Some((_, id)) = ENDPOINTS.iter().find(|(h, _)| *h == host) {
            return self == *id;
        }
        if self == Coin::Btc {
            return true;
        }
        const BTC_ONLY: [&str; 6] =
            ["public-pool.io", "nerdminers.org", "nerdminer.io", "ckpool.org", "ocean.xyz", "braiins.com"];
        !BTC_ONLY.contains(&public_pool_group(&host))
    }
    /// Why this pool cannot be used for the coin, `None` when it can.
    pub fn pool_error(self, host: &str, protocol: u8) -> Option<&'static str> {
        if self != Coin::Btc && protocol != 0 {
            return Some("This cryptocurrency requires Stratum V1");
        }
        if !self.pool_compatible(host) {
            return Some("Choose a pool for the selected cryptocurrency");
        }
        None
    }
}

/// Public telemetry uses a fixed group, never the configured hostname.
/// Appended in step with the statistics service, which refuses unknown groups.
pub fn public_pool_group(host: &str) -> &'static str {
    const DOMAINS: [&str; 25] = [
        "public-pool.io",
        "nerdminers.org",
        "nerdminer.io",
        "ckpool.org",
        "ocean.xyz",
        "braiins.com",
        "viabtc.com",
        "f2pool.com",
        "antpool.com",
        "bdl-pool.com",
        "solopool.org",
        "kano.is",
        "atlaspool.io",
        "solomining.de",
        "satoshiradio.nl",
        "noderunners.network",
        "d-central.tech",
        "mining-dutch.nl",
        "luxor.tech",
        "nicehash.com",
        "secpool.com",
        "spiderpool.com",
        "hmpool.io",
        "molepool.com",
        "peermining.org",
    ];
    let host = host.strip_suffix('.').unwrap_or(host).as_bytes();
    if host.is_empty() || host.len() > 253 || host[0] == b'.' {
        return "other";
    }
    for (i, &c) in host.iter().enumerate() {
        let ok = c.is_ascii_alphanumeric() || c == b'-' || (c == b'.' && i > 0 && host[i - 1] != b'.');
        if !ok {
            return "other";
        }
    }
    for domain in DOMAINS {
        let n = domain.len();
        if host.len() >= n
            && (host.len() == n || host[host.len() - n - 1] == b'.')
            && host[host.len() - n..].eq_ignore_ascii_case(domain.as_bytes())
        {
            return domain;
        }
    }
    "other"
}
