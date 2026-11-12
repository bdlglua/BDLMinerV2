//! Platform services from the Rust standard library: the host build, tests,
//! and (TCP, threads) the ESP-IDF build, whose std is backed by lwIP.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::platform::{Clock, Connection, Network, Storage};

/// Milliseconds since this clock was created; wall time from the system.
pub struct SystemClock {
    start: Instant,
    /// Fixed UTC offset for `local_hour` (POSIX TZ parsing is the device's job).
    pub utc_offset_s: i32,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock { start: Instant::now(), utc_offset_s: 0 }
    }
}

impl Clock for SystemClock {
    fn millis(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }
    fn unix_time(&self) -> Option<u32> {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        (t > crate::platform::CLOCK_SET as u64).then_some(t as u32)
    }
    fn local_hour(&self) -> Option<u8> {
        let t = self.unix_time()? as i64 + self.utc_offset_s as i64;
        Some((t.rem_euclid(86400) / 3600) as u8)
    }
    fn local_time(&self) -> Option<crate::platform::LocalTime> {
        let t = self.unix_time()? as i64 + self.utc_offset_s as i64;
        let days = t.div_euclid(86400);
        // Civil date from days since 1970 (proleptic Gregorian).
        let (mut year, mut left) = (1970i64, days);
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let length = if leap { 366 } else { 365 };
            if left < length {
                break;
            }
            left -= length;
            year += 1;
        }
        Some(crate::platform::LocalTime {
            year: (year - 1900) as u32,
            year_day: left as u32,
            hour: (t.rem_euclid(86400) / 3600) as u8,
            minute: (t.rem_euclid(3600) / 60) as u8,
        })
    }
}

/// A TCP stream polled every `poll`.
pub struct TcpConnection(pub TcpStream);

impl Connection for TcpConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_all(data)
    }
}

pub struct TcpNetwork {
    pub poll: Duration,
    pub random: fn(&mut [u8]),
}

impl Network for TcpNetwork {
    type Conn = TcpConnection;
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> io::Result<TcpConnection> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(s) => {
                    s.set_nodelay(true)?;
                    s.set_read_timeout(Some(self.poll))?;
                    s.set_write_timeout(Some(Duration::from_secs(10)))?;
                    return Ok(TcpConnection(s));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    fn random(&self, out: &mut [u8]) {
        (self.random)(out)
    }
}

/// Random bytes from the operating system (/dev/urandom).
pub fn os_random(out: &mut [u8]) {
    let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(out)).is_ok();
    assert!(ok, "no system random source");
}

/// One file per key: <dir>/<namespace>/<key>.
pub struct FileStorage {
    pub dir: PathBuf,
}

impl FileStorage {
    fn path(&self, namespace: &str, key: &str) -> PathBuf {
        self.dir.join(namespace).join(key)
    }
}

impl Storage for FileStorage {
    fn get(&mut self, namespace: &str, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(namespace, key)).ok()
    }
    fn set(&mut self, namespace: &str, key: &str, value: &[u8]) -> bool {
        let path = self.path(namespace, key);
        let tmp = path.with_extension("tmp");
        std::fs::create_dir_all(self.dir.join(namespace)).is_ok()
            && std::fs::write(&tmp, value).is_ok()
            && std::fs::rename(&tmp, &path).is_ok()
    }
    fn remove(&mut self, namespace: &str, key: &str) -> bool {
        std::fs::remove_file(self.path(namespace, key)).is_ok()
    }
    fn clear(&mut self, namespace: &str) -> bool {
        let dir = self.dir.join(namespace);
        !dir.exists() || std::fs::remove_dir_all(dir).is_ok()
    }
}
