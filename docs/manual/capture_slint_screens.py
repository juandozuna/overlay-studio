#!/usr/bin/env python3
"""Launch Overlay Studio workspaces and capture window screenshots for the manual."""
from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "desktop/slint/target/release/overlay-studio-slint"
OUT = Path(__file__).resolve().parent / "screenshots"
CAPTURE = Path(__file__).resolve().parent / "capture"
WINDOW_TOOL = Path("/tmp/os_window")

SHOTS = [
    ("11-slint-designer.png", "Slint Design Verification.overlay", ["--workspace", "0"]),
    ("14-slint-live.png", "Slint Live Verification.overlay", ["--workspace", "1"]),
    ("13-slint-setup.png", "Slint Live Verification.overlay", ["--workspace", "1", "--setup"]),
    ("15-slint-clocks.png", "Slint Live Verification.overlay", ["--workspace", "1", "--setup"]),
    ("16-slint-remote.png", "Slint Remote Verification.overlay", ["--workspace", "2"]),
    ("19-slint-timeline.png", "Slint Animation Verification.overlay", ["--workspace", "3"]),
    ("20-slint-events.png", "Slint Events Verification.overlay", ["--workspace", "3", "--events"]),
    ("21-slint-live-values.png", "Slint Live Values Verification.overlay", ["--workspace", "1"]),
    ("24-slint-custom-fields.png", "Slint Custom Field Verification.overlay", ["--workspace", "1", "--setup"]),
]


def ensure_window_tool() -> None:
    if WINDOW_TOOL.is_file():
        return
    src = Path("/tmp/os_window.swift")
    src.write_text(
        """
import Cocoa
import CoreGraphics
let needle = CommandLine.arguments.dropFirst().first ?? "overlay-studio-slint"
guard let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
    print("[]"); exit(1)
}
var matches: [[String: Any]] = []
for w in info {
    let owner = (w[kCGWindowOwnerName as String] as? String) ?? ""
    let name = (w[kCGWindowName as String] as? String) ?? ""
    let pid = w[kCGWindowOwnerPID as String] as? pid_t ?? 0
    if owner.localizedCaseInsensitiveContains(needle) || name.localizedCaseInsensitiveContains("Overlay Studio") {
        let bounds = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        let id = w[kCGWindowNumber as String] as? Int ?? 0
        matches.append([
            "id": id, "pid": Int(pid), "owner": owner, "name": name,
            "x": bounds["X"] as? Double ?? 0, "y": bounds["Y"] as? Double ?? 0,
            "w": bounds["Width"] as? Double ?? 0, "h": bounds["Height"] as? Double ?? 0,
        ])
    }
}
let data = try! JSONSerialization.data(withJSONObject: matches)
FileHandle.standardOutput.write(data)
print("")
"""
    )
    subprocess.run(
        ["swiftc", str(src), "-o", str(WINDOW_TOOL), "-framework", "Cocoa"],
        check=True,
    )


def find_window(pid: int) -> dict | None:
    raw = subprocess.check_output([str(WINDOW_TOOL), "overlay-studio-slint"], text=True).strip()
    windows = json.loads(raw or "[]")
    for window in windows:
        if window.get("pid") == pid and window.get("w", 0) > 400:
            return window
    for window in windows:
        if window.get("w", 0) > 400:
            return window
    return None


def run_shot(name: str, project: str, flags: list[str]) -> None:
    project_path = CAPTURE / project
    if not project_path.is_file():
        raise SystemExit(f"Missing project: {project_path}")
    proc = subprocess.Popen(
        [str(BINARY), "--skip-recovery", "--open", str(project_path), *flags],
        cwd=str(ROOT),
    )
    try:
        window = None
        for _ in range(40):
            time.sleep(0.5)
            if proc.poll() is not None:
                raise SystemExit(f"App exited early for {name} with code {proc.returncode}")
            window = find_window(proc.pid)
            if window:
                break
        if not window:
            raise SystemExit(f"Window did not appear for {name}")
        time.sleep(1.8)
        out = OUT / name
        subprocess.run(["screencapture", "-x", "-l", str(window["id"]), str(out)], check=True)
        if out.stat().st_size < 10_000:
            raise SystemExit(f"Capture too small for {name}: {out.stat().st_size}")
        print(f"wrote {out.name} ({out.stat().st_size} bytes)")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        time.sleep(0.6)


def main() -> None:
    if not BINARY.is_file():
        raise SystemExit(f"Build release binary first: {BINARY}")
    ensure_window_tool()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, project, flags in SHOTS:
        if not (CAPTURE / project).is_file():
            print(f"skip {name}: missing {project}")
            continue
        run_shot(name, project, flags)


if __name__ == "__main__":
    main()
