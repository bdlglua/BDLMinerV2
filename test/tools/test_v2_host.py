"""V2 host regressions against loopback HTTP and controlled clocks, not hardware."""
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import struct
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
import ssl
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools'))
from bdlhost.client import Client, Device, DeviceError, sanitize
from bdlhost.calibration import Calibrator
from bdlhost.hub import Fleet, handler
from firmware_image import CHIPS, ENV_CHIPS, inspect_app
from assemble_release import assemble
import make_release
import hardware_suite


def reading():
    return {'api_version': 2, 'board': 'Gamma', 'firmware': '2.0.0-beta.1', 'firmware_env': 'bitaxe-gamma-601',
            'boot_id': 'test-boot', 'uptime_s': 500, 'total_hashes': 5e14, 'hashrate': 1e12,
            'temp_c': 53, 'power_percent': 100, 'limitation_reason': 'full', 'firmware_trial': False,
            'shares': {'accepted': 10, 'rejected': 1, 'work': 100, 'wallet_secret': 'HIDDEN'},
            'pool': {'connected': True, 'difficulty': 1000, 'password': 'HIDDEN', 'address': 'HIDDEN'},
            'asic': {'model': 'BM1370', 'running': True, 'frequency_mhz': 525, 'input_v': 5.2,
                     'core_v': 1.145, 'current_a': 14, 'regulator_temp_c': 58, 'regulator_status': 0,
                     'fan_rpm': 8000, 'hardware_error_percent': .08, 'stock_fallback': False},
            'ssid': 'HIDDEN', 'address': 'HIDDEN', 'webpass': 'HIDDEN'}


class Clock:
    value = 0
    def now(self):
        return self.value
    def wait(self, seconds):
        self.value += seconds


class FakeGamma:
    def __init__(self, clock):
        self.clock = clock
        self.values = {'asicmhz': 525, 'fanauto': False, 'fanpct': 91, 'maxtemp': 65}
        self.writes = []
        self.hot = False
    def settings(self):
        return {'values': self.values.copy()}
    def patch(self, changes):
        self.writes.append(changes.copy())
        self.values.update(changes)
    def stats(self):
        s = reading()
        s['uptime_s'] += self.clock.value
        s['total_hashes'] += self.clock.value * 1e12
        s['asic']['frequency_mhz'] = self.values['asicmhz']
        if self.hot and self.clock.value > 10:
            s['temp_c'] = 64
        return s


def app(env, version):
    header = bytearray(24)
    header[0], header[1], header[23] = 0xe9, 1, 1
    struct.pack_into('<H', header, 12, CHIPS[ENV_CHIPS[env]])
    payload = struct.pack('<I', 0xabcd5432) + env.encode() + b'\0BDLMinerV1/' + version.encode() + b'\0'
    data = bytes(header) + struct.pack('<II', 0x3f400000, len(payload)) + payload
    checksum = 0xef
    for b in payload:
        checksum ^= b
    data += b'\0' * ((15 - len(data)) % 16) + bytes([checksum])
    return data + hashlib.sha256(data).digest()


def partial(root, env, version):
    root.mkdir(parents=True)
    firmware = app(env, version)
    factory = bytearray(b'\xff' * (0x10000 + len(firmware)))
    for index, (offset, subtype) in enumerate(((0x10000, 0x10), (0x1f0000, 0x11))):
        struct.pack_into('<HBBII16sI', factory, 0x8000 + 32 * index, 0x50aa, 0, subtype, offset,
                         0x1e0000, b'ota', 0)
    factory[0x10000:] = firmware
    (root / (env + '-firmware.bin')).write_bytes(firmware)
    (root / (env + '-factory.bin')).write_bytes(factory)
    (root / 'boards.json').write_text(json.dumps({env: ENV_CHIPS[env]}))
    make_release.write_checksums(str(root))


class HostTests(unittest.TestCase):
    def test_sanitizer_excludes_secrets_nested_and_invalid_numbers(self):
        raw = reading()
        raw['hashrate_live'] = float('nan')
        safe = sanitize(raw)
        self.assertNotIn('HIDDEN', json.dumps(safe))
        self.assertIsNone(safe['hashrate_live'])
        raw['total_hashes'] = -1
        with self.assertRaises(DeviceError):
            sanitize(raw)

    def test_config_rejects_embedded_credentials_and_paths(self):
        for url in ('http://user:secret@miner', 'file:///tmp/miner', 'http://miner/path', 'http://miner?key=secret'):
            with self.assertRaises(ValueError):
                Device('gamma', 'Gamma', url)

    def test_calibration_recommends_measured_rate_and_restores_manual_fan(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        runner = Calibrator(gamma, (525, 550), seconds=120, clock=clock.now, wait=clock.wait)
        report = runner.run()
        self.assertEqual(report['status'], 'completed')
        self.assertTrue(report['restored'])
        self.assertEqual(report['recommended_mhz'], 525)
        self.assertEqual(len(report['trials']), 2)
        self.assertEqual(gamma.values['fanauto'], False)
        self.assertEqual(gamma.values['fanpct'], 91)
        self.assertEqual(gamma.values['asicmhz'], 525)
        self.assertFalse(any('voltage' in changes for changes in gamma.writes))

    def test_calibration_monitors_warmup_and_restores_on_heat_or_cancel(self):
        for mode in ('heat', 'cancel'):
            clock = Clock()
            gamma = FakeGamma(clock)
            gamma.hot = mode == 'heat'
            cancel = threading.Event()
            def progress(event):
                if mode == 'cancel' and event['stage'] == 'warmup':
                    cancel.set()
            result = Calibrator(gamma, (550,), seconds=120, cancel=cancel, progress=progress,
                                clock=clock.now, wait=clock.wait).run()
            self.assertEqual(result['status'], 'cancelled' if mode == 'cancel' else 'failed')
            self.assertTrue(result['restored'])
            self.assertLess(clock.value, 60)
            self.assertEqual(gamma.values['asicmhz'], 525)

    def test_calibration_restore_failure_is_not_success(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        original = gamma.patch
        def write(changes):
            if changes.get('fanauto') is False:
                raise DeviceError('unreachable')
            original(changes)
        gamma.patch = write
        report = Calibrator(gamma, (550,), seconds=120, clock=clock.now, wait=clock.wait).run()
        self.assertEqual(report['status'], 'failed')
        self.assertFalse(report['restored'])
        self.assertIn('restore_error', report)

    def test_unsafe_or_unconfirmed_gamma_never_starts_trials(self):
        for key in ('firmware_trial', 'missing_temperature', 'fan'):
            clock = Clock()
            gamma = FakeGamma(clock)
            raw = gamma.stats()
            if key == 'firmware_trial': raw[key] = True
            elif key == 'missing_temperature': raw['temp_c'] = None
            else: raw['asic']['fan_rpm'] = 0
            gamma.stats = lambda: copy.deepcopy(raw)
            result = Calibrator(gamma, clock=clock.now, wait=clock.wait).run()
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(gamma.writes, [])

    def test_durable_checkpoint_recovers_an_interrupted_trial_on_restart(self):
        with tempfile.TemporaryDirectory() as tmp:
            device=Device('gamma','Gamma','http://127.0.0.1')
            database=Path(tmp)/'recovery.sqlite'
            old=Fleet([device],database)
            baseline={'asicmhz':525,'fanauto':False,'fanpct':91}
            old.checkpoint('gamma',baseline)
            old.close()
            fleet=Fleet([device],database)
            gamma=FakeGamma(Clock())
            gamma.values['asicmhz']=600
            fleet.clients['gamma']=gamma
            try:
                fleet.start()
                fleet.workers.shutdown(wait=True)
                self.assertEqual(gamma.values['asicmhz'],525)
                self.assertTrue(fleet.jobs['gamma']['report']['restored'])
                self.assertEqual(fleet.db.execute('SELECT COUNT(*) FROM calibration_recovery').fetchone()[0],0)
            finally:
                fleet.close()

    def test_recovery_never_follows_a_reused_miner_id_to_another_url(self):
        with tempfile.TemporaryDirectory() as tmp:
            database=Path(tmp)/'recovery.sqlite'
            old=Fleet([Device('gamma','Gamma','http://127.0.0.1')],database)
            old.checkpoint('gamma',{'asicmhz':525,'fanauto':True,'fanpct':100})
            old.close()
            fleet=Fleet([Device('gamma','Gamma','http://127.0.0.2')],database)
            gamma=FakeGamma(Clock())
            fleet.clients['gamma']=gamma
            try:
                fleet.start()
                self.assertEqual(fleet.jobs['gamma']['report']['status'],'failed')
                self.assertEqual(gamma.writes,[])
            finally:
                fleet.close()

    def test_fleet_preserves_offline_history_without_aggregating_stale_rate(self):
        with tempfile.TemporaryDirectory() as tmp:
            fleet = Fleet([Device('m5', 'M5', 'http://127.0.0.1')], Path(tmp) / 'history.sqlite')
            try:
                fleet.clients['m5'].stats = lambda: sanitize(reading())
                fleet.poll_one('m5')
                self.assertEqual(fleet.snapshot()['hashrate_hs'], 1e12)
                with patch.object(fleet.clients['m5'], 'stats', side_effect=DeviceError('offline')):
                    fleet.poll_one('m5')
                state = fleet.snapshot()
                self.assertEqual(state['hashrate_hs'], 0)
                self.assertFalse(state['miners'][0]['online'])
                self.assertIn('sample', state['miners'][0])
                self.assertEqual(len(fleet.history('m5', 24)['points']), 1)
                self.assertNotIn('HIDDEN', json.dumps(state))
            finally:
                fleet.close()

    def test_late_poll_body_or_error_cannot_replace_a_newer_applied_setting(self):
        for failed in (False,True):
            with tempfile.TemporaryDirectory() as tmp:
                fleet=Fleet([Device('m5','M5','http://127.0.0.1')],Path(tmp)/'data.sqlite')
                started=threading.Event();release=threading.Event()
                old=sanitize(reading());old['power_limit_percent']=100
                fresh=copy.deepcopy(old);fresh['power_limit_percent']=65
                def delayed():
                    started.set();release.wait(3)
                    if failed:raise DeviceError('old error')
                    return old
                fleet.clients['m5'].stats=delayed
                thread=threading.Thread(target=fleet.poll_one,args=('m5',));thread.start()
                try:
                    self.assertTrue(started.wait(1))
                    fleet.clients['m5'].stats=lambda:fresh
                    fleet.poll_one('m5');release.set();thread.join(3)
                    sample=fleet.snapshot()['miners'][0]
                    self.assertTrue(sample['online']);self.assertEqual(sample['sample']['power_limit_percent'],65)
                finally:
                    release.set();thread.join(3);fleet.close()

    def test_hub_auth_origin_host_allowlist_and_real_control_forwarding(self):
        with tempfile.TemporaryDirectory() as tmp:
            fleet = Fleet([Device('m5', 'M5', 'http://127.0.0.1')], Path(tmp) / 'data.sqlite')
            hosts = set()
            server = ThreadingHTTPServer(('127.0.0.1', 0), handler(fleet, 'test-token', hosts, True))
            url = f'http://127.0.0.1:{server.server_port}'
            hosts.add(f'127.0.0.1:{server.server_port}')
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            writes = []
            fleet.clients['m5'].patch = lambda changes: writes.append(changes)
            fleet.clients['m5'].stats = lambda: sanitize(reading())
            def request(path, data=None, headers=None):
                req = urllib.request.Request(url + path, data=json.dumps(data).encode() if data else None,
                                             headers={'Content-Type': 'application/json', **(headers or {})})
                return urllib.request.urlopen(req, timeout=3)
            try:
                self.assertEqual(request('/').status, 200)  # LAN login page remains accessible.
                with self.assertRaises(urllib.error.HTTPError) as failure:
                    request('/api/fleet')
                self.assertEqual(failure.exception.code, 401)
                auth = {'Authorization': 'Bearer test-token'}
                with self.assertRaises(urllib.error.HTTPError) as failure:
                    request('/api/action', {'miner': 'm5', 'action': 'power', 'value': 60}, auth)
                self.assertEqual(failure.exception.code, 403)
                self.assertEqual(request('/api/action', {'miner': 'm5', 'action': 'power', 'value': 60},
                                         {**auth, 'Origin': url}).status, 200)
                self.assertEqual(writes, [{'power': 60}])
                with self.assertRaises(urllib.error.HTTPError):
                    request('/api/fleet', headers={**auth, 'Host': 'attacker.example'})
                self.assertEqual(writes, [{'power': 60}])
            finally:
                server.shutdown()
                server.server_close()
                thread.join()
                fleet.close()

    def test_client_refuses_redirect_before_credentials_can_leave_device(self):
        class Redirect(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(302)
                self.send_header('Location', 'http://example.invalid/credentials')
                self.end_headers()
            def log_message(self, *_): pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), Redirect)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with self.assertRaises(DeviceError):
                Client(Device('m5', 'M5', f'http://127.0.0.1:{server.server_port}')).stats()
        finally:
            server.shutdown(); server.server_close(); thread.join()

    def test_client_transport_errors_are_classified_without_disclosing_details(self):
        for error, code, retryable in (
                (TimeoutError('HIDDEN'), 'timeout', True),
                (urllib.error.URLError(TimeoutError('HIDDEN')), 'timeout', True),
                (ConnectionResetError('HIDDEN'), 'connection', True),
                (urllib.error.URLError(OSError('HIDDEN')), 'connection', True),
                (urllib.error.URLError(ssl.SSLCertVerificationError('HIDDEN')), 'tls', False),
                (urllib.error.HTTPError('http://miner/HIDDEN', 403, 'HIDDEN', {}, None), 'http_403', False)):
            with self.subTest(code=code, error=type(error).__name__):
                client = Client(Device('gamma', 'Gamma', 'http://miner'))
                client.opener = Mock()
                client.opener.open.side_effect = error
                with self.assertRaises(DeviceError) as failure:
                    client.stats()
                self.assertEqual(failure.exception.code, code)
                self.assertEqual(failure.exception.retryable, retryable)
                self.assertNotIn('HIDDEN', str(failure.exception))
                self.assertEqual(client.opener.open.call_count, 1)

    def test_bench_detects_counter_reset_and_invalid_gamma_sensor(self):
        a = sanitize(reading())
        b = copy.deepcopy(a)
        b['uptime_s'] += 120
        b['total_hashes'] += 120e12
        self.assertEqual(hardware_suite.counter_measure(a, b)['hashrate_hs'], 1e12)
        b['boot_id'] = 'other-boot'
        with self.assertRaises(ValueError): hardware_suite.counter_measure(a, b)
        a['asic']['fan_rpm'] = 0
        self.assertFalse(hardware_suite.healthy(a))

    def test_bench_retains_the_failed_sample_and_health_reason(self):
        for field, value, reason in (
                ('fan_rpm', 0, 'Fan RPM unavailable or below 500'),
                ('input_v', 5.5, 'Input voltage unavailable or outside bench limits'),
                ('core_v', .8, 'Core voltage unavailable or outside bench limits'),
                ('pool_connected', False, 'Pool disconnected')):
            with self.subTest(field=field):
                clock = Clock()
                gamma = FakeGamma(clock)
                original = gamma.stats
                def sample():
                    data = sanitize(original())
                    if clock.value >= 10:
                        if field == 'pool_connected':
                            data['pool']['connected'] = value
                        else:
                            data['asic'][field] = value
                    return data
                gamma.stats = sample
                with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
                    report = hardware_suite.measure_counters(gamma, 3600)
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['failure_stage'], 'sample_health')
                self.assertEqual(report['issues'], [reason])
                self.assertEqual(report['elapsed_s'], 10)
                self.assertEqual(report['samples'], 3)
                self.assertTrue(hardware_suite.healthy(report['first']))
                self.assertEqual(report['last']['uptime_s'], 510)
                self.assertNotIn('HIDDEN', json.dumps(report))

    def test_bench_explains_failure_before_the_measurement_starts(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        sample = sanitize(reading())
        sample['asic']['regulator_status'] = 4
        gamma.stats = lambda: sample
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 3600)
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['failure_stage'], 'initial_health')
        self.assertEqual(report['issues'], ['Regulator shutdown fault bits set or status unavailable'])
        self.assertEqual(report['elapsed_s'], 0)
        self.assertEqual(report['samples'], 1)
        self.assertEqual(report['first']['asic']['regulator_status'], 4)

    def test_bench_matches_firmware_regulator_fault_bits_and_rejects_invalid_status(self):
        # PMBus STATUS_WORD shutdown bits used by the powered Gamma firmware.
        faults = {0x0040, 0x0020, 0x0010, 0x0008, 0x0004}
        for index in range(16):
            bit = 1 << index
            with self.subTest(bit=bit):
                sample = sanitize(reading())
                sample['asic']['regulator_status'] = bit
                self.assertEqual(hardware_suite.healthy(sample), bit not in faults)
        for invalid in (None, True, False, '2', 2.0, -1, 65536):
            with self.subTest(invalid=invalid):
                sample = sanitize(reading())
                sample['asic']['regulator_status'] = invalid
                self.assertFalse(hardware_suite.healthy(sample))

    def test_bench_retains_cml_as_a_warning_without_rejecting_healthy_mining(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        original = gamma.stats
        def sample():
            data = sanitize(original())
            data['asic']['regulator_status'] = 2
            return data
        gamma.stats = sample
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 60)
        self.assertEqual(report['status'], 'passed')
        self.assertEqual(report['regulator_warning_bits'], 2)
        self.assertTrue(report['warnings'])
        self.assertEqual(report['last']['asic']['regulator_status'], 2)
        self.assertEqual(report['hashrate_hs'], 1e12)

    def test_bench_distinguishes_transport_failure_from_unhealthy_hardware(self):
        for fail_after in (0, 10):
            with self.subTest(fail_after=fail_after):
                clock = Clock()
                gamma = FakeGamma(clock)
                original = gamma.stats
                def sample():
                    if clock.value >= fail_after:
                        raise DeviceError('Miner is unreachable or timed out')
                    return sanitize(original())
                gamma.stats = sample
                with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
                    report = hardware_suite.measure_counters(gamma, 3600)
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['error_type'], 'DeviceError')
                self.assertEqual(report['failure_stage'], 'initial_read' if fail_after == 0 else 'sample_read')
                self.assertNotIn('issues', report)
                if fail_after:
                    self.assertTrue(hardware_suite.healthy(report['last']))
                    self.assertEqual(report['last']['uptime_s'], 505)
                else:
                    self.assertIsNone(report['last'])
                    self.assertEqual(report['samples'], 0)

    def test_bench_retains_the_new_boot_on_counter_failure(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        original = gamma.stats
        def sample():
            data = sanitize(original())
            if clock.value >= 10:
                data['boot_id'] = 'restarted'
            return data
        gamma.stats = sample
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 3600)
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['failure_stage'], 'counter_check')
        self.assertEqual(report['first']['boot_id'], 'test-boot')
        self.assertEqual(report['last']['boot_id'], 'restarted')

    def test_bench_success_keeps_the_real_device_counter_rate(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 60)
        self.assertEqual(report['status'], 'passed')
        self.assertEqual(report['device_seconds'], 60)
        self.assertEqual(report['hashrate_hs'], 1e12)
        self.assertNotIn('error', report)

    def test_bench_recovers_a_transient_read_but_marks_the_completed_window_for_review(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        original = gamma.stats
        failures = []
        def sample():
            if clock.value >= 10 and not failures:
                failures.append(True)
                raise DeviceError('Miner request timed out', code='timeout', retryable=True)
            return sanitize(original())
        gamma.stats = sample
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 60)
        self.assertEqual(report['status'], 'review')
        self.assertGreaterEqual(report['device_seconds'], 60)
        self.assertEqual(report['hashrate_hs'], 1e12)
        self.assertEqual(report['read_error_count'], 1)
        self.assertEqual(report['read_recoveries'], 1)
        self.assertEqual(report['read_errors'][0]['code'], 'timeout')
        self.assertEqual(gamma.writes, [])

    def test_bench_bounds_read_retries_and_preserves_partial_progress_on_failure(self):
        for retries in (0, 2):
            with self.subTest(retries=retries):
                clock = Clock()
                gamma = FakeGamma(clock)
                original = gamma.stats
                def sample():
                    if clock.value >= 10:
                        raise DeviceError('Miner connection failed', code='connection', retryable=True)
                    return sanitize(original())
                gamma.stats = sample
                with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
                    report = hardware_suite.measure_counters(gamma, 3600, retries)
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['failure_stage'], 'sample_read')
                self.assertEqual(report['read_error_count'], retries + 1)
                self.assertEqual(report['elapsed_s'], 10 + retries * 2)
                self.assertEqual(report['last']['uptime_s'], 505)
                self.assertEqual(report['observed_counter_window']['device_seconds'], 5)
                self.assertEqual(report['observed_counter_window']['hashrate_hs'], 1e12)

    def test_bench_recovery_never_hides_a_reboot_or_health_fault(self):
        for problem in ('reboot', 'counter_reset', 'fan'):
            with self.subTest(problem=problem):
                clock = Clock()
                gamma = FakeGamma(clock)
                original = gamma.stats
                failed = []
                def sample():
                    if clock.value >= 10 and not failed:
                        failed.append(True)
                        raise DeviceError('Miner request timed out', code='timeout', retryable=True)
                    data = sanitize(original())
                    if clock.value >= 12:
                        if problem == 'reboot':
                            data['boot_id'] = 'rebooted'
                        elif problem == 'counter_reset':
                            data['total_hashes'] = 503e12  # above baseline, below the previous reading
                        else:
                            data['asic']['fan_rpm'] = 0
                    return data
                gamma.stats = sample
                with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
                    report = hardware_suite.measure_counters(gamma, 60)
                self.assertEqual(report['status'], 'failed')
                self.assertEqual(report['failure_stage'], 'sample_health' if problem == 'fan' else 'counter_check')
                self.assertEqual(report['read_error_count'], 1)

    def test_bench_does_not_retry_http_authentication_errors(self):
        clock = Clock()
        gamma = FakeGamma(clock)
        gamma.stats = Mock(side_effect=DeviceError('Miner returned HTTP 401', code='http_401'))
        with patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait):
            report = hardware_suite.measure_counters(gamma, 60)
        self.assertEqual(report['status'], 'failed')
        self.assertEqual(report['error_code'], 'http_401')
        self.assertEqual(gamma.stats.call_count, 1)

    def test_bench_cli_exits_two_when_a_counter_window_has_read_gaps(self):
        import contextlib
        import io
        clock = Clock()
        gamma = FakeGamma(clock)
        original = gamma.stats
        failed = []
        def sample():
            if clock.value >= 10 and not failed:
                failed.append(True)
                raise DeviceError('Miner request timed out', code='timeout', retryable=True)
            return sanitize(original())
        gamma.stats = sample
        gamma.diagnostic = lambda: {'compatible': True, 'code': 'ready'}
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            config = root / 'config.json'
            config.write_text(json.dumps({'miners': [{'id': 'gamma', 'name': 'Gamma', 'url': 'http://miner'}]}))
            output = root / 'report.json'
            args = ['hardware_suite.py', '--config', str(config), '--miner', 'gamma', '--seconds', '60', '--output', str(output)]
            with patch.object(sys, 'argv', args), patch.object(hardware_suite, 'Client', return_value=gamma), \
                    patch.object(hardware_suite.time, 'monotonic', clock.now), patch.object(hardware_suite.time, 'sleep', clock.wait), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as ended:
                    hardware_suite.main()
            self.assertEqual(ended.exception.code, 2)
            self.assertEqual(json.loads(output.read_text())['status'], 'review')

    def test_network_fault_always_runs_the_restore_command(self):
        clock=Clock();gamma=FakeGamma(clock)
        with patch.object(hardware_suite,'command',side_effect=[ValueError('disconnect failed'),None]) as commands,patch.object(hardware_suite.time,'monotonic',clock.now),patch.object(hardware_suite.time,'sleep',clock.wait):
            with self.assertRaisesRegex(ValueError,'disconnect failed'):
                hardware_suite.fault(gamma,'pool','disconnect-fixture','restore-fixture')
            self.assertEqual([c.args[0] for c in commands.call_args_list],['disconnect-fixture','restore-fixture'])

    def test_bench_ota_requires_a_real_trial_and_uses_physical_reset_for_rollback(self):
        with tempfile.TemporaryDirectory() as tmp:
            image=Path(tmp)/'firmware.bin'
            image.write_bytes(app('bitaxe-gamma-601','2.0.0-beta.1'))
            for mode in ('confirm','rollback'):
                initial=reading();initial['firmware']='1.9.15'
                pending=reading();pending['firmware_trial']=True;pending['boot_id']='new-boot'
                final=copy.deepcopy(pending);final['firmware_trial']=False
                if mode=='rollback':final['firmware']='1.9.15';final['boot_id']='rollback-boot'
                gamma=FakeGamma(Clock())
                samples=iter((initial,pending,final))
                gamma.stats=lambda:next(samples)
                gamma.settings=lambda:{'capabilities':{'ota':True}}
                gamma.request=lambda *a,**kw:'ok'
                with patch.object(hardware_suite,'verify_signature'),patch.object(hardware_suite.time,'sleep'),patch.object(hardware_suite,'command') as reset:
                    result=hardware_suite.ota(gamma,image,Path(tmp)/'signature','2.0.0-beta.1',mode,'physical-reset')
                self.assertEqual(result['status'],'passed')
                self.assertTrue(result['saw_pending_trial'])
                self.assertEqual(reset.call_count,mode=='rollback')


class ArtifactTests(unittest.TestCase):


    def test_missing_board_never_enters_the_central_signing_step(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            partial(root/'inputs'/'m5stickc-plus2','m5stickc-plus2','2.0.0-beta.1')
            (root/'output').mkdir()
            with patch.object(make_release,'sign') as signing:
                with self.assertRaisesRegex(ValueError,'Missing boards'):
                    assemble(root/'inputs',root/'output','2.0.0-beta.1','unused-test-key')
                signing.assert_not_called()

    def test_all_parallel_images_are_verified_before_unsigned_assembly(self):
        version = '2.0.0-beta.1'
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for env in make_release.envs():
                partial(root / 'inputs' / env, env, version)
            output = root / 'out'
            output.mkdir()
            self.assertEqual(assemble(root / 'inputs', output, version), 12)
            self.assertEqual(len(json.loads((output / 'boards.json').read_text())), 12)
            self.assertEqual(json.loads((output / 'update.json').read_text())['assets'], [])
            target = root / 'inputs' / 'm5stickc-plus2' / 'm5stickc-plus2-firmware.bin'
            target.write_bytes(target.read_bytes() + b'corrupt')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                assemble(root / 'inputs', output, version)

    def test_app_rejects_wrong_architecture_and_corrupt_digest(self):
        binary = app('m5stickc-plus2', '2.0.0-beta.1')
        inspect_app(binary, 'm5stickc-plus2', '2.0.0-beta.1')
        with self.assertRaises(ValueError): inspect_app(binary, 'bitaxe-gamma-601', '2.0.0-beta.1')
        with self.assertRaises(ValueError): inspect_app(binary[:-1] + b'x', 'm5stickc-plus2', '2.0.0-beta.1')



if __name__ == '__main__':
    unittest.main()
