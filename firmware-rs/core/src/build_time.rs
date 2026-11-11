//! The day this firmware was built, as a lower bound for a network clock.

/// A `__DATE__`-style "Nov  9 2026" as the Unix time of that day, 00:00 UTC;
/// 0 when it does not read as a date.
pub fn epoch(date: &str) -> u32 {
    const MONTHS: &str = "JanFebMarAprMayJunJulAugSepOctNovDec";
    let d = date.as_bytes();
    if d.len() != 11 || d[3] != b' ' || d[6] != b' ' {
        return 0;
    }
    let Some(month) = (0..12).find(|m| MONTHS.as_bytes()[3 * m..3 * m + 3] == d[..3]).map(|m| m as i64 + 1) else {
        return 0;
    };
    if !(d[4] == b' ' || d[4].is_ascii_digit()) || !d[5].is_ascii_digit() || !d[7..].iter().all(u8::is_ascii_digit) {
        return 0;
    }
    let day = if d[4] == b' ' { 0 } else { (d[4] - b'0') as i64 } * 10 + (d[5] - b'0') as i64;
    let year = d[7..].iter().fold(0i64, |y, c| y * 10 + (c - b'0') as i64);
    if !(1..=31).contains(&day) || !(1970..=2105).contains(&year) {
        return 0;
    }
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let days = era * 146097 + (yoe * 365 + yoe / 4 - yoe / 100 + doy) - 719468;
    let seconds = days * 86400;
    if seconds > 0 && seconds <= u32::MAX as i64 {
        seconds as u32
    } else {
        0
    }
}

/// The oldest time a network source may set the clock to (two days of margin),
/// never below `minimum`.
pub fn oldest_plausible(date: &str, minimum: u32) -> u32 {
    let built = epoch(date);
    let floor = built.saturating_sub(2 * 86400);
    floor.max(minimum)
}
