//! Opt-in remote monitoring identifiers. No raw MAC or pool host leaves; the
//! payout address is only sent as the Authorization of address-based spaces.

use crate::sha256::sha256;
use crate::work::to_hex;

pub fn valid_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn device_id(key: &str, mac: u64) -> String {
    let mut input = Vec::with_capacity(72);
    input.extend_from_slice(&key.as_bytes()[..64.min(key.len())]);
    input.extend_from_slice(&mac.to_le_bytes());
    to_hex(&sha256(&input)[..8])
}

/// The payout part of a Stratum user ("address.worker" -> "address").
pub fn payout_address(user: &str) -> Option<String> {
    let user = user.trim_start_matches([' ', '\t']);
    let end = user.find(['.', ' ', '\t']).unwrap_or(user.len());
    let address = &user[..end];
    if address.len() > 100
        || !address.bytes().all(|c| c.is_ascii_alphanumeric() || c == b':' || c == b'_' || c == b'-')
        || address.len() < 4
    {
        return None;
    }
    Some(address.to_owned())
}

pub fn address_device_id(address: &str, mac: u64) -> String {
    let mut input = b"address:".to_vec();
    input.extend(address.bytes().take(100).map(|c| c.to_ascii_lowercase()));
    input.extend_from_slice(&mac.to_le_bytes());
    to_hex(&sha256(&input)[..8])
}

/// Remote monitoring can run when a key or a usable payout address names its space.
pub fn can_report(key: &str, user: &str) -> bool {
    valid_key(key) || payout_address(user).is_some()
}
