//! Stratum V2 Noise_NX initiator (Noise_NX_Secp256k1+EllSwift_ChaChaPoly_SHA256).
//! Transport independent; the pool's authority key is mandatory.

use chacha20poly1305::aead::{AeadInOut, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, Tag};
use secp256k1::ellswift::{ElligatorSwift, Party};
use secp256k1::{schnorr, PublicKey, SecretKey, XOnlyPublicKey};

use crate::sha256::sha256;

pub const HELLO_LEN: usize = 64;
pub const RESPONSE_LEN: usize = 234;
pub const TAG_LEN: usize = 16;
/// Largest plaintext in one encrypted record.
pub const MAX_RECORD: usize = 65519;
const PROTOCOL_NAME: &[u8] = b"Noise_NX_Secp256k1+EllSwift_ChaChaPoly_SHA256";

// All handshake hashes use the portable software kernel: they cannot contend
// with the miner's hardware SHA peripheral.
fn mix(hash: &mut [u8; 32], data: &[u8]) {
    let mut bytes = hash.to_vec();
    bytes.extend_from_slice(data);
    *hash = sha256(&bytes);
}

pub(crate) fn hmac(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(64 + data.len().max(32));
    buf.extend((0..64).map(|i| if i < 32 { key[i] } else { 0 } ^ 0x36));
    buf.extend_from_slice(data);
    let inner = sha256(&buf);
    buf.clear();
    buf.extend((0..64).map(|i| if i < 32 { key[i] } else { 0 } ^ 0x5c));
    buf.extend_from_slice(&inner);
    let out = sha256(&buf);
    buf.fill(0);
    out
}

/// Noise HKDF with two outputs.
pub(crate) fn hkdf(ck: &[u8; 32], input: &[u8]) -> ([u8; 32], [u8; 32]) {
    let prk = hmac(ck, input);
    let one = hmac(&prk, &[1]);
    let mut message = [0u8; 33];
    message[..32].copy_from_slice(&one);
    message[32] = 2;
    let two = hmac(&prk, &message);
    (one, two)
}

pub(crate) fn nonce(counter: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_le_bytes());
    Nonce::from(n)
}

pub(crate) fn seal(key: &[u8; 32], counter: u64, ad: &[u8], plain: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let mut buf = plain.to_vec();
    let tag = cipher.encrypt_inout_detached(&nonce(counter), ad, buf.as_mut_slice().into()).expect("bounded record");
    buf.extend_from_slice(&tag);
    buf
}

pub(crate) fn open(key: &[u8; 32], counter: u64, ad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < TAG_LEN {
        return None;
    }
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let (body, tag) = sealed.split_at(sealed.len() - TAG_LEN);
    let tag = Tag::try_from(tag).ok()?;
    let mut buf = body.to_vec();
    cipher.decrypt_inout_detached(&nonce(counter), ad, buf.as_mut_slice().into(), &tag).ok()?;
    Some(buf)
}

/// Fills a buffer with cryptographically secure random bytes.
pub type Random<'a> = &'a mut dyn FnMut(&mut [u8]);

pub struct Noise {
    authority: [u8; 32],
    secret: Option<SecretKey>,
    ephemeral: [u8; 64],
    h: [u8; 32],
    ck: [u8; 32],
    tx: [u8; 32],
    rx: [u8; 32],
    tx_nonce: u64,
    rx_nonce: u64,
    ready: bool,
    waiting: bool,
    error: &'static str,
}

impl Default for Noise {
    fn default() -> Self {
        Noise {
            authority: [0; 32],
            secret: None,
            ephemeral: [0; 64],
            h: [0; 32],
            ck: [0; 32],
            tx: [0; 32],
            rx: [0; 32],
            tx_nonce: 0,
            rx_nonce: 0,
            ready: false,
            waiting: false,
            error: "Noise not established",
        }
    }
}

impl Drop for Noise {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl Noise {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error(&self) -> &'static str {
        self.error
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    fn wipe(&mut self) {
        if let Some(mut s) = self.secret.take() {
            s.non_secure_erase();
        }
        self.ck = [0; 32];
        self.tx = [0; 32];
        self.rx = [0; 32];
    }

    fn fail<T>(&mut self, message: &'static str) -> Result<T, &'static str> {
        self.ready = false;
        self.waiting = false;
        self.error = message;
        self.wipe();
        Err(message)
    }

    /// The first handshake message (our EllSwift-encoded ephemeral key).
    pub fn start(&mut self, authority: &[u8; 32], random: Random) -> Result<[u8; HELLO_LEN], &'static str> {
        self.ready = false;
        self.waiting = false;
        self.tx_nonce = 0;
        self.rx_nonce = 0;
        if XOnlyPublicKey::from_byte_array(*authority).is_err() {
            return self.fail("Invalid SV2 authority key");
        }
        self.authority = *authority;
        let mut secret = None;
        for _ in 0..8 {
            let mut bytes = [0u8; 32];
            random(&mut bytes);
            secret = SecretKey::from_secret_bytes(bytes).ok();
            bytes.fill(0);
            if secret.is_some() {
                break;
            }
        }
        let Some(secret) = secret else { return self.fail("SV2 random key generation failed") };
        let mut seed = [0u8; 32];
        random(&mut seed);
        self.ephemeral = ElligatorSwift::from_seckey(secret, Some(seed)).to_array();
        seed.fill(0);
        self.secret = Some(secret);
        self.h = sha256(PROTOCOL_NAME);
        self.ck = self.h;
        mix(&mut self.h, &[]);
        let e = self.ephemeral;
        mix(&mut self.h, &e);
        mix(&mut self.h, &[]);
        self.waiting = true;
        self.error = "Waiting for SV2 Noise response";
        Ok(self.ephemeral)
    }

    /// Checks the responder's message: its static key, then the certificate
    /// signed by the pool authority, valid at `now` (Unix time).
    pub fn finish(&mut self, response: &[u8; RESPONSE_LEN], now: u32) -> Result<(), &'static str> {
        let Some(secret) = self.secret.filter(|_| self.waiting && !self.ready) else {
            return self.fail("SV2 handshake not started");
        };
        if now < 1_700_000_000 {
            return self.fail("SV2 requires synchronized time");
        }
        let ours = ElligatorSwift::from_array(self.ephemeral);
        let re: [u8; 64] = response[..64].try_into().unwrap_or([0; 64]);
        mix(&mut self.h, &re);
        let shared = ElligatorSwift::shared_secret(ours, ElligatorSwift::from_array(re), secret, Party::Initiator);
        let (ck, mut key) = hkdf(&self.ck, shared.as_secret_bytes());
        self.ck = ck;
        let server = open(&key, 0, &self.h, &response[64..144]);
        key.fill(0);
        let Some(server) = server else { return self.fail("SV2 server key authentication failed") };
        mix(&mut self.h, &response[64..144]);
        let server: [u8; 64] = server.try_into().unwrap_or([0; 64]);
        let shared = ElligatorSwift::shared_secret(ours, ElligatorSwift::from_array(server), secret, Party::Initiator);
        let (ck, mut key) = hkdf(&self.ck, shared.as_secret_bytes());
        self.ck = ck;
        let certificate = open(&key, 0, &self.h, &response[144..234]);
        key.fill(0);
        let Some(certificate) = certificate else { return self.fail("SV2 certificate authentication failed") };
        if certificate[0] != 0 || certificate[1] != 0 {
            return self.fail("Unsupported SV2 certificate version");
        }
        let le32 = |p: &[u8]| u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        let (start, end) = (le32(&certificate[2..]), le32(&certificate[6..]));
        if start > end || now < start || now > end {
            return self.fail("SV2 certificate outside validity period");
        }
        let (server_x, _) = PublicKey::from_ellswift(ElligatorSwift::from_array(server)).x_only_public_key();
        let mut signed = [0u8; 42];
        signed[..10].copy_from_slice(&certificate[..10]);
        signed[10..].copy_from_slice(&server_x.to_byte_array());
        let digest = sha256(&signed);
        let Ok(authority) = XOnlyPublicKey::from_byte_array(self.authority) else {
            return self.fail("Invalid SV2 certificate key");
        };
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&certificate[10..74]);
        let signature = schnorr::Signature::from_byte_array(signature);
        if schnorr::verify(&signature, &digest, &authority).is_err() {
            return self.fail("SV2 authority signature mismatch");
        }
        let (tx, rx) = hkdf(&self.ck, &[]);
        (self.tx, self.rx) = (tx, rx);
        if let Some(mut s) = self.secret.take() {
            s.non_secure_erase();
        }
        self.ck = [0; 32];
        self.h = [0; 32];
        self.ready = true;
        self.waiting = false;
        self.error = "";
        Ok(())
    }

    /// One record: a frame's header and each payload chunk are separate.
    pub fn encrypt(&mut self, plain: &[u8]) -> Result<Vec<u8>, &'static str> {
        if !self.ready || plain.len() > MAX_RECORD || self.tx_nonce == u64::MAX {
            return self.fail("Invalid SV2 encrypted record");
        }
        let out = seal(&self.tx, self.tx_nonce, &[], plain);
        self.tx_nonce += 1;
        Ok(out)
    }

    pub fn decrypt(&mut self, cipher: &[u8]) -> Result<Vec<u8>, &'static str> {
        if !self.ready || cipher.len() < TAG_LEN || cipher.len() > 65535 || self.rx_nonce == u64::MAX {
            return self.fail("Invalid SV2 encrypted record");
        }
        match open(&self.rx, self.rx_nonce, &[], cipher) {
            Some(plain) => {
                self.rx_nonce += 1;
                Ok(plain)
            }
            None => self.fail("SV2 message authentication failed"),
        }
    }
}

/// The pool side of the handshake, for tests and local test pools.
pub mod responder {
    use super::*;
    use secp256k1::Keypair;

    pub struct Responder {
        pub tx: [u8; 32],
        pub rx: [u8; 32],
        tx_nonce: u64,
        rx_nonce: u64,
    }

    /// Answers `hello` with a server key `server_secret` certified by
    /// `authority` for [valid_from, valid_to].
    pub fn respond(
        hello: &[u8; 64],
        server_secret: SecretKey,
        authority: &Keypair,
        valid_from: u32,
        valid_to: u32,
        random: Random,
    ) -> ([u8; RESPONSE_LEN], Responder) {
        let mut h = sha256(PROTOCOL_NAME);
        let mut ck = h;
        mix(&mut h, &[]);
        mix(&mut h, hello);
        mix(&mut h, &[]);
        let mut bytes = [0u8; 32];
        random(&mut bytes);
        let ephemeral_secret = SecretKey::from_secret_bytes(bytes).expect("random key");
        random(&mut bytes);
        let ephemeral = ElligatorSwift::from_seckey(ephemeral_secret, Some(bytes));
        random(&mut bytes);
        let server = ElligatorSwift::from_seckey(server_secret, Some(bytes));
        let initiator = ElligatorSwift::from_array(*hello);
        let mut out = [0u8; RESPONSE_LEN];
        out[..64].copy_from_slice(&ephemeral.to_array());
        mix(&mut h, &ephemeral.to_array());
        let shared = ElligatorSwift::shared_secret(initiator, ephemeral, ephemeral_secret, Party::Responder);
        let (next, key) = hkdf(&ck, shared.as_secret_bytes());
        ck = next;
        let sealed = seal(&key, 0, &h, &server.to_array());
        out[64..144].copy_from_slice(&sealed);
        mix(&mut h, &sealed);
        let shared = ElligatorSwift::shared_secret(initiator, server, server_secret, Party::Responder);
        let (next, key) = hkdf(&ck, shared.as_secret_bytes());
        ck = next;
        let (server_x, _) = PublicKey::from_secret_key(&server_secret).x_only_public_key();
        let mut certificate = [0u8; 74];
        certificate[2..6].copy_from_slice(&valid_from.to_le_bytes());
        certificate[6..10].copy_from_slice(&valid_to.to_le_bytes());
        let mut signed = [0u8; 42];
        signed[..10].copy_from_slice(&certificate[..10]);
        signed[10..].copy_from_slice(&server_x.to_byte_array());
        let signature = schnorr::sign_no_aux_rand(&sha256(&signed), authority);
        certificate[10..].copy_from_slice(signature.as_ref());
        out[144..].copy_from_slice(&seal(&key, 0, &h, &certificate));
        // The initiator's tx is our rx.
        let (rx, tx) = hkdf(&ck, &[]);
        (out, Responder { tx, rx, tx_nonce: 0, rx_nonce: 0 })
    }

    impl Responder {
        pub fn encrypt(&mut self, plain: &[u8]) -> Vec<u8> {
            let out = seal(&self.tx, self.tx_nonce, &[], plain);
            self.tx_nonce += 1;
            out
        }
        pub fn decrypt(&mut self, cipher: &[u8]) -> Option<Vec<u8>> {
            let out = open(&self.rx, self.rx_nonce, &[], cipher)?;
            self.rx_nonce += 1;
            Some(out)
        }
    }
}
