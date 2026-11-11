//! Chain-aware payout address checks (Base58Check, Bech32/Bech32m and
//! CashAddr), to warn about a mistyped address instead of mining for nothing.

use crate::coin::Coin;
use crate::sha256::sha256d;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Type {
    #[default]
    Invalid,
    P2pkh,
    P2sh,
    P2wpkh,
    P2wsh,
    P2tr,
    SegwitOther,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::P2pkh => "P2PKH",
            Type::P2sh => "P2SH",
            Type::P2wpkh => "P2WPKH",
            Type::P2wsh => "P2WSH",
            Type::P2tr => "P2TR",
            Type::SegwitOther => "SegWit",
            Type::Invalid => "invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Network {
    #[default]
    Unknown,
    Main,
    Test,
    Regtest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Info {
    pub kind: Type,
    pub network: Network,
}

impl Info {
    fn new(kind: Type, network: Network) -> Self {
        Info { kind, network }
    }
    pub fn valid(&self) -> bool {
        self.kind != Type::Invalid
    }
}

pub(crate) const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BECH32: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn bech32_value(c: u8) -> Option<u8> {
    BECH32.iter().position(|&x| x == c).map(|p| p as u8)
}

/// Legacy address: 25 bytes = version + hash160 + 4-byte sha256d checksum.
fn parse_base58(s: &str, coin: Coin) -> Option<Info> {
    if !(26..=35).contains(&s.len()) {
        return None;
    }
    let mut num = [0u8; 25];
    for c in s.bytes() {
        let digit = BASE58.iter().position(|&x| x == c)? as u32;
        let mut carry = digit;
        for byte in num.iter_mut().rev() {
            carry += 58 * *byte as u32;
            *byte = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return None;
        }
    }
    // Each leading '1' encodes one leading zero byte, and only those do.
    let ones = s.bytes().take_while(|&c| c == b'1').count();
    let zeros = num.iter().take_while(|&&b| b == 0).count();
    if ones != zeros || sha256d(&num[..21])[..4] != num[21..] {
        return None;
    }
    use Network::*;
    use Type::*;
    let info = match (coin, num[0]) {
        (Coin::Dgb, 30) => Info::new(P2pkh, Main),
        (Coin::Dgb, 5 | 63) => Info::new(P2sh, Main),
        (Coin::Dgb, 126) => Info::new(P2pkh, Test),
        (Coin::Dgb, 140) => Info::new(P2sh, Test),
        (Coin::Dgb, _) => return None,
        (Coin::Ppc, 55) => Info::new(P2pkh, Main),
        (Coin::Ppc, 117) => Info::new(P2sh, Main),
        (Coin::Ppc, 111) => Info::new(P2pkh, Test),
        (Coin::Ppc, 196) => Info::new(P2sh, Test),
        (Coin::Ppc, _) => return None,
        (_, 0x00) => Info::new(P2pkh, Main),
        (_, 0x05) => Info::new(P2sh, Main),
        (_, 0x6f) => Info::new(P2pkh, Test),
        (_, 0xc4) => Info::new(P2sh, Test),
        _ => return None,
    };
    Some(info)
}

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for &v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ff_ffff) << 5) ^ v as u32;
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 != 0 {
                chk ^= g;
            }
        }
    }
    chk
}

/// Lowercases an address, refusing mixed case and non-printable characters.
fn single_case(addr: &str) -> Option<String> {
    let (mut lower, mut upper) = (false, false);
    for c in addr.bytes() {
        if !(33..=126).contains(&c) {
            return None;
        }
        lower |= c.is_ascii_lowercase();
        upper |= c.is_ascii_uppercase();
    }
    (!(lower && upper)).then(|| addr.to_ascii_lowercase())
}

/// SegWit address (BIP173 / BIP350).
fn parse_segwit(addr: &str, coin: Coin) -> Option<Info> {
    if addr.len() > 90 {
        return None;
    }
    let s = single_case(addr)?;
    let sep = s.rfind('1')?;
    if sep == 0 || sep + 8 > s.len() {
        return None;
    }
    let hrp = &s[..sep];
    let (main, test, reg) = match coin {
        Coin::Dgb => ("dgb", "dgbt", "dgbrt"),
        Coin::Ppc => ("pc", "tpc", "pcrt"),
        _ => ("bc", "tb", "bcrt"),
    };
    let net = if hrp == main {
        Network::Main
    } else if hrp == test || (coin == Coin::Ppc && hrp == "tb") {
        Network::Test
    } else if hrp == reg {
        Network::Regtest
    } else {
        return None;
    };
    let data: Vec<u8> = s[sep + 1..].bytes().map(bech32_value).collect::<Option<_>>()?;
    let mut values: Vec<u8> = hrp.bytes().map(|c| c >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|c| c & 31));
    values.extend_from_slice(&data);
    let version = data[0];
    if version > 16 || polymod(&values) != if version == 0 { 1 } else { 0x2bc830a3 } {
        return None;
    }
    // Witness program: 5-bit groups (minus version and checksum) -> bytes.
    let mut program = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &d in &data[1..data.len() - 6] {
        acc = ((acc << 5) | d as u32) & 0xfff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            program.push((acc >> bits) as u8);
        }
    }
    if bits >= 5 || acc & ((1 << bits) - 1) != 0 || !(2..=40).contains(&program.len()) {
        return None;
    }
    let kind = match (version, program.len()) {
        (0, 20) => Type::P2wpkh,
        (0, 32) => Type::P2wsh,
        (0, _) => return None,
        (1, 32) => Type::P2tr,
        _ => Type::SegwitOther,
    };
    Some(Info::new(kind, net))
}

/// CashAddr: verifies the prefix, 40-bit checksum, version, hash length and
/// zero padding. Never corrects a mistyped payout address.
fn parse_cash_addr(addr: &str, coin: Coin) -> Option<Info> {
    if addr.is_empty() || addr.len() > 124 {
        return None;
    }
    let text = single_case(addr)?;
    let (prefix, payload) = match text.find(':') {
        Some(colon) => (&text[..colon], &text[colon + 1..]),
        None => (coin.cash_prefix(0)?, text.as_str()),
    };
    let network = [Network::Main, Network::Test, Network::Regtest]
        .into_iter()
        .enumerate()
        .find(|(i, _)| coin.cash_prefix(*i) == Some(prefix))?
        .1;
    if !(42..=112).contains(&payload.len()) {
        return None;
    }
    const GENERATOR: [u64; 5] = [0x98f2bc8e61, 0x79b76d99e2, 0xf33e5fb3c4, 0xae2eabe2a8, 0x1e4f43e470];
    let mut check: u64 = 1;
    let mut step = |value: u8| {
        let top = check >> 35;
        check = ((check & 0x07_ffff_ffff) << 5) ^ value as u64;
        for (i, g) in GENERATOR.iter().enumerate() {
            if top & (1 << i) != 0 {
                check ^= g;
            }
        }
    };
    for c in prefix.bytes() {
        step(c & 31);
    }
    step(0);
    let mut decoded = Vec::with_capacity(65);
    let (mut acc, mut bits) = (0u32, 0u32);
    let payload = payload.as_bytes();
    for (i, &c) in payload.iter().enumerate() {
        let value = bech32_value(c)?;
        step(value);
        if i + 8 >= payload.len() {
            continue;
        }
        acc = ((acc << 5) | value as u32) & 0xfff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            if decoded.len() == 65 {
                return None;
            }
            decoded.push((acc >> bits) as u8);
        }
    }
    if check != 1 || bits >= 5 || (acc << (8 - bits)) & 255 != 0 || decoded.is_empty() {
        return None;
    }
    const SIZES: [usize; 8] = [20, 24, 28, 32, 40, 48, 56, 64];
    let version = decoded[0];
    if version & 0x80 != 0 || decoded.len() != SIZES[(version & 7) as usize] + 1 || (version >> 3) > 1 {
        return None;
    }
    Some(Info::new(if version >> 3 != 0 { Type::P2sh } else { Type::P2pkh }, network))
}

/// `user` is the Stratum user name: an address, optionally followed by
/// ".workerName". Only the address part is checked.
pub fn parse(user: &str, coin: Coin) -> Info {
    let addr = user.split('.').next().unwrap_or("");
    if coin.cash_prefix(0).is_some() {
        if let Some(info) = parse_cash_addr(addr, coin) {
            return info;
        }
    }
    let segwit = matches!(coin, Coin::Btc | Coin::Dgb | Coin::Ppc);
    segwit.then(|| parse_segwit(addr, coin)).flatten().or_else(|| parse_base58(addr, coin)).unwrap_or_default()
}
