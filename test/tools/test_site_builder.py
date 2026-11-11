"""Installer discovery content and offline demo packaging regressions."""
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
import make_installer
from make_site_pages import PAGES


class SiteBuilderTests(unittest.TestCase):
    def test_packaged_demo_uses_the_shipped_interfaces_without_network(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release = root / '2.0.1'
            release.mkdir()
            (release / 'boards.json').write_text('{"m5stickc-plus2":"esp32"}')
            (release / 'm5stickc-plus2-factory.bin').write_bytes(b'fixture')
            output = root / 'site'
            channels = {'stable': {'version': '2.0.1', 'published_at': None},
                        'beta': {'version': '2.1.0-beta.1', 'published_at': None}}
            make_installer.build_installer(str(release), str(output), 'stable', channels)
            page = (output / 'index.html').read_text()
            home = re.search(r'<main[^>]*>(.*?)</main>', page, re.S)[1]
            for other in ('board-comparison', 'discovery-demo', 'discovery-help', 'discovery-support', 'installer'):
                self.assertNotIn('id="' + other + '"', home)
            for key, (filename, _, _) in PAGES.items():
                document = (output / filename).read_text()
                self.assertIn('data-page="' + key + '"', document)
                self.assertNotIn('__PAGE_CONTENT__', document)
                # The USB tool flashes boards: served by the site itself, never from a CDN.
                self.assertEqual('src="assets/vendor/esp-web-tools-10.4.0/install-button.js"' in document, key == 'flash')
                self.assertNotIn('unpkg.com', document)
                self.assertIsNone(re.search(r'<script[^>]+src="(?:https?:)?//', document))
                self.assertRegex(document, r'src="assets/site\.js\?v=[a-f0-9]{12}"')
                self.assertRegex(document, r'href="assets/site\.css\?v=[a-f0-9]{12}"')
            self.assertIn('href="flash.html"', home)
            payload = json.loads(re.search(r'<script id="site-data" type="application/json">(.*?)</script>', page, re.S)[1])
            self.assertEqual(payload['channels']['beta']['version'], '2.1.0-beta.1')
            self.assertEqual(payload['support']['links'][0]['url'], 'https://github.com/sponsors/bdlglua')
            for folder, source in (('miner/dashboard.js', 'web/dashboard.js'),
                                   ('miner/settings.js', 'web/settings.js'),
                                   ('hub/app.js', 'tools/bdlhost/web/app.js')):
                self.assertEqual((output / 'demo' / folder).read_bytes(), (ROOT / source).read_bytes())
            demo = (output / 'demo/miner/index.html').read_text()
            self.assertIn("connect-src &#x27;none&#x27;", demo)
            self.assertIn('href="settings.html"', demo)
            self.assertNotIn('href="/settings"', demo)
            self.assertIn('"version": "2.0.1"', demo)
            self.assertTrue((output / 'demo/hub/demo.js').is_file())

    def content_error(self, update, message):
        content = json.loads((ROOT / 'installer/content.json').read_text())
        update(content)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'installer').mkdir()
            (root / 'installer/content.json').write_text(json.dumps(content))
            with patch.object(make_installer, 'ROOT', str(root)):
                with self.assertRaisesRegex(ValueError, message):
                    make_installer.site_content('2.0.1')

    def test_donation_link_cannot_execute_script_or_embed_credentials(self):
        for url in ('javascript:alert(1)', 'http://example.com', 'https://name:secret@example.com'):
            with self.subTest(url=url):
                self.content_error(lambda c: c['support'].update(links=[{'label': 'Support', 'url': url}]), 'HTTPS without credentials')

    def test_pool_links_are_official_https_sites_without_private_or_undisclosed_groups(self):
        for pool, destination in (
            ('public-pool.io', 'javascript:alert(1)'),
            ('public-pool.io', 'http://web.public-pool.io/'),
            ('public-pool.io', 'https://name:secret@web.public-pool.io/'),
            ('public-pool.io', 'https://192.168.1.40/'),
            ('public-pool.io', 'https://public-pool.io.evil.example/'),
            ('public-pool.io', 'https://web.public-pool.io/?worker=secret'),
            ('public-pool.io', 'https://web.public-pool.io/#secret'),
            ('other', 'https://private.example/'),
            ('unknown', 'https://example.com/'),
        ):
            with self.subTest(pool=pool, destination=destination):
                self.content_error(lambda c: c.update(pool_links={pool: destination}), 'public HTTPS links')

    def test_gallery_requires_permission_and_a_local_photo(self):
        for entry, message in (
            ({'image': 'assets/example.webp', 'url': 'https://www.reddit.com/r/bdlminer/comments/example'}, 'contributor permission'),
            ({'image': '../example.webp', 'url': 'https://www.reddit.com/r/bdlminer/comments/example', 'permission': True}, 'local site assets'),
            ({'image': 'assets/example.webp', 'url': 'javascript:alert(1)', 'permission': True}, 'Reddit or Discord'),
        ):
            with self.subTest(entry=entry):
                self.content_error(lambda c: c.update(gallery=[entry]), message)

    def test_content_cannot_end_the_json_script_and_keeps_its_original_text(self):
        content = make_installer.site_content('2.0.1')
        label = '</script><script id="injected">alert(1)</script>'
        content['support']['links'][0]['label'] = {'en': label, 'fr': label}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release = root / '2.0.1'
            release.mkdir()
            (release / 'boards.json').write_text('{"m5stickc-plus2":"esp32"}')
            (release / 'm5stickc-plus2-factory.bin').write_bytes(b'fixture')
            with patch.object(make_installer, 'site_content', return_value=content):
                make_installer.build_installer(str(release), str(root / 'site'))
            page = (root / 'site/index.html').read_text()
            self.assertNotIn('<script id="injected">', page)
            payload = json.loads(re.search(r'<script id="site-data" type="application/json">(.*?)</script>', page, re.S)[1])
            self.assertEqual(payload['support']['links'][0]['label']['en'], label)


if __name__ == '__main__':
    unittest.main()
