//! Bounded SemVer ordering, including historical two-component releases.

use std::cmp::Ordering;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub core: [u32; 3],
    pub pre: String,
}

fn identifiers(p: &str, leading_zero: bool) -> bool {
    !p.is_empty()
        && p.split('.').all(|id| {
            !id.is_empty()
                && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                && (leading_zero || !(id.bytes().all(|c| c.is_ascii_digit()) && id.len() > 1 && id.starts_with('0')))
        })
}

pub fn parse(text: &str) -> Option<Version> {
    if text.len() > 63 {
        return None;
    }
    let mut p = text.strip_prefix(['v', 'V']).unwrap_or(text);
    let mut v = Version::default();
    let mut parts = 0;
    while parts < 3 {
        let digits = p.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || (digits > 1 && p.starts_with('0')) {
            return None;
        }
        v.core[parts] = p[..digits].parse().ok()?;
        p = &p[digits..];
        parts += 1;
        if !p.starts_with('.') {
            break;
        }
        if parts == 3 {
            return None;
        }
        p = &p[1..];
    }
    if parts < 2 || !(p.is_empty() || p.starts_with(['-', '+'])) {
        return None;
    }
    if let Some(rest) = p.strip_prefix('-') {
        let end = rest.find('+').unwrap_or(rest.len());
        let pre = &rest[..end];
        if pre.len() >= 32 || !identifiers(pre, false) {
            return None;
        }
        v.pre = pre.to_owned();
        p = &rest[end..];
    }
    if let Some(build) = p.strip_prefix('+') {
        if !identifiers(build, true) {
            return None;
        }
    }
    Some(v)
}

/// -1, 0 or 1; 0 also when either side does not parse.
pub fn compare(left: &str, right: &str) -> i32 {
    let (Some(a), Some(b)) = (parse(left), parse(right)) else { return 0 };
    let ord = a.core.cmp(&b.core).then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let mut x = a.pre.split('.');
            let mut y = b.pre.split('.');
            loop {
                match (x.next(), y.next()) {
                    (None, None) => break Ordering::Equal,
                    (Some(_), None) => break Ordering::Greater,
                    (None, Some(_)) => break Ordering::Less,
                    (Some(p), Some(q)) => {
                        let pn = p.bytes().all(|c| c.is_ascii_digit());
                        let qn = q.bytes().all(|c| c.is_ascii_digit());
                        let o = match (pn, qn) {
                            (true, false) => Ordering::Less,
                            (false, true) => Ordering::Greater,
                            (true, true) => p.len().cmp(&q.len()).then_with(|| p.cmp(q)),
                            (false, false) => p.cmp(q),
                        };
                        if o != Ordering::Equal {
                            break o;
                        }
                    }
                }
            }
        }
    });
    ord as i32
}

/// The first version in a text such as a release name ("BDLMiner 2.6.1 (beta)").
pub fn find(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let end = text[i..].find([' ', ')', ']']).map(|e| i + e).unwrap_or(text.len());
        let candidate = &text[i..end];
        if candidate.len() < 64 && parse(candidate).is_some() {
            return Some(candidate.to_owned());
        }
        i = end.max(i + 1);
    }
    None
}
