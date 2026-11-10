// Anonymous hashrate sharing (settings.shareHashrate, on by default): every 5
// minutes, the firmware version, the board, the selected coin and the hashrate are posted to the
// BDLMiner statistics collector, which adds up the reports of every miner.
// Pool sharing (on by default, independently switchable) adds a public pool group. No Bitcoin
// address, worker, configured hostname, port, name or identifier is sent.
#pragma once

#include <stddef.h>

#ifndef STATS_URL
#define STATS_URL "https://api.bdl-pool.com:36018/report"
#endif

// Sends the report when it is due. Called every few seconds by the market
// task, like the other HTTPS requests.
void sharePoll();

// The JSON body sent: {"v":"1.9.4","board":"m5stack-esp32","hashrate":1.02e6,"every":300,
// "model":"M5Stack Core2","coin":"BTC"} with the hashrate rounded to 3 significant digits,
// "every" the seconds until the next report, and "model" only when the board
// tells more than its firmware (the M5Stack firmware serves several models).
// An optional pool host is reduced to a fixed public group or "other";
// nullptr omits the field entirely. Length written, 0 if `size` is too small.
size_t shareBody(char *out, size_t size, double hashrate, const char *model = nullptr,
                 const char *poolHost = nullptr, unsigned selectedCoin = 0);

// True for names equal but for case, spaces and punctuation.
bool sameName(const char *a, const char *b);
