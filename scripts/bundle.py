#!/usr/bin/env python3
"""Package an already-built macOS executable without changing system apps."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
profile = sys.argv[1] if len(sys.argv) > 1 else "release"
if profile not in ("debug", "release"):
    raise SystemExit("Choose debug or release")
target = root / "target" / profile
preview = len(sys.argv) > 2 and sys.argv[2] == "preview"
name = "Overlay Studio Preview" if preview else "Overlay Studio"
app = root / "dist" / f"{name}.app"
binary = target / "overlay-studio"
framework = next(target.glob("build/syphon-core-*/out/frameworks/Syphon.framework"), None)
if not binary.exists() or framework is None:
    raise SystemExit(f"Run cargo build {'--release' if profile == 'release' else ''} first")
(app / "Contents/MacOS").mkdir(parents=True, exist_ok=True)
(app / "Contents/Frameworks").mkdir(parents=True, exist_ok=True)
shutil.copy2(binary, app / "Contents/MacOS/overlay-studio")
destination = app / "Contents/Frameworks/Syphon.framework"
if destination.exists():
    shutil.rmtree(destination)
shutil.copytree(framework, destination, symlinks=True)
with (app / "Contents/Info.plist").open("wb") as file:
    plistlib.dump({
        "CFBundleName": name, "CFBundleDisplayName": name,
        "CFBundleIdentifier": "com.overlaystudio.preview" if preview else "com.overlaystudio.app", "CFBundleExecutable": "overlay-studio",
        "CFBundlePackageType": "APPL", "CFBundleShortVersionString": "0.1.0",
        "CFBundleVersion": "1", "NSHighResolutionCapable": True,
        "LSMinimumSystemVersion": "12.0", "NSLocalNetworkUsageDescription": "Send scoreboard graphics and connect phone or iPad controllers on your local network.",
    }, file)
subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(app)], check=True)
print(app)
