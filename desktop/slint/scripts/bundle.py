#!/usr/bin/env python3
"""Package the Slint migration build independently of the existing show app."""
from pathlib import Path
import os
import plistlib
import shutil
import subprocess
import argparse
import re

parser = argparse.ArgumentParser()
parser.add_argument('--profile', choices=['debug', 'release'], default='debug')
parser.add_argument('--name', default='Overlay Studio Slint')
args = parser.parse_args()
if not re.fullmatch(r'[A-Za-z0-9 -]+', args.name):
    raise SystemExit('Use letters, numbers, spaces or hyphens for the bundle name')

root = Path(__file__).resolve().parents[1]
project = root.parents[1]
target = Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'target')))
if not target.is_absolute():
    target = project / target
app = root / 'dist' / f'{args.name}.app'
contents = app / 'Contents'
(contents / 'MacOS').mkdir(parents=True, exist_ok=True)
framework = next((target / args.profile).glob('build/syphon-core-*/out/frameworks/Syphon.framework'), None)
if framework is None:
    raise SystemExit('Build the selected profile first to obtain Syphon.framework')
shutil.copy2(target / args.profile / 'overlay-studio-slint', contents / 'MacOS/overlay-studio-slint')
(contents / 'Frameworks').mkdir(exist_ok=True)
destination = contents / 'Frameworks' / 'Syphon.framework'
if destination.exists():
    shutil.rmtree(destination)
shutil.copytree(framework, destination, symlinks=True)
with (contents / 'Info.plist').open('wb') as file:
    plistlib.dump({
        'CFBundleName': args.name,
        'CFBundleDisplayName': args.name,
        'CFBundleIdentifier': 'com.overlaystudio.' + args.name.lower().replace(' ', '-'),
        'CFBundleExecutable': 'overlay-studio-slint',
        'CFBundlePackageType': 'APPL',
        'NSHighResolutionCapable': True,
        'NSLocalNetworkUsageDescription': 'Send scoreboard graphics and connect phone or iPad controllers on your local network.',
    }, file)
subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(app)], check=True)
print(app)
