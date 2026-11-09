#!/usr/bin/env python3
"""Build stable installer plus an opt-in beta installer/compact discovery pointer."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote
from build_output import staged_directory
from make_installer import build_installer
from release_version import Version, select_release


def download_release_assets(tag, folder, repository, run=None, sleep=None):
    run = subprocess.run if run is None else run
    sleep = time.sleep if sleep is None else sleep
    command = ['gh', 'release', 'download', tag, '--repo', repository, '--dir', str(folder),
               '--pattern', '*-factory.bin', '--pattern', 'boards.json', '--pattern', 'update.json',
               '--clobber']
    for attempt in range(4):
        result = run(command, capture_output=True, text=True)
        if result.returncode == 0:
            return
        error = subprocess.CalledProcessError(result.returncode, command,
                                              output=result.stdout, stderr=result.stderr)
        transient = re.search(r'\bHTTP 5\d\d\b', result.stderr or '')
        if not transient or attempt == 3:
            if result.stderr:
                print(result.stderr, end='', file=sys.stderr)
            raise error
        delay = 2 ** attempt
        print(f"GitHub asset download returned {transient.group()}; retrying in {delay}s "
              f"({attempt + 1}/3)", file=sys.stderr)
        sleep(delay)


def validate_manifest(manifest, repository, tag, boards):
    if manifest.get('tag_name') != tag:
        raise ValueError('Channel manifest release mismatch')
    expected = {env + suffix for env in boards for suffix in ('-firmware.bin', '-firmware.bin.sig')}
    assets = manifest.get('assets', [])
    if len(assets) != len(expected) or {a.get('name') for a in assets} != expected:
        raise ValueError('Channel requires one firmware/signature pair per board')
    base = f'https://github.com/{repository}/releases/download/{quote(tag, safe="")}/'
    bases = {base}
    # Old immutable manifests retain their upload-time repository name after a rename.
    if repository == 'bdlglua/BDLMinerV2' and Version(tag) <= Version('2.0.0'):
        bases.add(f'https://github.com/bdlglua/BDLMinerV1/releases/download/{quote(tag, safe="")}/')
    if any(a.get('browser_download_url') not in {prefix + a['name'] for prefix in bases} for a in assets):
        raise ValueError('Channel assets must belong to the selected repository release')


def build_channels(releases, fetch, output, repository):
    stable, beta = (select_release(releases, channel) for channel in ('stable', 'beta'))
    if stable is None:
        raise ValueError('A published stable release is required for the default installer')
    beta = beta or stable
    with tempfile.TemporaryDirectory(prefix='bdl-channels-') as temp:
        paths = {}
        for item in (stable, beta):
            tag = item.get('tag_name', item.get('tagName'))
            version = Version(tag).text
            if tag not in paths:
                folder = Path(temp) / version
                folder.mkdir()
                fetch(tag, folder)
                manifest = json.loads((folder / 'update.json').read_text())
                boards = json.loads((folder / 'boards.json').read_text())
                validate_manifest(manifest, repository, tag, boards)
                # Publish canonical links without modifying an archived release asset.
                for asset in manifest['assets']:
                    asset['browser_download_url'] = f'https://github.com/{repository}/releases/download/{quote(tag, safe="")}/' + asset['name']
                (folder / 'update.json').write_text(json.dumps(manifest, separators=(',', ':')))
                paths[tag] = folder
        stable_tag = stable.get('tag_name', stable.get('tagName'))
        beta_tag = beta.get('tag_name', beta.get('tagName'))
        channels = {name: {'version': Version(item.get('tag_name', item.get('tagName'))).text,
                           'published_at': item.get('published_at')}
                    for name, item in (('stable', stable), ('beta', beta))}
        build_installer(str(paths[stable_tag]), str(output), 'stable', channels)
        build_installer(str(paths[beta_tag]), str(Path(output) / 'beta'), 'beta', channels)
        pointers = Path(output) / 'updates'
        pointers.mkdir(exist_ok=True)
        for channel, tag in (('stable', stable_tag), ('beta', beta_tag)):
            (pointers / (channel + '.json')).write_bytes((paths[tag] / 'update.json').read_bytes())
    return stable_tag, beta_tag


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repository', required=True)
    p.add_argument('--output', type=Path, default=Path('site'))
    args = p.parse_args()
    if not re.fullmatch(r'[\w.-]+/[\w.-]+', args.repository):
        p.error('Use owner/repository')
    listing = subprocess.run(['gh', 'api', '--paginate', f'repos/{args.repository}/releases?per_page=100',
                              '--jq', '.[] | {tag_name,draft,prerelease,published_at}'], check=True, capture_output=True, text=True)
    releases = [json.loads(line) for line in listing.stdout.splitlines() if line.strip()]
    def fetch(tag, folder):
        download_release_assets(tag, folder, args.repository)
    with staged_directory(args.output) as staged:
        stable, beta = build_channels(releases, fetch, staged, args.repository)
    print(f'Installer stable: {stable}; opt-in beta: {beta}')


if __name__ == '__main__':
    main()
