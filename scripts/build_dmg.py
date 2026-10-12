#!/usr/bin/env python3
"""Build a shippable macOS DMG with Overlay Studio.app + instruction PDF."""
from __future__ import annotations

import argparse
import plistlib
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SLINT = ROOT / "desktop" / "slint"
DEFAULT_OUT = ROOT / "dist" / "Overlay-Studio.dmg"


def run(cmd: list[str], **kwargs) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, check=True, **kwargs)


def ensure_release_binary() -> Path:
    binary = SLINT / "target" / "release" / "overlay-studio-slint"
    if not binary.is_file():
        run(["cargo", "build", "--release", "--manifest-path", str(SLINT / "Cargo.toml")])
    if not binary.is_file():
        raise SystemExit(f"Missing release binary: {binary}")
    return binary


def find_syphon(profile: str = "release") -> Path:
    matches = sorted((SLINT / "target" / profile).glob("build/syphon-core-*/out/frameworks/Syphon.framework"))
    if not matches:
        raise SystemExit("Build release first so Syphon.framework is available")
    return matches[-1]


def build_app(stage: Path, name: str = "Overlay Studio") -> Path:
    binary = ensure_release_binary()
    app = stage / f"{name}.app"
    contents = app / "Contents"
    macos = contents / "MacOS"
    frameworks = contents / "Frameworks"
    macos.mkdir(parents=True)
    frameworks.mkdir(parents=True)
    shutil.copy2(binary, macos / "overlay-studio-slint")
    dest = frameworks / "Syphon.framework"
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(find_syphon(), dest, symlinks=True)
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(
            {
                "CFBundleName": name,
                "CFBundleDisplayName": name,
                "CFBundleIdentifier": "com.overlaystudio.app",
                "CFBundleExecutable": "overlay-studio-slint",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": "0.1.0",
                "CFBundleVersion": "0.1.0",
                "LSMinimumSystemVersion": "13.0",
                "NSHighResolutionCapable": True,
                "NSLocalNetworkUsageDescription": (
                    "Send scoreboard graphics and connect phone or iPad controllers on your local network."
                ),
                "NSBonjourUsageDescription": (
                    "Discover local network addresses used for remote control pairing."
                ),
            },
            file,
        )
    run(["codesign", "--force", "--deep", "--sign", "-", str(app)])
    return app


def ensure_manual() -> Path:
    pdf = ROOT / "docs" / "manual" / "output" / "Overlay Studio Instruction Manual.pdf"
    if not pdf.is_file():
        raise SystemExit(f"Build the PDF first: {pdf}")
    return pdf


def build_dmg(app: Path, pdf: Path, output: Path) -> Path:
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()
    with tempfile.TemporaryDirectory(prefix="overlay-studio-dmg-") as tmp:
        stage = Path(tmp) / "Overlay Studio"
        stage.mkdir()
        shutil.copytree(app, stage / app.name, symlinks=True)
        shutil.copy2(pdf, stage / "Overlay Studio Instruction Manual.pdf")
        readme = stage / "README.txt"
        readme.write_text(
            "Overlay Studio for macOS\n"
            "========================\n\n"
            "1. Drag Overlay Studio into Applications.\n"
            "2. Open Overlay Studio Instruction Manual.pdf for setup, usage, and ProPresenter NDI steps.\n"
            "3. On first launch, if macOS blocks the app: right-click → Open → Open.\n\n"
            "Need help with NDI discovery? See the ProPresenter chapter in the PDF "
            "(Renewed Vision Settings → Inputs).\n",
            encoding="utf-8",
        )
        (stage / "Applications").symlink_to("/Applications")
        # UDZO compressed image
        run(
            [
                "hdiutil",
                "create",
                "-volname",
                "Overlay Studio",
                "-srcfolder",
                str(stage),
                "-ov",
                "-format",
                "UDZO",
                str(output),
            ]
        )
    return output


def verify_dmg(path: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="overlay-studio-verify-") as tmp:
        mount = Path(tmp) / "mnt"
        mount.mkdir()
        run(["hdiutil", "attach", str(path), "-mountpoint", str(mount), "-nobrowse", "-readonly"])
        try:
            app = mount / "Overlay Studio.app"
            pdf = mount / "Overlay Studio Instruction Manual.pdf"
            apps_link = mount / "Applications"
            if not app.is_dir():
                raise SystemExit("DMG missing Overlay Studio.app")
            if not pdf.is_file() or pdf.stat().st_size < 50_000:
                raise SystemExit("DMG missing or empty instruction PDF")
            if not apps_link.is_symlink():
                raise SystemExit("DMG missing Applications symlink")
            binary = app / "Contents" / "MacOS" / "overlay-studio-slint"
            if not binary.is_file():
                raise SystemExit("DMG app missing executable")
            print(
                f"verified: app={app.name} pdf={pdf.stat().st_size} bytes "
                f"binary={binary.stat().st_size} bytes"
            )
        finally:
            run(["hdiutil", "detach", str(mount), "-quiet"])


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--skip-verify", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="overlay-studio-app-") as tmp:
        app = build_app(Path(tmp))
        pdf = ensure_manual()
        dmg = build_dmg(app, pdf, args.output.resolve())
    print(dmg, dmg.stat().st_size)
    if not args.skip_verify:
        verify_dmg(dmg)


if __name__ == "__main__":
    main()
