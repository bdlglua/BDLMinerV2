//! Runs another Stratum V2 Noise initiator against this crate's responder.
//!
//!     cargo run --example noise_interop -- <initiator binary>
//!
//! The initiator takes the authority key (64 hex digits) as its argument,
//! prints its hello, reads the 234-byte response (hex lines), prints one
//! encrypted record with "hello", then decrypts the reply and prints it.
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use bdlminer_core::sv2_noise::responder;
use bdlminer_core::work::{from_hex, from_hex_array, to_hex};
use secp256k1::{Keypair, SecretKey};

fn main() {
    let program = std::env::args().nth(1).expect("initiator binary");
    let authority = Keypair::from_secret_key(&SecretKey::from_secret_bytes([7; 32]).unwrap());
    let key = to_hex(&authority.x_only_public_key().0.to_byte_array());
    let mut child = Command::new(program).arg(&key).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut input = child.stdin.take().unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let hello: [u8; 64] = from_hex_array(line.trim()).expect("hello");
    let mut seed = 0x1234_5678u64;
    let mut random = |b: &mut [u8]| {
        for x in b {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *x = (seed >> 56) as u8;
        }
    };
    let server = SecretKey::from_secret_bytes([9; 32]).unwrap();
    let (response, mut pool) =
        responder::respond(&hello, server, &authority, 1_700_000_000, 1_900_000_000, &mut random);
    writeln!(input, "{}", to_hex(&response)).unwrap();
    line.clear();
    out.read_line(&mut line).unwrap();
    let plain = pool.decrypt(&from_hex(line.trim()).expect("record")).expect("authenticated record");
    assert_eq!(plain, b"hello", "initiator record");
    writeln!(input, "{}", to_hex(&pool.encrypt(b"pool reply"))).unwrap();
    line.clear();
    out.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "pool reply");
    drop(input);
    child.wait().unwrap();
    println!("interop OK: handshake, certificate and records in both directions");
}
