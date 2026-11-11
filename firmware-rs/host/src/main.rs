//! The BDLMiner firmware on a PC, for development and tests: the same
//! settings, pool sessions, statistics and dashboard as the ESP32 build,
//! with the software SHA-256d kernel on N threads.
//!
//!     bdlminer-host --user <address[.worker]> [--pool host:port] [--threads N]
//!                   [--http 127.0.0.1:8080] [--data DIR] [--difficulty D]
//!                   [--sv2-key KEY] [--coin BTC]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bdlminer_app::app::{App, Platform};
use bdlminer_app::miner::software_worker;
use bdlminer_app::notify::HttpClient;
use bdlminer_app::platform::{BoardProfile, Clock, Storage, SystemInfo};
use bdlminer_app::pool;
use bdlminer_app::settings::{self, Settings};
use bdlminer_app::std_platform::{os_random, FileStorage, SystemClock, TcpNetwork};
use bdlminer_app::watch::NoIndicators;
use bdlminer_app::web::{self, Method, Request};
use bdlminer_core::coin::Coin;

struct HostSystem {
    http: String,
}

impl SystemInfo for HostSystem {
    fn ip(&self) -> String {
        self.http.clone()
    }
    fn boot_reason(&self) -> String {
        "power on".into()
    }
}

/// The host build does not deliver notifications; it logs them.
struct NoDelivery;
impl HttpClient for NoDelivery {
    fn post(&self, _url: &str, _headers: &[(&str, &str)], _body: &[u8]) -> Result<u16, String> {
        Err("not sent (host)".into())
    }
}

struct Args {
    user: Option<String>,
    pool: Option<String>,
    threads: usize,
    http: String,
    data: PathBuf,
    difficulty: Option<f64>,
    sv2_key: Option<String>,
    coin: Option<Coin>,
}

fn usage() -> ! {
    eprintln!(
        "usage: bdlminer-host --user <address[.worker]> [--pool host:port] [--threads N] [--http 127.0.0.1:8080]\n\
         \x20                    [--data DIR] [--difficulty D] [--sv2-key KEY] [--coin BTC|BCH|BSV|XEC|DGB|PPC]"
    );
    std::process::exit(2)
}

fn parse_args() -> Args {
    let mut a = Args {
        user: None,
        pool: None,
        threads: thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(3),
        http: "127.0.0.1:8080".into(),
        data: PathBuf::from("bdlminer-data"),
        difficulty: None,
        sv2_key: None,
        coin: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--user" => a.user = Some(value()),
            "--pool" => a.pool = Some(value()),
            "--threads" => a.threads = value().parse().unwrap_or_else(|_| usage()),
            "--http" => a.http = value(),
            "--data" => a.data = value().into(),
            "--difficulty" => a.difficulty = Some(value().parse().unwrap_or_else(|_| usage())),
            "--sv2-key" => a.sv2_key = Some(value()),
            "--coin" => a.coin = Some(Coin::from_ticker(&value().to_ascii_uppercase()).unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    a.threads = a.threads.clamp(1, bdlminer_app::miner::MAX_MINERS);
    a
}

/// Applies the command line to the saved settings, like the setup portal.
fn configure(storage: &Arc<Mutex<dyn Storage>>, board: &BoardProfile, a: &Args) {
    let mut store = storage.lock().unwrap();
    let mut s = Settings::defaults(board);
    let seq = settings::load(&mut *store, &mut s, board).unwrap_or(0);
    let before = s.clone();
    if let Some(user) = &a.user {
        s.btc_address = user.clone();
    }
    if let Some(p) = &a.pool {
        s.pool_host = p.clone(); // sanitize() splits host:port and URLs
    }
    if let Some(d) = a.difficulty {
        s.suggested_difficulty = d;
    }
    if let Some(k) = &a.sv2_key {
        s.pool_protocol = 1;
        s.pool_authority = k.clone();
    }
    if let Some(c) = a.coin {
        s.mining_coin = c.id();
    }
    settings::sanitize(&mut s, board);
    if s.btc_address.is_empty() {
        eprintln!("No payout address configured: pass --user <address>.");
        usage();
    }
    if s != before && settings::save(&mut *store, &s, seq, board).is_none() {
        eprintln!("warning: settings could not be saved in {}", a.data.display());
    }
}

fn serve(app: Arc<App>, address: &str) {
    let listener = match TcpListener::bind(address) {
        Ok(l) => l,
        Err(e) => {
            app.log(&format!("[web] cannot listen on {address}: {e}"));
            return;
        }
    };
    app.log(&format!("[web] dashboard on http://{address}/"));
    for stream in listener.incoming().flatten() {
        let app = app.clone();
        thread::spawn(move || {
            let _ = handle_connection(&app, stream);
        });
    }
}

fn handle_connection(app: &App, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = match parts.next() {
        Some("GET") => Method::Get,
        Some("POST") => Method::Post,
        _ => Method::Other,
    };
    let target = parts.next().unwrap_or("/").to_owned();
    let (mut host, mut origin, mut authorization, mut length) = (None, None, None, 0usize);
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = h.split_once(':') {
            let value = value.trim().to_owned();
            match name.trim().to_ascii_lowercase().as_str() {
                "host" => host = Some(value),
                "origin" => origin = Some(value),
                "authorization" => authorization = Some(value),
                "content-length" => length = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; length.min(64 * 1024)];
    reader.read_exact(&mut body)?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let request = Request {
        method,
        path,
        query,
        host: host.as_deref(),
        origin: origin.as_deref(),
        authorization: authorization.as_deref(),
        body: &body,
    };
    let response = web::handle(app, &request);
    let mut out = stream;
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        if response.status < 400 { "OK" } else { "Error" },
        response.content_type,
        response.body.len()
    );
    for (name, value) in &response.headers {
        head += &format!("{name}: {value}\r\n");
    }
    head += "\r\n";
    out.write_all(head.as_bytes())?;
    out.write_all(&response.body)
}

fn main() {
    let args = parse_args();
    let board = BoardProfile::host();
    let storage: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(FileStorage { dir: args.data.clone() }));
    configure(&storage, &board, &args);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut id = [0u8; 12];
    os_random(&mut id);
    let app = App::new(Platform {
        board,
        clock: clock.clone(),
        storage,
        system: Arc::new(HostSystem { http: args.http.clone() }),
        console: Box::new(|line| eprintln!("{line}")),
        mac: u64::from_le_bytes(id[..8].try_into().unwrap()),
        boot_id: u32::from_le_bytes(id[8..].try_into().unwrap()),
        random: Arc::new(os_random),
    });
    app.log_configuration();
    app.miner.load_stats();
    for _ in 0..args.threads {
        let Some(engine) = app.miner.register_engine("sw") else { break };
        let miner = app.miner.clone();
        thread::spawn(move || software_worker(&miner, engine, false));
    }
    app.log(&format!("[miner] {} software engine(s)", args.threads));
    {
        let app = app.clone();
        thread::spawn(move || {
            let net = TcpNetwork { poll: Duration::from_millis(20), random: os_random };
            pool::run(&net, &app.pool_config, &app.miner, &app.pool, &*app.clock);
        });
    }
    {
        let app = app.clone();
        let address = args.http.clone();
        thread::spawn(move || serve(app, &address));
    }
    let mut last_tick = clock.millis();
    let mut last_log = clock.millis();
    loop {
        app.miner.renew_safety();
        let now = clock.millis();
        if now.wrapping_sub(last_tick) >= 1000 {
            last_tick = now;
            app.second_tick(&NoIndicators);
            app.notifier.poll(&NoDelivery);
            if let Some(why) = app.restart_due().or_else(|| app.watch.take_restart()) {
                app.prepare_restart(&why);
                app.log("[host] exiting instead of restarting; start again to apply");
                std::process::exit(0);
            }
        }
        if now.wrapping_sub(last_log) >= 30_000 {
            last_log = now;
            let s = app.miner.stats();
            app.log(&format!(
                "[stats] {:.2} kH/s | shares {} ok / {} rejected | best {:.6} (ever {:.6}) | {}",
                app.miner.hashrate() / 1000.0,
                s.accepted,
                s.rejected,
                s.session_best_diff,
                s.best_diff,
                app.pool.status()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
