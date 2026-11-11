"""Serve both packaged installer channels on loopback, without hardware or network."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import sys
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from firmware_image import ENV_CHIPS
def build_installer(release, output, channel, channels):
    command = [os.environ.get('CARGO', 'cargo'), 'run', '--quiet', '--locked', '--manifest-path',
               str(ROOT/'site/Cargo.toml'), '--', 'installer', release, output,
               '--reuse-browser', '--channel', channel]
    metadata = Path(release)/'channels.json'
    metadata.write_text(json.dumps(channels))
    subprocess.run([*command, '--channels', str(metadata)], check=True)



class Quiet(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith('/BDLMinerV2/'):
            self.path = self.path[len('/BDLMinerV2'):]
        super().do_GET()

    def log_message(self, *_):
        pass


with tempfile.TemporaryDirectory() as temporary:
    root = Path(temporary)
    channels = {'stable': {'version': '2.0.1', 'published_at': '2026-11-02T14:00:00Z'},
                'beta': {'version': '2.0.2-beta.1', 'published_at': '2026-11-03T14:00:00Z'}}
    for channel in ('stable', 'beta'):
        version = channels[channel]['version']
        release = root / version
        release.mkdir()
        (release / 'boards.json').write_text(json.dumps(ENV_CHIPS))
        for env in ENV_CHIPS:
            if os.environ.get('BDL_FLASH_FIXTURE'):
                image=bytearray((i*37+11)%256 for i in range(12345))
                chip=ENV_CHIPS[env]
                offset=0x1000 if chip in ('esp32','esp32s2') else 0
                image[offset]=0xe9
                image[offset+12]={'esp32':0,'esp32s2':2,'esp32s3':9,'esp32c3':5}[chip]
                image[offset+13]=0
                (release / (env + '-factory.bin')).write_bytes(image)
            else:
                (release / (env + '-factory.bin')).write_bytes(b'Browser fixture; never flash')
        output = root / 'site' if channel == 'stable' else root / 'site' / 'beta'
        build_installer(str(release), str(output), channel, channels)
    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Quiet, directory=str(root / 'site')))
    print(f'http://127.0.0.1:{server.server_port}', flush=True)
    try:
        server.serve_forever()
    finally:
        server.server_close()
