#!/usr/bin/env python3
"""Reference set for the Rust mining core, computed independently of it.

Only Python's hashlib and integers are used (no code shared with
firmware-rs or src/core). Regenerate with:

    python3 firmware-rs/core/tests/fixtures/make_golden.py > firmware-rs/core/tests/fixtures/golden.json

core/tests/golden.rs checks the Rust implementation against every entry.
"""
import hashlib
import json

DIFF1 = 0xFFFF * 2 ** 208


def sha256d(b):
    return hashlib.sha256(hashlib.sha256(b).digest()).digest()


class Lcg:
    def __init__(self, seed):
        self.state = seed

    def next(self):
        self.state = (self.state * 1103515245 + 12345) & 0xFFFFFFFF
        return self.state

    def bytes(self, n):
        return bytes(self.next() >> 24 for _ in range(n))


def target(nbits):
    """Bitcoin Core's SetCompact; None for negative, zero or overflowing."""
    exp, mant = nbits >> 24, nbits & 0x007FFFFF
    if mant == 0 or nbits & 0x00800000 or exp > 34 or (mant > 0xFF and exp > 33) or (mant > 0xFFFF and exp > 32):
        return None
    value = mant >> (8 * (3 - exp)) if exp <= 3 else mant << (8 * (exp - 3))
    return value or None


def headers(rng):
    out = []
    for i in range(48):
        header = bytearray(rng.bytes(80))
        if i == 0:
            header = bytearray(80)
        if i == 1:
            header = bytearray(b"\xff" * 80)
        h = sha256d(bytes(header))
        top = int.from_bytes(h[28:32], "little")
        out.append({"header": header.hex(), "hash": h.hex(), "top32": top,
                    "difficulty": DIFF1 / int.from_bytes(h, "little") if any(h) else None})
    return out


def targets():
    out = []
    for nbits in [0x1d00ffff, 0x1a44b9f2, 0x17034219, 0x1b0404cb, 0x207fffff, 0x01010000, 0x220000ff,
                  0x2100ffff, 0x1c7fffff, 0x18009645, 0x1d80ffff, 0x23000001, 0x22000100, 0x21010000,
                  0x00000000, 0x04000000, 0x01003456]:
        t = target(nbits)
        out.append({"nbits": nbits, "target": t.to_bytes(32, "little").hex() if t else None,
                    "difficulty": DIFF1 / t if t else 0.0})
    return out


def jobs(rng):
    out = []
    for i in range(16):
        coinb1, coinb2 = rng.bytes(42 + i * 3), rng.bytes(20 + i)
        en1 = rng.bytes(i % 5)
        en2_size = 1 + i % 8
        en2 = rng.next() * 7919 + i
        branches = [rng.bytes(32) for _ in range(i % 6)]
        prev = rng.bytes(32)
        version, nbits, ntime = rng.next(), 0x1d00ffff, rng.next()
        en2_bytes = (en2 & ((1 << (8 * en2_size)) - 1)).to_bytes(en2_size, "big")
        merkle = sha256d(coinb1 + en1 + en2_bytes + coinb2)
        for b in branches:
            merkle = sha256d(merkle + b)
        header = (version.to_bytes(4, "little") + prev + merkle + ntime.to_bytes(4, "little")
                  + nbits.to_bytes(4, "little") + bytes(4))
        out.append({"coinb1": coinb1.hex(), "coinb2": coinb2.hex(), "extranonce1": en1.hex(),
                    "extranonce2_size": en2_size, "extranonce2": en2, "branches": [b.hex() for b in branches],
                    "prev_hash": prev.hex(), "version": version, "nbits": nbits, "ntime": ntime,
                    "extranonce2_hex": en2_bytes.hex(), "header": header.hex()})
    return out


rng = Lcg(0x5EED2026)
print(json.dumps({"generator": "firmware-rs/core/tests/fixtures/make_golden.py",
                  "headers": headers(rng), "targets": targets(), "jobs": jobs(rng)}, indent=1))
