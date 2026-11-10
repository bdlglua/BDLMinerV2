#!/usr/bin/env python3
"""Export only anonymous public totals for the website's initial display."""
import argparse
import json
import math
import time
import urllib.request
from pathlib import Path

SUMMARY_URL = 'https://api.bdl-pool.com:36018/api/summary'
MAX_BYTES = 512 * 1024
FIELDS = ('at', 'miners', 'hashrate', 'boards', 'versions', 'coins', 'pools', 'fleet',
          'models', 'recent', 'history', 'current_hour', 'periods', 'record')
ARRAYS = ('boards', 'versions', 'coins', 'pools', 'fleet', 'models', 'recent', 'history')

def number(value):
    return type(value) in (int, float) and math.isfinite(value)

def public_summary(value, now_ms):
    if not isinstance(value, dict):
        raise ValueError('Summary must be an object')
    if (not number(value.get('at')) or not -60000 <= now_ms-value['at'] <= 5*60000
            or not number(value.get('hashrate')) or value['hashrate'] < 0
            or type(value.get('miners')) is not int or value['miners'] < 0):
        raise ValueError('Summary is invalid or too old')
    for key in ARRAYS:
        rows = value.get(key)
        if rows is not None and (not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows)):
            raise ValueError('Invalid public statistics array')
    return {key: value[key] for key in FIELDS if key in value}

def fetch_summary():
    request = urllib.request.Request(SUMMARY_URL, headers={
        'User-Agent': 'BDLMiner-public-snapshot/1', 'Cache-Control': 'no-cache',
        'Accept': 'application/json',
    })
    with urllib.request.urlopen(request, timeout=15) as response:
        if response.status != 200:
            raise ValueError('Summary did not return HTTP 200')
        raw = response.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError('Summary exceeds the public snapshot limit')
    return public_summary(json.loads(raw), time.time()*1000)

def write_summary(output, value):
    content = json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False)+'\n'
    if len(content.encode('utf-8')) > MAX_BYTES:
        raise ValueError('Public snapshot exceeds the size limit')
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name+'.tmp')
    temporary.write_text(content, encoding='utf-8')
    temporary.replace(output)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    for attempt in range(3):
        try:
            value = fetch_summary()
            write_summary(args.output, value)
            print('Public snapshot saved; reading timestamp:', value['at'])
            return 0
        except (OSError, ValueError, TypeError) as error:
            print('Public snapshot attempt', attempt+1, 'failed:', type(error).__name__)
            if attempt < 2:
                time.sleep(2**(attempt+1))
    return 1

if __name__ == '__main__':
    raise SystemExit(main())
