"""Package the actual V2 browser interfaces with isolated, fictional data."""
from pathlib import Path
import html
import json
import shutil


def build_demos(root, output, version):
    root, output = Path(root), Path(output)
    scripts = root / 'installer' / 'demo'
    policy = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'none'; form-action 'none'; base-uri 'self'"
    for mode in ('miner', 'hub'):
        folder = output / 'demo' / mode
        folder.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(scripts / 'demo.js', folder / 'demo.js')
        shutil.copyfile(scripts / 'demo.css', folder / 'demo.css')
        setup = json.dumps({'mode': mode, 'version': version}).replace('<', '\\u003c')
        head = f'<meta http-equiv="Content-Security-Policy" content="{html.escape(policy, quote=True)}"><script>window.BDL_DEMO={setup};const LANG=new URLSearchParams(location.search).get("lang")==="fr"?"fr":"en";</script><script src="demo.js"></script>'
        if mode == 'miner':
            source = root / 'web'
            for name in ('style.css', 'common.js', 'dashboard.js', 'settings.js', 'log.js'):
                shutil.copyfile(source / name, folder / name)
            header = (source / 'header.html').read_text()
            for old, new in (('/settings', 'settings.html'), ('/log', 'log.html'), ('/', 'index.html')):
                header = header.replace(f'href="{old}"', f'href="{new}"')
            for page, file in (('dashboard', 'index.html'), ('settings', 'settings.html'), ('log', 'log.html')):
                body = header + (source / (page + '.html')).read_text() + '</div></div>'
                for old, new in (('/settings', 'settings.html'), ('/log', 'log.html'), ('/', 'index.html')):
                    body = body.replace(f'href="{old}"', f'href="{new}"').replace(f'href="{old}#', f'href="{new}#')
                page_head = head + '<link rel="icon" href="../../assets/icon.svg" type="image/svg+xml"><link rel="stylesheet" href="style.css"><link rel="stylesheet" href="demo.css"><script defer src="common.js"></script>' + f'<script defer src="{page}.js"></script>'
                (folder / file).write_text(f'<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>BDLMiner V2 · Demo</title>{page_head}</head><body>{body}</body></html>', encoding='utf-8')
        else:
            source = root / 'tools' / 'bdlhost' / 'web'
            for name in ('style.css', 'app.js', 'icon.svg'):
                shutil.copyfile(source / name, folder / name)
            page = (source / 'index.html').read_text().replace('href="/icon.svg"', 'href="icon.svg"').replace('href="/style.css"', 'href="style.css"').replace('src="/app.js"', 'src="app.js"').replace('href="/"', 'href="index.html"')
            page = page.replace('</head>', head + '<link rel="stylesheet" href="demo.css"></head>')
            (folder / 'index.html').write_text(page, encoding='utf-8')
