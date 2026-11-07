#!/usr/bin/env python3
"""Create an offline Linux test kit from verified firmware. Never publishes or flashes."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import zipfile
from build_output import staged_directory
from firmware_image import inspect_factory, verify_signature
import make_release
from release_version import Version

ROOT=Path(__file__).resolve().parents[1]
DEFAULT_BOARDS=('m5stickc-plus2','bitaxe-gamma-601')
TOOLS=('bdlhub.py','calibrate_gamma.py','check_fleet.py','check_hardware.py',
       'hardware_suite.py','trace_hardware.py','monitor_network.py','reset_serial.py','firmware_image.py','release_version.py')
PACKAGE=('__init__.py','client.py','hub.py','calibration.py','example.json',
         'web/index.html','web/app.js','web/style.css','web/icon.svg')
DOCS=('v2/README.md','v2/implementation.md','audits/2026-11-02-v2-beta-readiness.md',
      'remote-access.md','remote-access.fr.md',
      'v2/hardware-validation.md','v2/hardware-validation.fr.md',
      'v2/repository-migration.md',
      'audits/2026-11-02-resumable-sha.md',
      'audits/2026-11-02-board-hashrate-research.md',
      'benchmarks/v2-native-sha256d-2026-11-02.json',
      'releases/v2.0.0-beta.2.md','releases/v2.0.0-beta.2.fr.md',
      'releases/v2.0.0-beta.3.md','releases/v2.0.0-beta.3.fr.md',
      'releases/v2.0.0-beta.4.md','releases/v2.0.0-beta.4.fr.md',
      'releases/v2.0.0.md','releases/v2.0.0.fr.md',
      'releases/v2.0.1.md','releases/v2.0.1.fr.md',
      'releases/v2.0.2.md','releases/v2.0.2.fr.md',
      'releases/v2.0.3.md','releases/v2.0.3.fr.md',
      'patches/README.md','patches/BDLMiner-stats-v2-semver.patch')


def build_kit(release,output,boards=DEFAULT_BOARDS):
    release,output=Path(release),Path(output)
    version=Version(release.name).text
    chips=json.loads((release/'boards.json').read_text())
    sums={line.split('  ')[1]:line.split('  ')[0] for line in (release/'SHA256SUMS.txt').read_text().splitlines()}
    images={}
    if not boards or len(set(boards))!=len(boards):raise ValueError('Choose unique supported boards')
    for env in boards:
        if env not in chips or env not in make_release.envs():raise ValueError('Missing or unsupported board: '+env)
        firmware=release/(env+'-firmware.bin');factory=release/(env+'-factory.bin')
        for image in (firmware,factory):
            if image.is_symlink() or image.stat().st_size>32*1024*1024:raise ValueError('Invalid firmware input')
            if hashlib.sha256(image.read_bytes()).hexdigest()!=sums.get(image.name):raise ValueError('Firmware checksum mismatch: '+image.name)
        slot=inspect_factory(factory.read_bytes(),firmware.read_bytes(),env,version,chips[env])
        signature=firmware.with_name(firmware.name+'.sig')
        if signature.is_symlink():raise ValueError('Invalid signature input')
        if signature.exists():verify_signature(firmware,signature)
        images[env]={'chip':chips[env],'ota_slot_bytes':slot,'firmware_bytes':firmware.stat().st_size,
                     'sha256':sums[firmware.name],'signed':signature.exists()}
    (output/'firmware').mkdir(parents=True)
    for env in boards:
        for suffix in ('-firmware.bin','-factory.bin','-firmware.bin.sig'):
            image=release/(env+suffix)
            if image.exists():shutil.copyfile(image,output/'firmware'/image.name)
    (output/'tools').mkdir()
    for name in TOOLS:shutil.copyfile(ROOT/'tools'/name,output/'tools'/name)
    for name in PACKAGE:
        destination=output/'tools/bdlhost'/name;destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(ROOT/'tools/bdlhost'/name,destination)
    (output/'src').mkdir();shutil.copyfile(ROOT/'src/update_key.h',output/'src/update_key.h')
    for name in DOCS:
        destination=output/'docs'/name;destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(ROOT/'docs'/name,destination)
    shutil.copyfile(ROOT/'tools/bdlhost/example.json',output/'bdlhub.local.json')
    source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    dirty=bool(subprocess.check_output(['git','status','--porcelain','--','tools/bdlhost','src/update_key.h',
                                       'docs/v2',*['tools/'+name for name in TOOLS]],cwd=ROOT,text=True))
    metadata={'kind':'bdlminer-linux-test-kit','firmware_version':version,'tool_source_commit':source,
              'tool_source_dirty':dirty,'boards':images,'automatic_install':False}
    (output/'kit.json').write_text(json.dumps(metadata,indent=2)+'\n')
    for name,command in (
        ('start-hub.sh','python3 tools/bdlhub.py --config bdlhub.local.json'),
        ('check-miners.sh','python3 tools/check_fleet.py --config bdlhub.local.json --expected '+shlex.quote(version)+' --output bdlminer-v2-check.json')):
        target=output/name
        target.write_text('#!/bin/sh\nset -eu\nCDPATH= cd -- "$(dirname -- "$0")"\nexec '+command+' "$@"\n')
        target.chmod(0o755)
    (output/'README.txt').write_text(f'''BDLMiner {version} — Linux test kit

1. Extract the complete archive. Python 3.10+ is the only host requirement.
2. Edit bdlhub.local.json if your miner addresses differ from the supplied M5
   (192.168.1.40) and Gamma (192.168.1.135).
3. Run: sh check-miners.sh
   This only reads statistics. "review"/exit 2 means that a miner is offline,
   unhealthy, still on trial or running a different firmware; inspect the JSON.
4. Run: sh start-hub.sh
   Open http://127.0.0.1:8787. Ctrl+C stops the hub gracefully.

Firmware files are in firmware/. An OTA upload uses the matching *-firmware.bin
in the miner's existing local Settings page. A *-factory.bin is for USB recovery
and must not be uploaded as OTA. No script in this kit installs firmware.
Check kit.json for signature availability. Unsigned development images cannot
be installed by the automatic GitHub updater or the signed-OTA bench command.
Keep your current known-good firmware and a private settings backup for testing.

SHA256SUMS.txt covers all kit files. Run: sha256sum -c SHA256SUMS.txt
It detects archive corruption; it is not a release signature.
The source commit in kit.json describes the host tools. Firmware version,
board, hashes, chip, factory application and OTA capacity are independently
checked when this kit is built. Real hashrate/cooling/OTA remain hardware tests.
See docs/v2/README.md for passwords, calibration, rollback and hardware testing.
The four hardware checks, including rollback BEFORE confirming the M5 update,
are described in docs/v2/hardware-validation.md (French: .fr.md).
reset_serial.py is an active hardware reset helper for an identified USB-UART
port. ota-monitor only invokes it when --reset-command is explicitly supplied.
''')
    (output/'LIRE-MOI.txt').write_text(f'''BDLMiner {version} — kit de test Linux

Décompresse le dossier complet. Python 3.10 ou plus récent suffit.
Les adresses du M5 (192.168.1.40) et du Gamma (192.168.1.135) sont déjà dans
bdlhub.local.json ; modifie ce fichier si nécessaire.

Vérification en lecture seule : sh check-miners.sh
Le rapport est bdlminer-v2-check.json. « review » (code 2) indique qu'une carte
est inaccessible, en essai, à examiner ou sur une version différente.

Dashboard central : sh start-hub.sh
Ouvre ensuite http://127.0.0.1:8787. Ctrl+C arrête le hub proprement.
Pour vérifier l'archive : sha256sum -c SHA256SUMS.txt

Les firmwares sont dans firmware/. Pour l'OTA manuelle, choisis uniquement le
*-firmware.bin de ta carte depuis ses réglages locaux. Le *-factory.bin sert
à l'installation/récupération USB. Aucun script ne flashe automatiquement.
Les images de développement non signées ne passent pas par l'OTA GitHub
automatique ; la présence d'une signature est indiquée dans kit.json.
Conserve ton firmware fonctionnel et une sauvegarde privée des réglages.
Les essais réels de débit, de chauffe et d'OTA restent à faire sur les cartes.
Pour les quatre validations, commence par docs/v2/hardware-validation.fr.md :
le test de rollback du M5 précède la confirmation de sa mise à jour.
reset_serial.py provoque un reset matériel du port USB-UART choisi explicitement.
L'observateur OTA ne le lance que si --reset-command est fourni.
''')
    checks=[]
    for path in sorted(output.rglob('*')):
        if path.is_file():checks.append(hashlib.sha256(path.read_bytes()).hexdigest()+'  '+path.relative_to(output).as_posix())
    (output/'SHA256SUMS.txt').write_text('\n'.join(checks)+'\n')
    return metadata


def archive_kit(folder,destination):
    folder=Path(folder);destination=Path(destination);destination.parent.mkdir(parents=True,exist_ok=True)
    files={}
    for line in (folder/'SHA256SUMS.txt').read_text().splitlines():
        digest,name=line.split('  ')
        if (not re.fullmatch('[0-9a-f]{64}',digest) or '\\' in name or Path(name).is_absolute() or
                '..' in Path(name).parts or name in files or name=='SHA256SUMS.txt'):
            raise ValueError('Invalid test-kit checksum manifest')
        files[name]=digest
    files['SHA256SUMS.txt']=None
    with tempfile.NamedTemporaryFile(dir=destination.parent,delete=False) as tmp:temporary=Path(tmp.name)
    try:
        with zipfile.ZipFile(temporary,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
            for name,digest in sorted(files.items()):
                file=folder/name
                if not file.resolve().is_relative_to(folder.resolve()):raise ValueError('File leaves the test-kit directory')
                data=file.read_bytes()
                if digest and hashlib.sha256(data).hexdigest()!=digest:raise ValueError('Test-kit checksum mismatch: '+name)
                info=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0))
                info.external_attr=(0o100755 if file.suffix=='.sh' else 0o100644)<<16
                archive.writestr(info,data,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
        temporary.replace(destination)
    finally:temporary.unlink(missing_ok=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--release',type=Path,default=ROOT/'release'/make_release.version())
    p.add_argument('--output',type=Path,default=ROOT/'.pio/v2-test-kit')
    p.add_argument('--boards',nargs='+',default=list(DEFAULT_BOARDS))
    p.add_argument('--archive',type=Path)
    args=p.parse_args()
    if args.output.resolve()==args.release.resolve() or args.output.resolve() in args.release.resolve().parents:
        p.error('Kit output must not contain its firmware inputs')
    if args.archive and (args.archive.resolve()==args.output.resolve() or args.output.resolve() in args.archive.resolve().parents):
        p.error('Archive must be outside the kit directory')
    with staged_directory(args.output) as staging:metadata=build_kit(args.release,staging,args.boards)
    if args.archive:archive_kit(args.output,args.archive)
    print(f"Linux test kit {metadata['firmware_version']}: {len(metadata['boards'])} verified boards; {args.output}")


if __name__=='__main__':main()
