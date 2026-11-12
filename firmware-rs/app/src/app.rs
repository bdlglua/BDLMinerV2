//! The firmware's shared context: live settings, miner, pool state,
//! supervision, notifications and the platform services they use.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bdlminer_core::address;

use crate::log::Log;
use crate::miner::Miner;
use crate::notify::{Notifier, Targets};
use crate::platform::{native_user_agent, BoardProfile, Clock, RandomFn, Storage, SystemInfo, FW_VERSION};
use crate::pool::{Endpoint, PoolConfig, PoolState};
use crate::settings::{self, Settings};
use crate::watch::Watch;
use crate::web_validation::RebindingGuard;

pub struct App {
    pub board: BoardProfile,
    settings: RwLock<Settings>,
    brightness: AtomicU8,
    settings_seq: Mutex<u32>,
    staged_for_restart: AtomicBool,
    pub clock: Arc<dyn Clock>,
    pub storage: Arc<Mutex<dyn Storage>>,
    pub log: Arc<Log>,
    pub miner: Arc<Miner>,
    pub pool: Arc<PoolState>,
    pub pool_config: PoolConfig,
    pub watch: Watch,
    pub notifier: Notifier,
    pub updater: crate::update::Updater,
    pub market: crate::market::Market,
    pub block_status: crate::block_status::BlockStatus,
    pub mqtt: crate::mqtt::MqttState,
    pub crashes: crate::crash::Crashes,
    summary: Mutex<crate::summary::Summary>,
    pub system: Arc<dyn SystemInfo>,
    /// "bdlminer-xxxx": mDNS and DHCP name.
    pub hostname: String,
    pub mac: u64,
    pub boot_id: u32,
    random: RandomFn,
    /// Restart due at this millis() (0 = none), and why.
    restart_at: AtomicU32,
    restart_why: Mutex<String>,
    /// The loop applies display settings after a live change.
    pub settings_changed: AtomicBool,
    pub ota_busy: AtomicBool,
    /// A freshly installed firmware not confirmed yet (rolls back if it fails).
    pub firmware_trial: AtomicBool,
    pub rebinding: Mutex<RebindingGuard>,
}

/// Name of the access point the setup portal opens.
pub fn portal_ap_name(mac: u64) -> String {
    format!("BDLMinerV2-{:04X}", (mac >> 32) & 0xffff)
}

pub fn device_host_name(mac: u64) -> String {
    format!("bdlminer-{:04x}", (mac >> 32) & 0xffff)
}

/// The pool task's view of the settings.
pub fn pool_config(s: &Settings, board: &BoardProfile) -> PoolConfig {
    PoolConfig {
        coin: s.coin(),
        main: Endpoint {
            host: s.pool_host.clone(),
            port: s.pool_port,
            protocol: s.pool_protocol,
            authority: s.pool_authority.clone(),
        },
        backup: (!s.backup_host.is_empty()).then(|| Endpoint {
            host: s.backup_host.clone(),
            port: s.backup_port,
            protocol: s.backup_protocol,
            authority: s.backup_authority.clone(),
        }),
        user: s.btc_address.clone(),
        password: s.pool_password.clone(),
        suggested_difficulty: s.suggested_difficulty,
        user_agent: s.user_agent.clone(),
        native_agent: native_user_agent(),
        rotate_hours: s.rotate_hours,
        asic: board.asic(),
        hardware: board.hardware().into(),
        firmware: FW_VERSION.into(),
    }
}

pub struct Platform {
    pub board: BoardProfile,
    pub clock: Arc<dyn Clock>,
    pub storage: Arc<Mutex<dyn Storage>>,
    pub system: Arc<dyn SystemInfo>,
    /// Console output of the log (serial port, stderr).
    pub console: Box<dyn Fn(&str) + Send + Sync>,
    pub mac: u64,
    pub boot_id: u32,
    /// Cryptographically secure random bytes.
    pub random: RandomFn,
}

impl App {
    /// Loads the settings (or defaults), then builds the miner and services.
    pub fn new(p: Platform) -> Arc<Self> {
        let console = p.console;
        let log = Arc::new(Log::new(move |line| console(line)));
        let mut values = Settings::defaults(&p.board);
        let sequence = settings::load(&mut *p.storage.lock().unwrap(), &mut values, &p.board).unwrap_or(0);
        settings::sanitize(&mut values, &p.board);
        let clock = p.clock.clone();
        let (l1, c1) = (log.clone(), clock.clone());
        let miner = Miner::new(values.coin(), clock.clone(), p.storage.clone(), move |s| l1.line(c1.millis(), s));
        let (l2, c2) = (log.clone(), clock.clone());
        let pool = PoolState::new(move |s| l2.line(c2.millis(), s));
        let (l3, c3) = (log.clone(), clock.clone());
        let notifier = Notifier::new(
            Targets {
                ntfy_server: values.ntfy_server.clone(),
                ntfy_topic: values.ntfy_topic.clone(),
                webhook_url: values.webhook_url.clone(),
            },
            move |s| l3.line(c3.millis(), s),
        );
        let (l4, c4) = (log.clone(), clock.clone());
        let updater = crate::update::Updater::new(move |s| l4.line(c4.millis(), s));
        Arc::new(App {
            updater,
            market: Default::default(),
            block_status: Default::default(),
            mqtt: Default::default(),
            crashes: Default::default(),
            summary: Default::default(),
            pool_config: pool_config(&values, &p.board),
            watch: Watch::new(&p.board),
            brightness: AtomicU8::new(values.brightness),
            settings: RwLock::new(values),
            settings_seq: Mutex::new(sequence),
            staged_for_restart: AtomicBool::new(false),
            hostname: device_host_name(p.mac),
            mac: p.mac,
            random: p.random,
            board: p.board,
            clock,
            storage: p.storage,
            log,
            miner,
            pool,
            notifier,
            system: p.system,
            boot_id: p.boot_id,
            restart_at: AtomicU32::new(0),
            restart_why: Mutex::new(String::new()),
            settings_changed: AtomicBool::new(false),
            ota_busy: AtomicBool::new(false),
            firmware_trial: AtomicBool::new(false),
            rebinding: Mutex::new(RebindingGuard::default()),
        })
    }

    pub fn log(&self, line: &str) {
        self.log.line(self.clock.millis(), line);
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn brightness(&self) -> u8 {
        self.brightness.load(Ordering::Relaxed)
    }

    /// Saves a settings record; `for_restart` stages it for the restart that
    /// follows (later ordinary saves are then refused).
    pub fn save_settings(&self, values: &Settings, for_restart: bool) -> bool {
        let mut seq = self.settings_seq.lock().unwrap();
        if self.staged_for_restart.load(Ordering::Relaxed) && !for_restart {
            return false;
        }
        let saved = settings::save(&mut *self.storage.lock().unwrap(), values, *seq, &self.board);
        match saved {
            Some(next) => {
                *seq = next;
                if for_restart {
                    self.staged_for_restart.store(true, Ordering::Relaxed);
                }
                true
            }
            None => {
                drop(seq);
                self.log("[cfg] settings save failed; previous configuration kept");
                false
            }
        }
    }

    /// The brightness button and the web page change it without a restart.
    pub fn set_brightness(&self, percent: u8) {
        let percent = percent.min(100);
        self.brightness.store(percent, Ordering::Relaxed);
        let mut copy = self.settings();
        copy.brightness = percent;
        self.save_settings(&copy, false);
    }

    /// Applies the live (number and switch) settings of `pending`.
    pub fn apply_live(&self, pending: &Settings) {
        settings::copy_live(&mut self.settings.write().unwrap(), pending, &self.board);
        self.settings_changed.store(true, Ordering::Relaxed);
    }

    pub fn request_restart(&self, why: &str) {
        *self.restart_why.lock().unwrap() = why.to_owned();
        let due = self.clock.millis().wrapping_add(1500);
        self.restart_at.store(due.max(1), Ordering::Relaxed);
    }

    pub fn restart_pending(&self) -> bool {
        self.restart_at.load(Ordering::Relaxed) != 0
    }

    /// The reason, once a requested restart is due.
    pub fn restart_due(&self) -> Option<String> {
        let at = self.restart_at.load(Ordering::Relaxed);
        (at != 0 && (self.clock.millis().wrapping_sub(at) as i32) >= 0)
            .then(|| self.restart_why.lock().unwrap().clone())
    }

    /// Logs the configuration at boot, as the C++ firmware does.
    pub fn log_configuration(&self) {
        let s = self.settings();
        self.log(&format!("BDLMiner V2 {FW_VERSION} on {} (Rust)", self.board.name));
        self.log(&format!("[cfg] pool {}:{} user {}", s.pool_host, s.pool_port, s.btc_address));
        if !s.backup_host.is_empty() {
            self.log(&format!("[cfg] backup pool {}:{}", s.backup_host, s.backup_port));
        }
        self.log(&format!("[cfg] cryptocurrency {} (pool-supplied SHA-256d jobs)", s.coin().ticker()));
        let info = address::parse(&s.btc_address, s.coin());
        if info.valid() {
            let test = if info.network == address::Network::Main { "" } else { " (testnet)" };
            self.log(&format!("[cfg] address type {}{test}", info.kind.name()));
        } else {
            self.log(&format!(
                "[cfg] Address is not recognized for {}; check the pool's address/account requirements.",
                s.coin().ticker()
            ));
        }
    }

    /// One pass of the main loop's once-a-second work.
    pub fn second_tick(&self, ui: &dyn crate::watch::Indicators) {
        self.miner.tick();
        let s = self.settings();
        let log = |line: &str| self.log(line);
        self.watch.tick(
            &s,
            &*self.clock,
            &self.miner,
            self.system.temperature(),
            self.system.on_battery() == 1,
            self.brightness(),
            &self.pool.status(),
            &self.system.boot_reason(),
            &self.notifier,
            ui,
            &log,
        );
        self.summary.lock().unwrap().tick(self);
        self.crashes.poll(self);
        self.miner.history_poll();
    }

    /// Saves what must survive, before a restart the firmware does itself.
    pub fn prepare_restart(&self, why: &str) {
        self.log(&format!("[watch] restarting: {why}"));
        self.miner.save_stats(false);
    }

    /// The setup portal's password: the published one until the miner has a
    /// payout address, then one of its own (not derived from the MAC, which
    /// the access point broadcasts), kept in its own namespace.
    pub fn portal_password(&self) -> String {
        if self.settings().btc_address.is_empty() {
            return settings::PORTAL_AP_PASSWORD.into();
        }
        let mut store = self.storage.lock().unwrap();
        if let Some(saved) = store.get("portal", "pw").and_then(|b| String::from_utf8(b).ok()).filter(|p| p.len() == 10)
        {
            return saved;
        }
        const LETTERS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789"; // none that read alike
        let mut bytes = [0u8; 10];
        (self.random)(&mut bytes);
        let drawn: String = bytes.iter().map(|b| LETTERS[*b as usize % LETTERS.len()] as char).collect();
        // A password nobody could read back would lock the owner out.
        if store.set("portal", "pw", drawn.as_bytes()) && store.get("portal", "pw").as_deref() == Some(drawn.as_bytes())
        {
            drawn
        } else {
            settings::PORTAL_AP_PASSWORD.into()
        }
    }

    /// Statistics survive a factory reset; settings do not.
    pub fn factory_reset(&self) {
        self.miner.save_stats(false);
        self.storage.lock().unwrap().clear(settings::NAMESPACE);
    }
}
