//! Firmware updates from the GitHub releases of the Rust firmware (tags
//! `rust-v<version>`), as the C++ firmware's ghupdate.cpp: checked 30 seconds
//! after boot and then every hour (5 minutes after a failed check), or on
//! request from the dashboard. The release's `<env>-firmware.bin` is
//! installed (by itself with the setting `update` = 2, or from the
//! dashboard) only when `<env>-firmware.bin.sig` is a valid signature of it
//! by the release key, and the image names this board and that version.
//!
//! The C++ firmware's update channels (update.json, the beta manifest) never
//! list Rust images, and its releases carry no `-rust-firmware.bin`: the two
//! firmwares never install each other by themselves.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use bdlminer_core::image_marker::Scanner;
use bdlminer_core::release_version;
use bdlminer_core::sha256::Sha256;
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use p256::ecdsa::{Signature, VerifyingKey};

use crate::app::App;
use crate::notify::{get_body, BodySink, HttpClient};
use crate::platform::{transport_agent, FW_VERSION};
use crate::settings::GITHUB_REPO;

/// The release key (ECDSA P-256, uncompressed point): src/update_key.h.
const PUBLIC_KEY: [u8; 65] = [
    0x04, 0x5a, 0xff, 0xb8, 0xa2, 0x86, 0xcf, 0x08, 0xc0, 0xa0, 0x55, 0x40, 0x5d, 0x51, 0x84, 0x40, 0x84, 0x78, 0x66,
    0x36, 0x3b, 0x22, 0xc1, 0xf5, 0xb1, 0x99, 0x5d, 0x0e, 0xe6, 0x67, 0x53, 0xe7, 0x4e, 0x1a, 0x20, 0x5d, 0xc2, 0x51,
    0x9a, 0x71, 0xa9, 0xfa, 0x95, 0xf2, 0xcf, 0x17, 0xd2, 0x8d, 0x8e, 0x34, 0xc6, 0x95, 0xef, 0x97, 0xd1, 0xbe, 0x0c,
    0x7e, 0xd2, 0x0b, 0xb2, 0x2f, 0x86, 0x58, 0x3b,
];

const FIRST_CHECK_MS: u32 = 30 * 1000;
const CHECK_MS: u32 = 3600 * 1000;
const RETRY_MS: u32 = 5 * 60 * 1000;
const FIRST_PING_MS: u32 = 5 * 60 * 1000;
const PING_MS: u32 = 24 * 3600 * 1000;
/// What a TLS handshake needs (tls.h kTlsMinFree).
const MIN_HEAP: u32 = 54 * 1024;
/// GitHub's release file host refuses an expired link with this status.
const LINK_REFUSED: u16 = 618;

/// True when `signature` (DER, as `openssl dgst -sha256 -sign` writes it) is
/// the release key's signature of the SHA-256 `hash`.
pub fn verify(hash: &[u8; 32], signature: &[u8]) -> bool {
    verify_with(&PUBLIC_KEY, hash, signature)
}

fn verify_with(public_key: &[u8], hash: &[u8; 32], signature: &[u8]) -> bool {
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_sec1_bytes(public_key), Signature::from_der(signature)) else {
        return false;
    };
    key.verify_prehash(hash, &sig).is_ok()
}

/// The firmware slot an update is written to.
pub trait Firmware: Send + Sync {
    /// Whether the partition table has a second slot.
    fn supported(&self) -> bool;
    /// Starts writing the other slot.
    fn begin(&self) -> Result<Box<dyn FirmwareWriter>, String>;
    /// After a successful installation, before the restart: the running
    /// firmware worked (its trial ends), `version` is expected next.
    fn installed(&self, version: &str) {
        let _ = version;
    }
}

/// An image being written; dropped unfinished, the slot is left as it was.
pub trait FirmwareWriter: Send {
    fn write(&mut self, data: &[u8]) -> Result<(), String>;
    /// Checks the image and makes it the next one to boot.
    fn finish(self: Box<Self>) -> Result<(), String>;
}

/// What the dashboard shows (`github_update`).
#[derive(Clone, Debug, Default)]
pub struct UpdateState {
    /// A release was found at least once.
    pub checked: bool,
    pub checking: bool,
    /// Newer than this firmware, with its files for this board.
    pub available: bool,
    pub installing: bool,
    /// Percent, while installing.
    pub progress: u8,
    /// Acknowledges an accepted dashboard command.
    pub install_request: u32,
    pub retry_seconds: u32,
    /// Of the latest release.
    pub version: String,
    /// Last problem, "" when none.
    pub error: String,
}

#[derive(Default)]
struct Inner {
    next_check: u32,
    next_ping: u32,
    firmware_url: String,
    signature_url: String,
    /// A release whose installation was refused (bad signature, wrong
    /// image) gives the same answer every time: not installed automatically
    /// again, only when asked for from the dashboard.
    refused_version: String,
    refused_error: String,
    retry_install: bool,
    failures: u8,
    retry_at: u32,
    channel: u8,
    last_wait: u8,
}

pub struct Updater {
    state: Mutex<UpdateState>,
    inner: Mutex<Inner>,
    install_requested: AtomicBool,
    check_requested: AtomicBool,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

/// Finds the Rust releases' firmware links in GitHub's release list, as it
/// streams in: `.../releases/download/rust-v<version>/<env>-firmware.bin`.
/// Keeps the newest version allowed by the channel.
pub struct ReleaseScan {
    env: String,
    allow_beta: bool,
    carry: Vec<u8>,
    pub best: Option<(String, String)>,
}

impl ReleaseScan {
    const NEEDLE: &'static [u8] = b"/releases/download/rust-v";

    pub fn new(env: &str, allow_beta: bool) -> Self {
        ReleaseScan { env: env.into(), allow_beta, carry: Vec::new(), best: None }
    }

    fn consider(&mut self, url_tail: &str) {
        // url_tail: "<version>/<file>"
        let Some((version, file)) = url_tail.split_once('/') else { return };
        if file != format!("{}-firmware.bin", self.env) {
            return;
        }
        let Some(parsed) = release_version::parse(version) else { return };
        if !parsed.pre.is_empty() && !self.allow_beta {
            return;
        }
        let url = format!("https://github.com/{GITHUB_REPO}/releases/download/rust-v{url_tail}");
        if self.best.as_ref().is_none_or(|(v, _)| release_version::compare(version, v) > 0) {
            self.best = Some((version.into(), url));
        }
    }
}

impl BodySink for ReleaseScan {
    fn data(&mut self, chunk: &[u8]) -> Result<(), String> {
        let mut buf = std::mem::take(&mut self.carry);
        buf.extend_from_slice(chunk);
        let mut pos = 0;
        loop {
            let Some(at) = find(&buf[pos..], Self::NEEDLE) else {
                let keep = buf.len().saturating_sub(Self::NEEDLE.len() - 1).max(pos);
                self.carry = buf[keep..].to_vec();
                return Ok(());
            };
            let start = pos + at + Self::NEEDLE.len();
            match buf[start..].iter().position(|&b| b == b'"') {
                Some(end) => {
                    if end < 160 {
                        if let Ok(tail) = std::str::from_utf8(&buf[start..start + end]) {
                            let tail = tail.to_owned();
                            self.consider(&tail);
                        }
                    }
                    pos = start + end;
                }
                None if buf.len() - start < 160 => {
                    self.carry = buf[pos + at..].to_vec();
                    return Ok(());
                }
                None => pos = start,
            }
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Hashes, checks and writes the downloaded image.
struct Install<'a> {
    firmware: &'a dyn Firmware,
    writer: Option<Box<dyn FirmwareWriter>>,
    sha: Sha256,
    board: Scanner,
    version: Scanner,
    size: u64,
    done: u64,
    state: &'a Mutex<UpdateState>,
}

impl BodySink for Install<'_> {
    fn begin(&mut self, length: Option<u64>) -> Result<(), String> {
        self.size = length.filter(|&n| n > 0).ok_or("the firmware size is unknown")?;
        self.writer = Some(self.firmware.begin()?);
        Ok(())
    }
    fn data(&mut self, chunk: &[u8]) -> Result<(), String> {
        let writer = self.writer.as_mut().ok_or("the update was not started")?;
        self.sha.update(chunk);
        self.board.feed(chunk);
        self.version.feed(chunk);
        writer.write(chunk)?;
        self.done += chunk.len() as u64;
        self.state.lock().unwrap().progress = (self.done * 100 / self.size.max(1)).min(99) as u8;
        Ok(())
    }
}

enum Outcome {
    Done,
    Retry,
    Refused,
}

impl Updater {
    pub fn new(log: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Updater {
            state: Mutex::new(UpdateState::default()),
            inner: Mutex::new(Inner { next_check: FIRST_CHECK_MS, next_ping: FIRST_PING_MS, ..Default::default() }),
            install_requested: AtomicBool::new(false),
            check_requested: AtomicBool::new(false),
            log: Box::new(log),
        }
    }

    fn set_error(&self, error: String) {
        (self.log)(&format!("[update] {error}"));
        self.state.lock().unwrap().error = error;
    }

    pub fn state(&self, now: u32) -> UpdateState {
        let mut s = self.state.lock().unwrap().clone();
        s.checking |= self.check_requested.load(Ordering::Relaxed);
        s.installing |= self.install_requested.load(Ordering::Relaxed);
        let retry_at = self.inner.lock().unwrap().retry_at;
        let remaining = retry_at.wrapping_sub(now) as i32;
        s.retry_seconds = if retry_at != 0 && remaining > 0 { (remaining as u32).div_ceil(1000) } else { 0 };
        s
    }

    /// Queues an immediate check; false when disabled or busy.
    pub fn request_check(&self, app: &App) -> bool {
        let s = self.state(app.clock.millis());
        if app.settings().github_update == 0 || s.checking || s.installing || app.ota_busy.load(Ordering::Relaxed) {
            return false;
        }
        !self.check_requested.swap(true, Ordering::Relaxed)
    }

    /// Asks for the available update to be installed; false when there is none.
    pub fn request_install(&self, app: &App) -> bool {
        let s = self.state(app.clock.millis());
        if app.settings().github_update == 0
            || !s.available
            || s.checking
            || s.installing
            || app.ota_busy.load(Ordering::Relaxed)
        {
            return false;
        }
        if self.install_requested.swap(true, Ordering::Relaxed) {
            return false;
        }
        self.state.lock().unwrap().install_request += 1;
        true
    }

    /// A version that must not be installed automatically again (rolled
    /// back after failed trials); `why` is shown while it is the latest.
    pub fn refuse(&self, version: &str, why: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.refused_version = version.into();
        inner.refused_error = why.into();
    }

    /// Checks or installs when it is time. Called every few seconds by the
    /// task that makes the HTTPS requests. One network operation per call.
    pub fn poll(&self, app: &App, http: &dyn HttpClient, firmware: &dyn Firmware, link_up: bool) {
        if app.ota_busy.load(Ordering::Relaxed) {
            return;
        }
        let settings = app.settings();
        let now = app.clock.millis();
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.channel != settings.update_channel {
                inner.channel = settings.update_channel;
                inner.retry_install = false;
                inner.failures = 0;
                inner.firmware_url.clear();
                inner.signature_url.clear();
                inner.next_check = now;
                inner.retry_at = 0;
                self.install_requested.store(false, Ordering::Relaxed);
                let mut s = self.state.lock().unwrap();
                s.checked = false;
                s.available = false;
                s.version.clear();
                s.error.clear();
            }
        }
        let due = settings.github_update != 0
            && (self.install_requested.load(Ordering::Relaxed)
                || self.check_requested.load(Ordering::Relaxed)
                || now.wrapping_sub(self.inner.lock().unwrap().next_check) as i32 >= 0);
        let memory = app.system.free_heap() >= MIN_HEAP;
        if due {
            if !link_up || !memory {
                let wait = if link_up { 1 } else { 2 };
                let mut inner = self.inner.lock().unwrap();
                if inner.last_wait != wait || self.state.lock().unwrap().error.is_empty() {
                    inner.last_wait = wait;
                    drop(inner);
                    self.set_error(format!("update waiting: {}", if wait == 1 { "low memory" } else { "no WiFi" }));
                }
                return;
            }
            let requested_install = self.install_requested.swap(false, Ordering::Relaxed);
            let requested_check = self.check_requested.swap(false, Ordering::Relaxed);
            let mut ready = true;
            if requested_check || !requested_install {
                self.state.lock().unwrap().checking = true;
                ready = self.check(app, http);
                self.inner.lock().unwrap().next_check =
                    app.clock.millis().wrapping_add(if ready { CHECK_MS } else { RETRY_MS });
                self.state.lock().unwrap().checking = false;
            }
            let current = self.state(app.clock.millis());
            let (refused, retry_install) = {
                let inner = self.inner.lock().unwrap();
                (inner.refused_version == current.version, inner.retry_install)
            };
            let automatic = settings.github_update == 2 && !refused;
            if ready && (requested_install || retry_install || automatic) && current.available {
                let outcome = self.install(app, http, firmware);
                let mut inner = self.inner.lock().unwrap();
                if let Outcome::Refused = outcome {
                    let s = self.state.lock().unwrap();
                    inner.refused_version = s.version.clone();
                    inner.refused_error = s.error.clone();
                }
                inner.retry_install = matches!(outcome, Outcome::Retry);
                if inner.retry_install {
                    inner.failures = (inner.failures + 1).min(4);
                    let delay = (RETRY_MS << (inner.failures - 1)).min(30 * 60 * 1000);
                    inner.next_check = app.clock.millis().wrapping_add(delay);
                    inner.retry_at = inner.next_check.max(1);
                    drop(inner);
                    (self.log)(&format!("[update] will retry the installation in {} seconds", delay / 1000));
                } else {
                    inner.failures = 0;
                    inner.retry_at = 0;
                }
            } else if ready && !current.available {
                let mut inner = self.inner.lock().unwrap();
                inner.retry_install = false;
                inner.failures = 0;
                inner.retry_at = 0;
            } else if !ready {
                let mut inner = self.inner.lock().unwrap();
                if inner.retry_install {
                    inner.retry_at = inner.next_check.max(1);
                }
            }
            return;
        }
        let ping_due = {
            let mut inner = self.inner.lock().unwrap();
            let due = settings.usage_count && now.wrapping_sub(inner.next_ping) as i32 >= 0 && link_up && memory;
            if due {
                inner.next_ping = now.wrapping_add(PING_MS);
            }
            due
        };
        if ping_due {
            self.ping(app, http);
        }
    }

    /// GitHub's release list, newest first. Unreadable or incomplete newer
    /// releases are retried sooner (false).
    fn check(&self, app: &App, http: &dyn HttpClient) -> bool {
        let channel = app.settings().update_channel;
        // A beta firmware follows the betas until a stable release.
        let this_is_beta = release_version::parse(FW_VERSION).is_some_and(|v| !v.pre.is_empty());
        let mut scan = ReleaseScan::new(app.board.env, channel == 1 || this_is_beta);
        let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases?per_page=20");
        let headers = [("User-Agent", transport_agent()), ("Accept", "application/vnd.github+json")];
        match http.get(&url, &headers, &mut scan) {
            Ok(200) => {}
            Ok(code) => {
                self.set_error(format!("release list unavailable: HTTP {code}"));
                return false;
            }
            Err(e) => {
                self.set_error(format!("release list unavailable: {e}"));
                return false;
            }
        }
        let Some((version, firmware_url)) = scan.best else {
            let mut s = self.state.lock().unwrap();
            s.checked = true;
            s.available = false;
            s.error.clear();
            drop(s);
            (self.log)(&format!("[update] no Rust release for {} yet", app.board.env));
            return true;
        };
        let newer = release_version::compare(&version, FW_VERSION) > 0;
        let was_available;
        {
            let mut s = self.state.lock().unwrap();
            was_available = s.available && s.version == version;
            s.checked = true;
            s.available = newer;
            s.version = version.clone();
            s.error.clear();
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.signature_url = format!("{firmware_url}.sig");
            inner.firmware_url = firmware_url;
        }
        let refused = {
            let inner = self.inner.lock().unwrap();
            (newer && inner.refused_version == version).then(|| inner.refused_error.clone())
        };
        if let Some(why) = refused {
            self.set_error(why);
        }
        (self.log)(&format!(
            "[update] latest release {version}, this firmware {FW_VERSION}{}",
            if newer { ": update available" } else { "" }
        ));
        if newer && !was_available {
            app.notifier.notify(
                "Firmware update",
                &format!("Version {version} is available (this miner runs {FW_VERSION})."),
                3,
                "arrow_up",
            );
        }
        true
    }

    fn install(&self, app: &App, http: &dyn HttpClient, firmware: &dyn Firmware) -> Outcome {
        if !firmware.supported() {
            self.set_error("this board has no room for a second firmware: update by USB".into());
            return Outcome::Refused;
        }
        if app.ota_busy.swap(true, Ordering::Relaxed) {
            self.set_error("another firmware update is already running".into());
            return Outcome::Retry;
        }
        app.miner.set_ota_busy(true);
        let version = {
            let mut s = self.state.lock().unwrap();
            s.installing = true;
            s.progress = 0;
            s.error.clear();
            s.version.clone()
        };
        (self.log)(&format!("[update] installing {version}"));
        let outcome = self.download(app, http, firmware, &version);
        let ok = matches!(outcome, Outcome::Done);
        {
            // After a success `installing` stays set until the restart.
            let mut s = self.state.lock().unwrap();
            if ok {
                s.progress = 100;
            } else {
                s.installing = false;
            }
        }
        if ok {
            firmware.installed(&version);
            let why = format!("updated to {version} from GitHub");
            (self.log)(&format!("[update] {why}, restarting"));
            app.request_restart(&why);
        } else {
            app.miner.set_ota_busy(false);
            app.ota_busy.store(false, Ordering::Relaxed);
        }
        outcome
    }

    fn download(&self, app: &App, http: &dyn HttpClient, firmware: &dyn Firmware, version: &str) -> Outcome {
        let (firmware_url, signature_url) = {
            let inner = self.inner.lock().unwrap();
            (inner.firmware_url.clone(), inner.signature_url.clone())
        };
        let headers = [("User-Agent", transport_agent())];
        let signature = match get_body(http, &signature_url, &headers, 128) {
            Ok((200, body)) if !body.is_empty() => body,
            Ok((code, _)) => {
                let refused = if code == LINK_REFUSED { ": GitHub refused its download link" } else { "" };
                self.set_error(format!("cannot download the signature (HTTP {code}{refused})"));
                return Outcome::Retry;
            }
            Err(e) => {
                self.set_error(format!("cannot download the signature: {e}"));
                return Outcome::Retry;
            }
        };
        // The signature says the file is a release, not which one: a signed
        // image of another board or of an older version would pass it too.
        // The image itself must say this board, this version.
        let mut install = Install {
            firmware,
            writer: None,
            sha: Sha256::new(),
            board: Scanner::default(),
            version: Scanner::default(),
            size: 0,
            done: 0,
            state: &self.state,
        };
        install.board.set("", app.board.env);
        install.version.set("BDLMinerV1/", version);
        match http.get(&firmware_url, &headers, &mut install) {
            Ok(200) => {}
            Ok(code) => {
                let refused = if code == LINK_REFUSED { ": GitHub refused its download link" } else { "" };
                self.set_error(format!("cannot download the firmware (HTTP {code}{refused})"));
                return Outcome::Retry;
            }
            Err(e) => {
                let progress = self.state.lock().unwrap().progress;
                self.set_error(format!("download interrupted at {progress}%: {e}"));
                return Outcome::Retry;
            }
        }
        if install.done != install.size {
            self.set_error(format!("download interrupted ({} of {} bytes)", install.done, install.size));
            return Outcome::Retry;
        }
        let Install { writer, sha, board, version: marker, .. } = install;
        let hash = sha.finish();
        if !verify(&hash, &signature) {
            self.set_error("bad signature: update refused".into());
            return Outcome::Refused;
        }
        if !board.found() || !marker.found() {
            self.set_error(format!("not firmware {version} for this board: update refused"));
            return Outcome::Refused;
        }
        let Some(writer) = writer else {
            self.set_error("the update was not started".into());
            return Outcome::Retry;
        };
        if let Err(e) = writer.finish() {
            self.set_error(format!("cannot finish the update: {e}"));
            return Outcome::Retry;
        }
        Outcome::Done
    }

    /// The anonymous usage count: GitHub counts the downloads of each release
    /// file, so fetching ping-<board>.txt of this version once a day makes
    /// the download counts read "miner-days" per version and board. Nothing
    /// about the miner is sent.
    fn ping(&self, app: &App, http: &dyn HttpClient) {
        let url =
            format!("https://github.com/{GITHUB_REPO}/releases/download/rust-v{FW_VERSION}/ping-{}.txt", app.board.env);
        let result = get_body(http, &url, &[("User-Agent", transport_agent())], 64);
        (self.log)(&match result {
            Ok((200, _)) => "[update] usage count: counted".into(),
            Ok((code, _)) => format!("[update] usage count: HTTP {code}"),
            Err(e) => format!("[update] usage count: {e}"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_newest_rust_firmware() {
        let page = br#"[{"tag_name":"v2.7.0","assets":[{"browser_download_url":"https://github.com/bdlglua/BDLMinerV2/releases/download/v2.7.0/m5stickc-plus2-firmware.bin"}]},
{"tag_name":"rust-v2.6.1-beta.2","assets":[{"browser_download_url":"https://github.com/bdlglua/BDLMinerV2/releases/download/rust-v2.6.1-beta.2/m5stickc-plus2-rust-firmware.bin"},
{"browser_download_url":"https://github.com/bdlglua/BDLMinerV2/releases/download/rust-v2.6.1-beta.2/esp32-rust-firmware.bin"}]},
{"tag_name":"rust-v2.6.1-beta.1","assets":[{"browser_download_url":"https://github.com/bdlglua/BDLMinerV2/releases/download/rust-v2.6.1-beta.1/m5stickc-plus2-rust-firmware.bin"}]}]"#;
        // Fed in small pieces: links cut by a chunk boundary are found.
        for size in [1, 7, 64, page.len()] {
            let mut scan = ReleaseScan::new("m5stickc-plus2-rust", true);
            for chunk in page.chunks(size) {
                scan.data(chunk).unwrap();
            }
            let (version, url) = scan.best.unwrap();
            assert_eq!(version, "2.6.1-beta.2");
            assert!(url.ends_with("/rust-v2.6.1-beta.2/m5stickc-plus2-rust-firmware.bin"));
        }
        let mut stable = ReleaseScan::new("m5stickc-plus2-rust", false);
        stable.data(page).unwrap();
        assert!(stable.best.is_none());
        let mut other = ReleaseScan::new("esp32s3-rust", true);
        other.data(page).unwrap();
        assert!(other.best.is_none());
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn openssl_signatures_verify() {
        // `openssl dgst -sha256 -sign` of "firmware" with a test key.
        let key = hex("04fd495d92162b582c56591145230e0f75932402ddf35fb2d9282acfd3f2522778c3de1a854585c2e6420ac560a63877f06a6f2f5f3feafd5c8c6dacf0c2291217");
        let sig = hex("304402203510e8d222f30a8d29f5da2a4d27b5f2c5b6929930449f09881dc1bedd9cdc8f02201f5b3f4753226757bd9e82a0083cb5d330cbf9218b69a8bf3d76d39636a6ec34");
        let hash = bdlminer_core::sha256::sha256(b"firmware");
        assert!(verify_with(&key, &hash, &sig));
        assert!(!verify_with(&key, &bdlminer_core::sha256::sha256(b"firmwarf"), &sig));
        // Not the release key.
        assert!(!verify(&hash, &sig));
    }

    #[test]
    fn signature_checks() {
        let hash = bdlminer_core::sha256::sha256(b"firmware");
        assert!(!verify(&hash, &[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01]));
        assert!(!verify(&hash, b"not der"));
    }

    #[test]
    fn version_marker_is_in_the_image() {
        let mut s = Scanner::default();
        s.set("BDLMinerV1/", FW_VERSION);
        s.feed(crate::platform::VERSION_MARKER.as_bytes());
        assert!(s.found());
    }
}
