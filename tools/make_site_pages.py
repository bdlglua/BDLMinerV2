"""Fill the public pages exported by the React/Next.js site (installer/site/) for one release channel."""
import html
import hashlib
import json
from pathlib import Path
import re


PAGES = {
    'home': ('index.html', 'Bitcoin solo mining firmware', 'Firmware de minage solo Bitcoin'),
    'start': ('start.html', 'Get started', 'Bien démarrer'),
    'boards': ('boards.html', 'Supported boards', 'Cartes compatibles'),
    'flash': ('flash.html', 'Flash your board', 'Flasher ta carte'),
    'demo': ('demo.html', 'Interactive demos', 'Démos interactives'),
    'faq': ('faq.html', 'Troubleshooting', 'Dépannage'),
    'releases': ('releases.html', 'Releases', 'Nouveautés'),
    'stats': ('stats.html', 'Live statistics', 'Statistiques en direct'),
    'gallery': ('gallery.html', 'Community', 'Communauté'),
    'support': ('support.html', 'Support BDLMiner', 'Soutenir BDLMiner'),
    'remote': ('remote.html', 'My miners', 'Mes mineurs'),
}

LEGACY_SECTIONS = {
    'top': 'index.html', 'discover': 'start.html', 'boards': 'boards.html',
    'install': 'flash.html', 'demo': 'demo.html', 'help': 'faq.html',
    'news': 'releases.html', 'stats': 'stats.html', 'gallery': 'gallery.html',
    'community': 'gallery.html', 'support': 'support.html',
}


REMOTE_REDIRECT = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="robots" content="noindex">
<meta http-equiv="refresh" content="0; url=../remote.html"><link rel="canonical" href="../remote.html">
<title>BDLMiner V2: My miners</title></head>
<body><p><a href="../remote.html">My miners / Mes mineurs</a></p></body></html>
"""


def build_pages(root, output, version, boards, content):
    root, output = Path(root), Path(output)
    asset_versions = {name: hashlib.sha256((root / 'installer/assets' / name).read_bytes()).hexdigest()[:12]
                      for name in ('site.js', 'site.css', 'discovery.js', 'discovery.css', 'remote.js', 'remote.css',
                                   'pulse-theme.css', 'pulse-chart.js', 'home-chart.js', 'navigation.js', 'vendor/chart.umd.min.js')}
    titles = {key: {'en': page[1], 'fr': page[2]} for key, page in PAGES.items()}
    for key, (filename, title, _) in PAGES.items():
        # Complete pages rendered by the React/Next.js site (site/, `npm run build`).
        page = (root / 'installer/site' / filename).read_text(encoding='utf-8')
        data = {**content, 'page': key, 'page_titles': titles, 'legacy_sections': LEGACY_SECTIONS}
        page = page.replace('__PAGE_KEY__', key)
        page = page.replace('__PAGE_TITLE__', html.escape('BDLMiner V2: ' + title))
        page = page.replace('__BOARDS__', json.dumps(boards).replace('<', '\\u003c')).replace('__VERSION__', version)
        page = page.replace('__SITE_DATA__', json.dumps(data, ensure_ascii=False).replace('<', '\\u003c'))
        for name, digest in asset_versions.items():
            page = page.replace('"assets/' + name + '"', '"assets/' + name + '?v=' + digest + '"')
        page = re.sub(r'href="#([a-z-]+)"', lambda match: 'href="' + LEGACY_SECTIONS.get(match[1], '#' + match[1]) + '"', page)
        if key != 'flash':
            page = re.sub(r'<script type="module" src="assets/vendor/esp-web-tools[^>]+></script>\n?', '', page)
        (output / filename).write_text(page, encoding='utf-8')
    # Old "My miners" address (/remote/), still used by bookmarks and the
    # statistics service: open the site page, keeping no query string.
    (output / 'remote').mkdir(exist_ok=True)
    (output / 'remote' / 'index.html').write_text(REMOTE_REDIRECT, encoding='utf-8')
