// BM1397 wire vectors, and a job decoded the way the chip uses it: finishing
// SHA-256 from each midstate must give the double SHA-256 of the full header.
#include "core/bm1397.h"
#include "core/sha256.h"
#include "core/work.h"
#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>
using namespace bm1397;
static void golden(const Packet &p, const char *hex) { assert(work::toHex(p.bytes.data(), p.size) == hex); }
static uint32_t value(const Packet &p) {
    return uint32_t(p.bytes[6]) << 24 | uint32_t(p.bytes[7]) << 16 | uint32_t(p.bytes[8]) << 8 | p.bytes[9];
}
// What the chip computes for one midstate of a job packet and a nonce.
static void chipHash(const Packet &p, int index, uint32_t nonce, uint8_t out[32]) {
    const uint8_t *data = p.bytes.data() + 4;
    uint32_t state[8];
    for (int w = 0; w < 8; ++w) {
        const uint8_t *b = data + 18 + 32 * index + 4 * w;
        state[7 - w] = uint32_t(b[0]) | uint32_t(b[1]) << 8 | uint32_t(b[2]) << 16 | uint32_t(b[3]) << 24;
    }
    uint8_t block[64]{};
    std::memcpy(block, data + 14, 4);      // merkle tail
    std::memcpy(block + 4, data + 10, 4);  // ntime
    std::memcpy(block + 8, data + 6, 4);   // nbits
    std::memcpy(block + 12, &nonce, 4);
    block[16] = 0x80; block[62] = 0x02; block[63] = 0x80;  // 640 bits
    sha::transform(state, block);
    uint8_t first[32];
    for (int w = 0; w < 8; ++w) for (int b = 0; b < 4; ++b) first[4 * w + b] = uint8_t(state[w] >> (24 - 8 * b));
    sha::sha256(first, 32, out);
}
int main() {
    // Commands as the Bitaxe documentation lists them.
    golden(chainInactive(), "55aa5305000003");
    golden(readRegister(0), "55aa520500000a");
    golden(setAddress(0), "55aa400500001c");
    assert(value(maxBaud()) == 0x00006131 && maxBaud().bytes[5] == 0x18);
    assert(value(pllDivider()) == 0x0f0f0f00);
    assert(value(ticketMask(6)) == 0x000000fc && value(ticketMask(0)) == 0 && !ticketMask(32).size);

    // PLL0 as cgminer's gekko driver computes it.
    struct { float mhz, actual; uint32_t reg; } plls[] = {
        {300, 300, 0x40780215}, {210, 210, 0x40a80225}, {50, 50, 0x403c0235},
        {600, 600, 0x40780115}, {650, 650, 0x40820115}, {20, 50, 0x403c0235}, {900, 650, 0x40820115}};
    for (auto &t : plls) {
        float actual = 0;
        Packet p = frequency(t.mhz, &actual);
        assert(p.size == 11 && p.bytes[5] == 0x08 && value(p) == t.reg && std::fabs(actual - t.actual) < 0.01f);
    }
    assert(!frequency(NAN).size);

    // Version rolling inside the mask, carrying between its bits.
    assert(incrementMask(0x20000000, kVersionMask) == 0x20002000);
    assert(incrementMask(0x20002000, kVersionMask) == 0x20004000);
    assert(incrementMask(0x20002000, 0x0000a000) == 0x20008000);  // bit 13 carries into bit 15
    assert(incrementMask(0x20000000, 0) == 0x20000000);

    uint8_t header[80];
    for (int i = 0; i < 80; ++i) header[i] = uint8_t(i * 7 + 3);
    uint32_t base = 0x20000000;
    std::memcpy(header, &base, 4);
    uint32_t versions[kMidstates];
    uint8_t count = 0;
    Packet p = job(10, header, kVersionMask, versions, count);
    assert(count == 4 && p.size == 4 + 18 + 128 + 2 && p.bytes[2] == 0x21 && p.bytes[3] == 18 + 128 + 4);
    assert(p.bytes[4] == 40 && p.bytes[5] == 4);
    assert(versions[0] == 0x20000000 && versions[1] == 0x20002000 && versions[2] == 0x20004000 && versions[3] == 0x20006000);
    const uint32_t nonces[] = {0, 1, 0x12345678, 0xffffffff};
    for (int i = 0; i < 4; ++i) for (uint32_t nonce : nonces) {
        uint8_t full[80];
        std::memcpy(full, header, 80);
        std::memcpy(full, &versions[i], 4);
        std::memcpy(full + 76, &nonce, 4);
        uint8_t expected[32], chip[32];
        sha::sha256d(full, 80, expected);
        chipHash(p, i, nonce, chip);
        assert(!std::memcmp(expected, chip, 32));
    }
    // Without a rolling mask: one midstate, a shorter job.
    Packet single = job(0, header, 0, versions, count);
    assert(count == 1 && single.size == 4 + 18 + 32 + 2 && single.bytes[5] == 1 && versions[0] == base);
    assert(!job(32, header, kVersionMask, versions, count).size && count == 0);

    // Responses: chip id, a nonce for slot 10 midstate 1, resynchronized after garbage and a lost byte.
    Receiver rx;
    Response r;
    std::vector<uint8_t> stream;
    const uint8_t id[] = {0xaa, 0x55, 0x13, 0x97, 0x18, 0x00, 0x00, 0x00, 0x06};
    const uint8_t nonce[] = {0xaa, 0x55, 0x78, 0x56, 0x34, 0x12, 0x01, 41, 0x99};
    const uint8_t broken[] = {0xaa, 0x55, 0x01, 0x02};  // cut short, followed by a full frame
    stream.insert(stream.end(), {0x00, 0x13, 0xaa});
    stream.insert(stream.end(), id, id + 9);
    stream.insert(stream.end(), nonce, nonce + 9);
    std::vector<Response> got;
    for (uint8_t b : stream) if (rx.push(b, r)) got.push_back(r);
    assert(got.size() == 2 && got[0].chipId && !got[1].chipId);
    assert(got[1].nonce == 0x12345678 && got[1].slot == 10 && got[1].midstate == 1);
    rx.clear(); got.clear();
    for (uint8_t b : broken) rx.push(b, r);
    rx.clear();  // the backend clears on a timeout
    for (uint8_t b : nonce) if (rx.push(b, r)) got.push_back(r);
    assert(got.size() == 1 && got[0].slot == 10);
    // A corrupted job id (bit 7 set) names no slot: every slot a frame can give is one of the 32.
    rx.clear(); got.clear();
    for (int id = 0; id < 256; ++id) {
        uint8_t frame[] = {0xaa, 0x55, 0x78, 0x56, 0x34, 0x12, 0x01, uint8_t(id), 0x99};
        bool complete = false;
        for (uint8_t b : frame) complete = rx.push(b, r);
        assert(complete == (id < 128));
        if (complete) assert(r.slot == id / 4 && r.slot < kSlots && r.midstate == id % 4);
    }
    // The receiver stays in step after a dropped frame.
    for (uint8_t b : nonce) if (rx.push(b, r)) got.push_back(r);
    assert(got.size() == 1 && got[0].slot == 10);
    puts("BM1397: command and PLL vectors, version rolling, chip-side midstate hashing for 4 versions and 4 nonces, frames and resync passed");
}
