//! Stratum identity (user agent), learned separately for each pool endpoint.

pub const NERD_MINER: &str = "NerdMinerV2";

#[derive(Debug, Default, Clone)]
pub struct PoolAgent {
    host: String,
    port: u16,
    fallback: bool,
}

impl PoolAgent {
    pub fn compatible() -> &'static str {
        NERD_MINER
    }

    pub fn endpoint(&mut self, host: &str, port: u16) {
        let mut name = host.to_ascii_lowercase();
        if name.ends_with('.') {
            name.pop();
        }
        if name == self.host && port == self.port {
            return;
        }
        self.host = name;
        self.port = port;
        // Always start with the native identity; compatibility is learned
        // only from an explicit agent refusal.
        self.fallback = false;
    }

    pub fn select<'a>(&self, native: &'a str, automatic: bool, manual: Option<&'a str>) -> &'a str {
        if automatic {
            if self.fallback {
                NERD_MINER
            } else {
                native
            }
        } else {
            manual.unwrap_or(native)
        }
    }

    /// Retries only an explicit agent refusal, once.
    pub fn refused(&mut self, error: Option<&str>) -> bool {
        let Some(error) = error else { return false };
        if self.fallback {
            return false;
        }
        if error.to_ascii_lowercase().contains("agent") {
            self.fallback = true;
            return true;
        }
        false
    }
}

/// The two selectable identities: NerdMinerV2 or the firmware's own.
pub mod user_agent {
    use super::NERD_MINER;

    pub fn normalize<'a>(saved: &str, native: &'a str) -> &'a str {
        if saved == NERD_MINER {
            NERD_MINER
        } else {
            native
        }
    }

    pub fn valid(value: &str, native: &str) -> bool {
        value == NERD_MINER || value == native
    }

    pub fn portal_select(saved: &str, native: &str) -> String {
        let nerd = saved == NERD_MINER;
        format!(
            "<label for='ua'>User agent</label><select id='ua' name='ua'><option value='NerdMinerV2'{}>NerdMinerV2</option><option value='{native}'{}>{native}</option></select>",
            if nerd { " selected" } else { "" },
            if nerd { "" } else { " selected" }
        )
    }
}
