//! Stratum V2 authority keys: checked Base58Check or 64 hex digits.

use crate::address::BASE58;
use crate::sha256::sha256d;
use crate::work::from_hex_array;

pub fn authority_key(text: &str) -> Option<[u8; 32]> {
    if text.len() == 64 {
        return from_hex_array::<32>(text);
    }
    if !(48..=55).contains(&text.len()) || text.starts_with('1') {
        return None;
    }
    let mut decoded = [0u8; 38];
    for c in text.bytes() {
        let mut carry = BASE58.iter().position(|&x| x == c)? as u32;
        for byte in decoded.iter_mut().rev() {
            carry += 58 * *byte as u32;
            *byte = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return None;
        }
    }
    if decoded[0] != 1 || decoded[1] != 0 || sha256d(&decoded[..34])[..4] != decoded[34..] {
        return None;
    }
    decoded[2..34].try_into().ok()
}
