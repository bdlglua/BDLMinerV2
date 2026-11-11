//! Ported from test/host/test_sv2_protocol.cpp, plus a full Noise_NX
//! handshake against the responder side.

use bdlminer_core::sha256::sha256d;
use bdlminer_core::stratum::Share;
use bdlminer_core::sv2_protocol::{Event, Frame, Protocol};
use bdlminer_core::work::{self, Work};

fn u(v: &mut Vec<u8>, x: u32, size: usize) {
    v.extend_from_slice(&x.to_le_bytes()[..size]);
}
fn success(flags: u32) -> Frame {
    let mut f = Frame::new(0, 1, vec![]);
    u(&mut f.payload, 2, 2);
    u(&mut f.payload, flags, 4);
    f
}
fn channel() -> Frame {
    let mut v = vec![];
    u(&mut v, 1, 4);
    u(&mut v, 7, 4);
    v.extend_from_slice(&[255; 32]);
    u(&mut v, 8, 2);
    v.extend_from_slice(&[2, 0xaa, 0xbb]);
    u(&mut v, 11, 4);
    Frame::new(0, 0x14, v)
}
fn job(id: u32, future: bool, time: u32, rolling: bool) -> Frame {
    let mut v = vec![];
    u(&mut v, 11, 4);
    u(&mut v, id, 4);
    v.push(!future as u8);
    if !future {
        u(&mut v, time, 4);
    }
    u(&mut v, 0x20000000, 4);
    v.push(rolling as u8);
    v.push(1);
    v.extend_from_slice(&[0x11; 32]);
    u(&mut v, 2, 2);
    v.extend_from_slice(&[1, 2]);
    u(&mut v, 1, 2);
    v.push(3);
    Frame::new(0x8000, 0x1f, v)
}
fn fjob(id: u32) -> Frame {
    job(id, true, 1800000000, false)
}
fn previous(id: u32) -> Frame {
    let mut v = vec![];
    u(&mut v, 11, 4);
    u(&mut v, id, 4);
    v.extend_from_slice(&[0x22; 32]);
    u(&mut v, 1800000000, 4);
    u(&mut v, 0x1d00ffff, 4);
    Frame::new(0x8000, 0x20, v)
}
fn ack(last: u32, count: u32) -> Frame {
    let mut v = vec![];
    for x in [7, last, count, 0, 0] {
        u(&mut v, x, 4);
    }
    Frame::new(0x8000, 0x1c, v)
}
fn share(w: &Work, nonce: u32) -> Share {
    let hash = w.mid.scan(nonce, u32::MAX).unwrap();
    Share {
        job_id: w.job_id.clone(),
        extranonce2: w.extranonce2_hex.clone(),
        ntime: w.ntime,
        nonce,
        version: 0x20000000,
        hash,
        difficulty: work::hash_difficulty(&hash),
        ..Default::default()
    }
}
fn opened() -> Protocol {
    let mut p = Protocol::new();
    assert_eq!(p.handle(&success(0)), Event::Setup);
    assert_eq!(p.handle(&channel()), Event::Opened);
    p
}

#[test]
fn wire_layout_jobs_and_acknowledgments() {
    let mut p = Protocol::new();
    let setup = p.setup("p", 3333, "a", "ESP32", "v", false).unwrap();
    let expected = [0, 2, 0, 2, 0, 0, 0, 0, 0, 1, b'p', 5, 13, 1, b'a', 5, b'E', b'S', b'P', b'3', b'2', 1, b'v', 0];
    assert_eq!(setup.kind, 0);
    assert_eq!(setup.payload, expected);
    assert_eq!(p.handle(&success(0)), Event::Setup);
    let open = p.open("address.worker", 1_000_000.0).unwrap();
    assert!(open.kind == 0x13 && open.payload[4] == 14 && open.payload[open.payload.len() - 2] == 8);
    assert!(p.handle(&channel()) == Event::Opened && p.opened());
    assert!(p.handle(&fjob(42)) == Event::None && !p.has_job());
    for bits in [0u32, 0x1d80ffff, 0x23000001] {
        let mut invalid = Protocol::new();
        invalid.handle(&success(0));
        invalid.handle(&channel());
        invalid.handle(&fjob(42));
        let mut frame = previous(42);
        let n = frame.payload.len();
        frame.payload[n - 4..].copy_from_slice(&bits.to_le_bytes());
        assert!(invalid.handle(&frame) == Event::Error && !invalid.has_job());
    }
    assert!(p.handle(&previous(42)) == Event::Job && p.job().clean);
    let w = work::build(p.job(), 10);
    assert!(w.extranonce2_hex == "000000000000000a" && w.exact_share_target && w.quick_limit == u32::MAX);
    let s = share(&w, 123);
    for _ in 0..3 {
        assert!(p.submit(&s).is_some());
    }
    assert_eq!(p.pending(), 3);
    let mut rejected = Frame::new(0x8000, 0x1d, vec![]);
    u(&mut rejected.payload, 7, 4);
    u(&mut rejected.payload, 1, 4);
    rejected.payload.extend_from_slice(&[3, b'b', b'a', b'd']);
    assert!(p.handle(&rejected) == Event::Rejected && p.acknowledgments().len() == 1 && p.pending() == 2);
    assert!(p.acknowledgments()[0].id == 1 && p.acknowledgments()[0].difficulty == w.share_difficulty);
    assert!(p.handle(&rejected) == Event::None && p.acknowledgments().is_empty());
    assert_eq!(p.handle(&ack(2, 3)), Event::Error);
    let mut p = {
        // The C++ test copies the protocol; replay to the same state instead.
        let mut q = opened();
        q.handle(&fjob(42));
        q.handle(&previous(42));
        for _ in 0..3 {
            q.submit(&s);
        }
        q.handle(&rejected);
        q
    };
    assert!(p.handle(&ack(2, 2)) == Event::Accepted && p.acknowledgments().len() == 2 && p.pending() == 0);
    assert!(p.acknowledgments()[0].id == 0 && p.acknowledgments()[1].id == 2);
    assert!(p.handle(&ack(2, 2)) == Event::None && p.acknowledgments().is_empty());
    assert!(p.handle(&job(43, false, 1800000001, false)) == Event::Job && !p.job().clean);
    for _ in 0..32 {
        assert!(p.submit(&s).is_some());
    }
    assert!(p.pending() == 32 && p.submit(&s).is_none());
    assert!(p.handle(&ack(34, 32)) == Event::Accepted && p.acknowledgments().len() == 32 && p.pending() == 0);
    assert!(p.submit(&s).is_some()); // previous job remains valid on the same previous block
    let mut target = Frame::new(0x8000, 0x21, vec![]);
    u(&mut target.payload, 7, 4);
    target.payload.resize(36, 0);
    target.payload[4] = 1;
    assert!(p.handle(&target) == Event::Target && p.job().exact_share_target);
    assert!(p.submit(&s).is_none()); // exact U256 recheck after a target change
}

#[test]
fn credits_follow_dynamic_targets() {
    let mut credits = opened();
    assert!(credits.handle(&fjob(1)) == Event::None && credits.handle(&previous(1)) == Event::Job);
    let original = work::build(credits.job(), 1);
    let mut candidate = share(&original, 123);
    candidate.hash = [0; 32];
    let first = credits.job().difficulty;
    assert!(credits.submit(&candidate).is_some() && credits.submission_id() == 0);
    let mut changed = Frame::new(0x8000, 0x21, vec![]);
    u(&mut changed.payload, 7, 4);
    changed.payload.resize(36, 255);
    changed.payload[35] = 0x7f;
    assert_eq!(credits.handle(&changed), Event::Target);
    let second = credits.job().difficulty;
    assert!(second > first);
    assert!(credits.submit(&candidate).is_some() && credits.submission_id() == 1);
    changed.payload[35] = 0x3f;
    assert_eq!(credits.handle(&changed), Event::Target);
    assert!(credits.handle(&ack(1, 2)) == Event::Accepted && credits.acknowledgments().len() == 2);
    assert!(credits.acknowledgments()[0].id == 0 && credits.acknowledgments()[0].difficulty == first);
    assert!(credits.acknowledgments()[1].id == 1 && credits.acknowledgments()[1].difficulty == second);
}

#[test]
fn future_jobs_survive_template_updates() {
    let mut early = opened();
    early.handle(&fjob(42));
    early.handle(&previous(42));
    assert_eq!(early.handle(&fjob(500)), Event::None);
    for i in 0..20 {
        assert_eq!(early.handle(&job(600 + i, false, 1800000002 + i, false)), Event::Job);
    }
    assert!(early.handle(&previous(500)) == Event::Job && early.job().id == "500" && early.job().clean);
    let mut two = opened();
    two.handle(&fjob(42));
    two.handle(&previous(42));
    assert!(two.handle(&fjob(700)) == Event::None && two.handle(&fjob(701)) == Event::None);
    for i in 0..20 {
        assert_eq!(two.handle(&job(800 + i, false, 1800000002 + i, false)), Event::Job);
    }
    assert!(two.handle(&previous(701)) == Event::Job && two.job().id == "701");
}

#[test]
fn explicit_ntime_jobs_keep_their_target() {
    let mut target = Frame::new(0x8000, 0x21, vec![]);
    u(&mut target.payload, 7, 4);
    target.payload.resize(36, 0);
    target.payload[4] = 1;
    let mut t = opened();
    assert!(t.handle(&fjob(1)) == Event::None && t.handle(&previous(1)) == Event::Job);
    let future = work::build(t.job(), 1);
    let future_share = share(&future, 123);
    assert_eq!(t.handle(&job(2, false, 1800000000, false)), Event::Job);
    let fixed = work::build(t.job(), 2);
    let fixed_share = share(&fixed, 123);
    assert_eq!(t.handle(&target), Event::Target);
    assert_eq!(t.job().difficulty, fixed.share_difficulty);
    assert_eq!(work::build(t.job(), 2).share_target, fixed.share_target);
    assert!(t.submit(&fixed_share).is_some());
    assert!(t.submit(&future_share).is_none());
    assert_eq!(t.handle(&job(3, false, 1800000000, false)), Event::Job);
    assert_eq!(work::build(t.job(), 3).share_target[..], target.payload[4..36]);
    assert_eq!(t.handle(&fjob(4)), Event::None);
    let mut easier = target.clone();
    easier.payload[4..36].fill(255);
    assert_eq!(t.handle(&easier), Event::Target);
    assert!(t.handle(&previous(4)) == Event::Job && t.job().share_target[31] == 255);
    assert!(t.submit(&fixed_share).is_none());
    assert!(t.handle(&target) == Event::Target && t.job().share_target[31] == 0);
    assert_eq!(work::build(t.job(), 4).share_target[..], target.payload[4..36]);
}

#[test]
fn every_truncated_frame_is_refused() {
    let mut rejected = Frame::new(0x8000, 0x1d, vec![]);
    u(&mut rejected.payload, 7, 4);
    u(&mut rejected.payload, 1, 4);
    rejected.payload.extend_from_slice(&[3, b'b', b'a', b'd']);
    let mut target = Frame::new(0x8000, 0x21, vec![]);
    u(&mut target.payload, 7, 4);
    target.payload.resize(36, 0);
    target.payload[4] = 1;
    let mut prefix = Frame::new(0x8000, 0x19, vec![]);
    u(&mut prefix.payload, 7, 4);
    prefix.payload.extend_from_slice(&[2, 0xcc, 0xdd]);
    let mut regroup = Frame::new(0, 0x25, vec![]);
    u(&mut regroup.payload, 12, 4);
    u(&mut regroup.payload, 1, 2);
    u(&mut regroup.payload, 7, 4);
    let mut close = Frame::new(0x8000, 0x18, vec![]);
    u(&mut close.payload, 99, 4);
    close.payload.push(0);
    for frame in [success(0), channel(), fjob(42), previous(42), ack(2, 2), target, rejected, prefix, regroup, close] {
        for n in 0..frame.payload.len() {
            let mut parser = opened();
            let mut cut = frame.clone();
            cut.payload.truncate(n);
            assert_eq!(parser.handle(&cut), Event::Error, "type {:02x} cut at {n}", frame.kind);
        }
    }
}

#[test]
fn cache_prefix_and_groups() {
    let mut cache = opened();
    assert_eq!(cache.handle(&previous(999)), Event::Error);
    let mut malformed = fjob(44);
    malformed.payload[8] = 2;
    assert_eq!(cache.handle(&malformed), Event::Error);
    let mut foreign = fjob(44);
    foreign.payload[0] = 99;
    assert_eq!(cache.handle(&foreign), Event::Error);
    assert_eq!(cache.handle(&fjob(44)), Event::None);
    assert_eq!(cache.handle(&fjob(44)), Event::Error);

    let mut changes = opened();
    assert_eq!(changes.handle(&fjob(1)), Event::None);
    let mut prefix = Frame::new(0x8000, 0x19, vec![]);
    u(&mut prefix.payload, 7, 4);
    prefix.payload.extend_from_slice(&[2, 0xcc, 0xdd]);
    assert!(changes.handle(&prefix) == Event::None && !changes.has_job());
    assert_eq!(changes.handle(&previous(1)), Event::Job);
    assert_eq!(changes.job().extranonce1, [0xaa, 0xbb]);
    let old = work::build(changes.job(), 11);
    let old_share = share(&old, 123);
    assert_eq!(changes.handle(&job(2, false, 1800000000, false)), Event::Job);
    assert_eq!(changes.job().extranonce1, [0xcc, 0xdd]);
    assert!(changes.submit(&old_share).is_some());
    let mut group_prefix = prefix.clone();
    group_prefix.payload[0] = 11;
    group_prefix.payload[5] = 0xee;
    assert_eq!(changes.handle(&group_prefix), Event::None);
    assert!(changes.handle(&job(3, false, 1800000000, false)) == Event::Job && changes.job().extranonce1[0] == 0xcc);
    let mut regroup = Frame::new(0, 0x25, vec![]);
    u(&mut regroup.payload, 12, 4);
    u(&mut regroup.payload, 1, 2);
    u(&mut regroup.payload, 7, 4);
    assert_eq!(changes.handle(&regroup), Event::None);
    let mut grouped = job(4, false, 1800000000, false);
    grouped.payload[0] = 12;
    assert!(changes.handle(&grouped) == Event::Job && changes.job().id == "4");
    let mut close = Frame::new(0x8000, 0x18, vec![]);
    u(&mut close.payload, 99, 4);
    close.payload.push(0);
    assert!(changes.handle(&close) == Event::None && changes.opened());
    close.payload[0] = 12;
    assert_eq!(changes.handle(&close), Event::Error);
}

#[test]
fn version_rolling_requirements() {
    let mut roll = opened();
    assert_eq!(roll.handle(&job(1, true, 1800000000, true)), Event::None);
    assert!(roll.handle(&previous(1)) == Event::Job && roll.job().version_mask == 0x1fffe000);
    let mut rw = work::build(roll.job(), 1);
    let mut rs = share(&rw, 123);
    rs.version = 0x20002000;
    rw.header[..4].copy_from_slice(&rs.version.to_le_bytes());
    rw.header[76..].copy_from_slice(&rs.nonce.to_le_bytes());
    rs.hash = sha256d(&rw.header);
    assert!(roll.submit(&rs).is_some());
    rs.version ^= 1;
    assert!(roll.submit(&rs).is_none());

    for flags in [0u32, 2] {
        let mut required = Protocol::new();
        let request = required.setup("p", 23330, "a", "ASIC", "v", true).unwrap();
        assert_eq!(request.payload[5..9], [4, 0, 0, 0]);
        assert!(required.handle(&success(flags)) == Event::Setup && required.handle(&channel()) == Event::Opened);
        assert_eq!(required.handle(&job(1, true, 1800000000, true)), Event::None);
        assert!(required.handle(&previous(1)) == Event::Job && required.job().version_mask == 0x1fffe000);
        assert_eq!(required.handle(&job(2, false, 1800000001, false)), Event::Error);
        assert!(required.version_rolling_refused() && required.job().id == "1");
    }
    for flags in [1u32, 3] {
        let mut required = Protocol::new();
        required.setup("p", 23330, "a", "ASIC", "v", true);
        assert!(required.handle(&success(flags)) == Event::Error && required.version_rolling_refused());
    }
    let mut required = Protocol::new();
    required.setup("p", 23330, "a", "ASIC", "v", true);
    let mut refused = Frame::new(0, 2, vec![]);
    u(&mut refused.payload, 4, 4);
    refused.payload.push(0);
    assert!(required.handle(&refused) == Event::Error && required.version_rolling_refused());
    let mut wrong = Protocol::new();
    wrong.setup("p", 23330, "a", "ASIC", "v", true);
    assert_eq!(wrong.handle(&success(4)), Event::Error);
    let mut fixed = Protocol::new();
    let request = fixed.setup("p", 23330, "a", "ESP32", "v", false).unwrap();
    assert!(request.payload[5] == 0 && fixed.handle(&success(1)) == Event::Setup);
    assert!(fixed.handle(&channel()) == Event::Opened && fixed.handle(&fjob(1)) == Event::None);
    assert!(fixed.handle(&previous(1)) == Event::Job && fixed.job().version_mask == 0);
    assert_eq!(fixed.handle(&job(2, true, 1800000000, true)), Event::Error);
}

#[cfg(feature = "sv2-noise")]
mod noise {
    use bdlminer_core::sv2_key::authority_key;
    use bdlminer_core::sv2_noise::{responder, Noise};
    use secp256k1::{Keypair, SecretKey};

    fn rng(seed: u64) -> impl FnMut(&mut [u8]) {
        let mut state = seed;
        move |out: &mut [u8]| {
            for b in out {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *b = (state >> 56) as u8;
            }
        }
    }

    #[test]
    fn handshake_and_records() {
        let authority = Keypair::from_secret_key(&SecretKey::from_secret_bytes([7; 32]).unwrap());
        let key = authority.x_only_public_key().0.to_byte_array();
        let server = SecretKey::from_secret_bytes([9; 32]).unwrap();
        let mut client = Noise::new();
        let mut random = rng(1);
        let hello = client.start(&key, &mut random).unwrap();
        let mut random = rng(2);
        let (response, mut pool) =
            responder::respond(&hello, server, &authority, 1_700_000_000, 1_900_000_000, &mut random);
        client.finish(&response, 1_800_000_000).unwrap();
        assert!(client.ready());
        let sealed = client.encrypt(b"setup connection").unwrap();
        assert_eq!(pool.decrypt(&sealed).unwrap(), b"setup connection");
        let reply = pool.encrypt(b"success");
        assert_eq!(client.decrypt(&reply).unwrap(), b"success");
        let mut tampered = pool.encrypt(b"x");
        tampered[0] ^= 1;
        assert!(client.decrypt(&tampered).is_err());

        // Expired certificate, wrong authority, tampered response.
        for (now, other, flip) in
            [(1_950_000_000u32, false, None), (1_800_000_000, true, None), (1_800_000_000, false, Some(150))]
        {
            let mut c = Noise::new();
            let wanted = if other {
                Keypair::from_secret_key(&SecretKey::from_secret_bytes([8; 32]).unwrap())
                    .x_only_public_key()
                    .0
                    .to_byte_array()
            } else {
                key
            };
            let hello = c.start(&wanted, &mut rng(3)).unwrap();
            let (mut response, _) =
                responder::respond(&hello, server, &authority, 1_700_000_000, 1_900_000_000, &mut rng(4));
            if let Some(i) = flip {
                response[i] ^= 1;
            }
            assert!(c.finish(&response, now).is_err());
            assert!(c.encrypt(b"no").is_err());
        }
    }

    #[test]
    fn authority_keys() {
        let hex = "f8a8cd3a3e4a3c4da6d0e1e3f5c6c1d2b2a2f2e2d2c2b2a29282726252423222";
        assert_eq!(authority_key(hex).unwrap()[0], 0xf8);
        assert!(authority_key("zz").is_none());
        assert!(authority_key(&"1".repeat(50)).is_none());
        // Stratum Reference Implementation's example authority key.
        let sri = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
        let key = authority_key(sri).expect("SRI example key");
        let mut broken = sri.to_owned();
        broken.replace_range(10..11, if &sri[10..11] == "a" { "b" } else { "a" });
        assert!(authority_key(&broken).is_none());
        assert_eq!(key.len(), 32);
    }
}
