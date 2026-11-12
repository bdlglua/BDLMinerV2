use std::path::Path;
use std::{env, fs};

/// The Xtensa kernels of the C++ firmware (src/*.S), for `global_asm!` in
/// src/hwsha.rs: what the C preprocessor did for them, done here (they only
/// use `#include "sdkconfig.h"`, one `#if` around the whole file, which
/// src/hwsha.rs replaces with `#[cfg]`, and plain numeric `#define`s).
fn kernels() {
    let src = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../src");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).to_path_buf();
    for name in ["hw_sha_classic", "hw_sha_fill", "sw_sha_prefilter"] {
        let path = src.join(format!("{name}.S"));
        println!("cargo::rerun-if-changed={}", path.display());
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut defines: Vec<(String, String)> = Vec::new();
        let mut asm = String::with_capacity(text.len());
        for line in text.lines() {
            let directive = line.trim_start();
            if let Some(rest) = directive.strip_prefix("#define") {
                let mut parts = rest.split_whitespace();
                let (Some(key), Some(value), None) = (parts.next(), parts.next(), parts.next()) else {
                    panic!("{name}.S: unsupported #define: {line}");
                };
                defines.push((key.into(), value.into()));
                asm.push('\n');
                continue;
            }
            if directive.starts_with('#') {
                // #include, #if, #endif: the file's guard, now a #[cfg].
                asm.push('\n');
                continue;
            }
            asm.push_str(&substitute(line, &defines));
            asm.push('\n');
        }
        fs::write(out.join(format!("{name}.s")), asm).unwrap();
    }
}

/// `line` with every identifier that is a `#define` replaced by its value.
fn substitute(line: &str, defines: &[(String, String)]) -> String {
    if defines.is_empty() {
        return line.into();
    }
    let mut out = String::with_capacity(line.len());
    let mut word = String::new();
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let flush = |word: &mut String, out: &mut String| {
        match defines.iter().find(|(k, _)| k == word) {
            Some((_, v)) => out.push_str(v),
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in line.chars() {
        if ident(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// The build day as C's __DATE__ ("Nov  9 2026"): the oldest time the
/// pool's job time may set the clock to (esp/src/main.rs).
fn build_date() {
    let secs = env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<i64>().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    });
    // Howard Hinnant's civil_from_days.
    let z = secs.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    println!("cargo:rustc-env=BDL_BUILD_DATE={} {day:>2} {year}", MONTHS[(month - 1) as usize]);
}

fn main() {
    build_date();
    // The chip cfgs esp-idf-sys passes on (esp32, esp32s3...).
    println!("cargo::rustc-check-cfg=cfg(esp32, esp32s2, esp32s3, esp32c3)");
    kernels();
    embuild::espidf::sysenv::output();
}
