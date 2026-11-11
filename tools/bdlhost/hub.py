"""Small optional fleet service: stdlib HTTP, SQLite history, bounded polling."""
import concurrent.futures
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import signal
import sqlite3
import threading
import time
from urllib.parse import parse_qs, urlsplit
from .client import Client, DeviceError
from .calibration import Calibrator, restore_settings

ASSETS = Path(__file__).with_name('web')


class Fleet:
    def __init__(self, devices, database, interval=10, retention_days=7):
        if not 5 <= interval <= 300 or not 1 <= retention_days <= 90:
            raise ValueError('Polling must be 5–300 seconds; retention 1–90 days')
        self.devices = {d.id: d for d in devices}
        self.clients = {d.id: Client(d) for d in devices}
        self.operations = {d.id: threading.Lock() for d in devices}
        self.interval, self.retention = interval, retention_days * 86400
        self.lock = threading.RLock()
        self.stop = threading.Event()
        self.state, self.jobs = {}, {}
        self.poll_sequence = {d.id: 0 for d in devices}
        self.cancellations = {}
        self.db = sqlite3.connect(database, check_same_thread=False)
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.execute('CREATE TABLE IF NOT EXISTS readings (miner TEXT, at REAL, boot TEXT, rate REAL, temp REAL, PRIMARY KEY(miner,at))')
        self.db.execute('CREATE TABLE IF NOT EXISTS calibration_recovery (miner TEXT PRIMARY KEY, settings TEXT)')
        self.workers = concurrent.futures.ThreadPoolExecutor(max_workers=4, thread_name_prefix='bdl-control')

    def poll_one(self, miner):
        if self.stop.is_set():
            return
        with self.lock:
            self.poll_sequence[miner] += 1
            sequence = self.poll_sequence[miner]
        now = time.time()
        try:
            sample = self.clients[miner].stats()
            with self.lock:
                if sequence != self.poll_sequence[miner]:
                    return
                self.state[miner] = {'online': True, 'sample': sample, 'at': now, 'attempted_at': now}
                self.db.execute('INSERT OR REPLACE INTO readings VALUES (?,?,?,?,?)',
                                (miner, now, sample.get('boot_id'), sample['hashrate'], sample.get('temp_c')))
                self.db.commit()
        except Exception as error:
            with self.lock:
                if sequence != self.poll_sequence[miner]:
                    return
                old = self.state.get(miner, {})
                self.state[miner] = {**old, 'online': False, 'attempted_at': now,
                                     'error': str(error) if isinstance(error, DeviceError) else 'Invalid miner response'}

    def poll(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=min(4, len(self.devices))) as executor:
            while not self.stop.is_set():
                list(executor.map(self.poll_one, self.devices))
                with self.lock:
                    self.db.execute('DELETE FROM readings WHERE at < ?', (time.time() - self.retention,))
                    self.db.commit()
                self.stop.wait(self.interval)

    def snapshot(self):
        with self.lock:
            now = time.time()
            miners = []
            total = 0
            for key, device in self.devices.items():
                state = self.state.get(key, {'online': False})
                age = now - state.get('at', 0)
                stale = age > max(self.interval * 3, 30)
                if state['online'] and not stale:
                    total += state['sample']['hashrate']
                miners.append({'id': key, 'name': device.name, 'url': device.url, **state,
                               'age_s': age if 'at' in state else None, 'stale': stale,
                               'busy': self.operations[key].locked()})
            jobs = json.loads(json.dumps(self.jobs))
            return {'miners': miners, 'hashrate_hs': total, 'poll_interval_s': self.interval,
                    'recorded_at': now, 'jobs': jobs}

    def history(self, miner, hours):
        if miner not in self.devices:
            raise ValueError('Unknown miner')
        if hours not in (1, 6, 24, 168):
            raise ValueError('History window must be 1, 6, 24 or 168 hours')
        bucket = max(self.interval, hours * 3600 // 480)
        with self.lock:
            rows = self.db.execute('SELECT CAST(at / ? AS INTEGER) * ?, AVG(rate), MAX(temp), '
                                   'MIN(boot), COUNT(DISTINCT boot) FROM readings '
                                   'WHERE miner=? AND at>=? GROUP BY CAST(at / ? AS INTEGER) ORDER BY at',
                                   (bucket, bucket, miner, time.time() - hours * 3600, bucket)).fetchall()
        return {'interval_s': bucket, 'points': [{'t': r[0], 'rate': r[1], 'temp': r[2],
                                                  'boot': r[3], 'mixed_boots': r[4] > 1} for r in rows]}

    def action(self, miner, action, value=None):
        if miner not in self.devices:
            raise ValueError('Unknown miner')
        operation = self.operations[miner]
        if not operation.acquire(blocking=False):
            raise ValueError('A miner operation is already running')
        client = self.clients[miner]
        if action == 'calibrate':
            try:
                with self.lock:
                    if self.db.execute('SELECT 1 FROM calibration_recovery WHERE miner=?', (miner,)).fetchone():
                        raise ValueError('Restore the unfinished calibration before starting another trial')
                frequencies = value if isinstance(value, list) else [525, 550, 575, 600]
                cancel = threading.Event()
                def progress(event):
                    with self.lock:
                        self.jobs[miner] = {'running': event['stage'] != 'finished', **event}
                runner = Calibrator(client, frequencies=frequencies, cancel=cancel, progress=progress,
                                    checkpoint=lambda baseline: self.checkpoint(miner, baseline))
                with self.lock:
                    self.jobs[miner] = {'running': True, 'stage': 'queued'}
                self.cancellations[miner] = cancel
                def run():
                    try:
                        runner.run()
                    finally:
                        self.cancellations.pop(miner, None)
                        operation.release()
                self.workers.submit(run)
                return {'started': True}
            except Exception:
                operation.release()
                raise
        try:
            if action == 'diagnose':
                return client.diagnostic()
            if action == 'power':
                if type(value) is not int or not 0 <= value <= 100:
                    raise ValueError('Power must be 0–100%')
                client.patch({'power': value})
            elif action == 'channel':
                if value not in ('stable', 'beta'):
                    raise ValueError('Unknown update channel')
                if client.request('/api/stats?history=0').get('api_version', 0) < 2:
                    raise ValueError('Channel selection requires BDLMiner V2')
                client.patch({'channel': int(value == 'beta')})
            elif action in ('check-update', 'install-update'):
                if action == 'install-update' and client.stats().get('firmware_trial'):
                    raise ValueError('Wait for firmware trial confirmation')
                client.request('/api/github-check' if action == 'check-update' else '/api/github-update',
                               {}, method='POST', raw=True)
            else:
                raise ValueError('Unknown action')
            if action in ('power', 'channel'):
                self.poll_one(miner)  # show the applied setting rather than the preceding cached snapshot
            return {'ok': True}
        finally:
            operation.release()

    def checkpoint(self, miner, baseline):
        with self.lock:
            if baseline is None:
                self.db.execute('DELETE FROM calibration_recovery WHERE miner=?', (miner,))
            else:
                self.db.execute('INSERT OR REPLACE INTO calibration_recovery VALUES (?,?)',
                                (miner, json.dumps({'url': self.devices[miner].url, 'values': baseline}, allow_nan=False)))
            self.db.commit()

    def recover(self, miner, baseline):
        try:
            with self.lock:
                self.jobs[miner] = {'running': True, 'stage': 'restoring'}
            restore_settings(self.clients[miner], baseline)
            self.checkpoint(miner, None)
            report = {'status': 'completed', 'restored': True, 'note': 'Recovered an interrupted calibration'}
        except Exception:
            report = {'status': 'failed', 'restored': False,
                      'restore_error': 'Recovery was not verified. Keep the miner configured and restart the hub to retry.'}
        finally:
            with self.lock:
                self.jobs[miner] = {'running': False, 'stage': 'finished', 'report': report}
            self.operations[miner].release()

    def start(self):
        with self.lock:
            recovery = self.db.execute('SELECT miner, settings FROM calibration_recovery').fetchall()
        for miner, baseline in recovery:
            if miner in self.devices:
                saved = json.loads(baseline)
                if saved.get('url') != self.devices[miner].url:
                    self.jobs[miner] = {'running': False, 'stage': 'finished', 'report': {
                        'status': 'failed', 'restored': False, 'restore_error': 'Restore the original miner URL to recover its calibration'}}
                    continue
                self.operations[miner].acquire()
                self.workers.submit(self.recover, miner, saved['values'])
        self.thread = threading.Thread(target=self.poll, name='bdl-poll', daemon=True)
        self.thread.start()

    def close(self):
        self.stop.set()
        for event in tuple((self.cancellations or {}).values()):
            event.set()
        self.workers.shutdown(wait=True)
        if hasattr(self, 'thread'):
            self.thread.join(timeout=40)
        self.db.close()


def handler(fleet, token, allowed_hosts, require_token=False):
    class Handler(BaseHTTPRequestHandler):
        server_version = 'BDLMinerHub/2'

        def setup(self):
            super().setup()
            self.connection.settimeout(15)

        def log_message(self, *_):
            pass  # URL/header values can contain credentials: no request logging.

        def reply(self, code, body, content='application/json'):
            data = json.dumps(body, allow_nan=False).encode() if content == 'application/json' else body
            self.send_response(code)
            self.send_header('Content-Type', content)
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Cache-Control', 'no-store')
            self.send_header('X-Content-Type-Options', 'nosniff')
            self.send_header('Referrer-Policy', 'no-referrer')
            self.send_header('Content-Security-Policy', "default-src 'self'; style-src 'self'; script-src 'self'; frame-ancestors 'none'; base-uri 'none'")
            self.end_headers()
            self.wfile.write(data)

        def access(self, write=False):
            host = self.headers.get('Host', '')
            if host not in allowed_hosts or self.headers.get('Sec-Fetch-Site') == 'cross-site':
                self.reply(403, {'error': 'Open the configured hub address'})
                return False
            if write and self.headers.get('Origin') != 'http://' + host:
                self.reply(403, {'error': 'A same-origin request is required'})
                return False
            supplied = self.headers.get('Authorization', '').removeprefix('Bearer ')
            protected = write or (require_token and self.path.startswith('/api/'))
            # Bytes: compare_digest() raises on a non-ASCII str, which a header can hold.
            if protected and not hmac.compare_digest(supplied.encode('latin-1', 'replace'), token.encode()):
                self.reply(401, {'error': 'Hub token required'})
                return False
            return True

        def do_GET(self):
            if not self.access():
                return
            route = urlsplit(self.path)
            try:
                if route.path == '/api/session':
                    self.reply(200, {'token': token if not require_token else None})
                elif route.path == '/api/fleet':
                    self.reply(200, fleet.snapshot())
                elif route.path == '/api/history':
                    q = parse_qs(route.query)
                    self.reply(200, fleet.history(q.get('miner', [''])[0], int(q.get('hours', ['24'])[0])))
                elif route.path in ('/', '/app.js', '/style.css', '/icon.svg'):
                    name = 'index.html' if route.path == '/' else route.path[1:]
                    self.reply(200, (ASSETS / name).read_bytes(),
                               {'index.html': 'text/html; charset=utf-8', 'app.js': 'text/javascript', 'style.css': 'text/css',
                                'icon.svg': 'image/svg+xml'}[name])
                else:
                    self.reply(404, {'error': 'Not found'})
            except (ValueError, KeyError):
                self.reply(400, {'error': 'Invalid query'})

        def do_POST(self):
            if not self.access(write=True):
                return
            if self.path != '/api/action':
                self.reply(404, {'error': 'Not found'})
                return
            try:
                size = int(self.headers.get('Content-Length', 0))
                if not 0 < size <= 4096 or self.headers.get_content_type() != 'application/json':
                    raise ValueError('Send a JSON action below 4 KiB')
                data = json.loads(self.rfile.read(size))
                if not isinstance(data, dict):
                    raise ValueError('Send an action object')
                miner, action = data.get('miner'), data.get('action')
                if action == 'cancel':
                    event = (fleet.cancellations or {}).get(miner)
                    if event is None:
                        raise ValueError('No calibration is running')
                    event.set()
                    result = {'cancelling': True}
                else:
                    result = fleet.action(miner, action, data.get('value'))
                self.reply(200, result)
            except (ValueError, TypeError, KeyError, DeviceError) as error:
                self.reply(400, {'error': str(error)})
            except Exception:
                self.reply(502, {'error': 'Miner operation failed'})

    return Handler


def serve(config, devices, database, bind='127.0.0.1', port=8787):
    exposed = bind not in ('127.0.0.1', '::1', 'localhost')
    token = os.environ.get('BDLHUB_TOKEN', '')
    if exposed and len(token) < 24:
        raise ValueError('LAN access requires BDLHUB_TOKEN (at least 24 characters)')
    token = token or secrets.token_urlsafe(32)
    hosts = {f'{bind}:{port}', f'localhost:{port}', f'127.0.0.1:{port}'}
    hosts.update(config.get('allowed_hosts', []))
    fleet = Fleet(devices, database, config.get('poll_interval_s', 10), config.get('retention_days', 7))
    server = ThreadingHTTPServer((bind, port), handler(fleet, token, hosts, exposed))
    server.daemon_threads = True
    server.timeout = 15
    fleet.start()
    def stopping(*_):
        raise KeyboardInterrupt
    previous_signal = signal.signal(signal.SIGTERM, stopping)
    print(f'BDLMiner hub: http://{bind}:{port} (Ctrl+C stops and restores active calibrations)', flush=True)
    try:
        server.serve_forever(poll_interval=.5)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        fleet.close()
        signal.signal(signal.SIGTERM, previous_signal)
