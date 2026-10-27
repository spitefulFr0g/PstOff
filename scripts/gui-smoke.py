#!/usr/bin/env python3
"""Launch the synthetic GUI smoke driver in a clean Windows Sandbox.

The driver opens the production wizard with the real engine, holding a progress
event until the screenshot is taken. The guest maps only the built .exe,
generated synthetic OSTs and the guest
script, read-only, plus one writable output folder. No checkout, crate cache or
toolchain is mapped, and networking and the clipboard are off. The guest drives
the window with its own mouse, so the host desktop is never touched.

Build the inputs first, through the cargo wrapper:

    python scripts/sandbox-cargo.py build --release --locked -p pstoff-gui --example gui-smoke --config "target.x86_64-pc-windows-msvc.rustflags=['-C','target-feature=+crt-static']"
    python scripts/sandbox-cargo.py test --locked -p pstoff-gui --test smoke_fixtures -- --ignored

Then run `python scripts/gui-smoke.py [--out DIR]`.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("sandbox_cargo", HERE / "sandbox-cargo.py")
sandbox_cargo = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sandbox_cargo)

GUEST_INPUT = r"C:\PstOffSmokeInput"
GUEST_OUTPUT = r"C:\PstOffSmokeOutput"
DEFAULT_TIMEOUT_SECONDS = 10 * 60


def persistent_output(root: Path) -> Path:
    """The cargo wrapper's output folder for this worktree."""
    digest = hashlib.sha256(str(root).casefold().encode()).hexdigest()[:16]
    return sandbox_cargo.temp_root() / "PstOffSandboxCargo" / digest / "output-v2"


def make_config(path: Path, input_dir: Path, output: Path) -> None:
    config = ET.Element("Configuration")
    ET.SubElement(config, "VGpu").text = "Enable"
    ET.SubElement(config, "Networking").text = "Disable"
    ET.SubElement(config, "ClipboardRedirection").text = "Disable"
    ET.SubElement(config, "MappedFolders")
    sandbox_cargo.add_mapping(config, input_dir, GUEST_INPUT, True)
    sandbox_cargo.add_mapping(config, output, GUEST_OUTPUT, False)
    logon = ET.SubElement(config, "LogonCommand")
    ET.SubElement(logon, "Command").text = (
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File "
        rf"{GUEST_INPUT}\gui-smoke-guest.ps1"
    )
    ET.SubElement(config, "MemoryInMB").text = "4096"
    path.write_bytes(b"<?xml version='1.0' encoding='utf-8'?>\r\n" + ET.tostring(config, encoding="utf-8"))


def stage_input(input_dir: Path, exe: Path, fixtures: Path) -> None:
    input_dir.mkdir()
    shutil.copy2(exe, input_dir / "pstoff-gui.exe")
    shutil.copy2(HERE / "gui-smoke-guest.ps1", input_dir / "gui-smoke-guest.ps1")
    (input_dir / "fixtures").mkdir()
    for fixture in sorted(fixtures.glob("*.ost")):
        shutil.copy2(fixture, input_dir / "fixtures" / fixture.name)


def retain_output(output: Path, input_dir: Path, destination: Path) -> None:
    """Keep nested conversion artifacts and their generated comparison inputs."""
    reparse_flag = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    for parent, directories, files in os.walk(output, followlinks=False):
        for name in (*directories, *files):
            entry = Path(parent) / name
            info = entry.lstat()
            if stat.S_ISLNK(info.st_mode) or (
                reparse_flag and getattr(info, "st_file_attributes", 0) & reparse_flag
            ):
                raise RuntimeError(f"Refusing to retain smoke output containing a link: {entry}")
    shutil.copytree(output, destination, dirs_exist_ok=True)
    shutil.copytree(input_dir / "fixtures", destination / "sources", dirs_exist_ok=True)
    evidence = {}
    for folder in ("sources", "conversions"):
        for path in sorted((destination / folder).glob("*")):
            if path.is_file():
                evidence[path.relative_to(destination).as_posix()] = {
                    "bytes": path.stat().st_size,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
    (destination / "artifacts.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


def run(exe: Path, fixtures: Path, destination: Path, timeout: int) -> int:
    temp = sandbox_cargo.temp_root()
    lock = temp / "PstOffSandboxCargo" / "sandbox.lock"
    with sandbox_cargo.host_lock(lock):
        if sandbox_cargo.sandbox_processes():
            print("Windows Sandbox is already running outside this script; leaving it untouched.", file=sys.stderr)
            return sandbox_cargo.EXIT_SANDBOX_RUNNING
        if destination.exists() and any(destination.iterdir()):
            raise RuntimeError(f"Refusing to overwrite retained smoke evidence: {destination}")
        run_dir = Path(tempfile.mkdtemp(prefix="PstOffGuiSmoke-run-", dir=temp))
        retained = False
        try:
            input_dir = run_dir / "input"
            output = run_dir / "output"
            output.mkdir()
            stage_input(input_dir, exe, fixtures)
            wsb = run_dir / "smoke.wsb"
            make_config(wsb, input_dir, output)
            if sandbox_cargo.sandbox_processes():
                print("Windows Sandbox is already running outside this script; leaving it untouched.", file=sys.stderr)
                return sandbox_cargo.EXIT_SANDBOX_RUNNING
            launcher = os.environ.get("PSTOFF_WINDOWS_SANDBOX", "WindowsSandbox.exe")
            process = subprocess.Popen([launcher, str(wsb)])
            try:
                state = sandbox_cargo.wait_for_guest(process, output / "finished.txt", timeout)
            except BaseException:
                sandbox_cargo.close_sandbox(process)
                retain_output(output, input_dir, destination)
                retained = True
                raise
            # Stop only the Sandbox this run launched, then retain evidence even
            # on timeout or guest failure. The marker is written after all files.
            if state != "closed":
                sandbox_cargo.close_sandbox(process)
            retain_output(output, input_dir, destination)
            retained = True
            if state == "timeout":
                print("The smoke run timed out.", file=sys.stderr)
                return sandbox_cargo.EXIT_CANNOT_RUN
            if state == "closed":
                print("Windows Sandbox closed before the smoke run finished.", file=sys.stderr)
                return sandbox_cargo.EXIT_CANNOT_RUN
            log = output / "smoke.log"
            if log.exists():
                print(log.read_text(encoding="utf-8", errors="replace"), end="")
            return int((output / "finished.txt").read_text(encoding="ascii").strip() or 2)
        finally:
            resolved = run_dir.resolve()
            if resolved.parent != temp.resolve() or not resolved.name.startswith("PstOffGuiSmoke-run-"):
                raise RuntimeError(f"Refusing to remove unexpected smoke directory: {resolved}")
            if retained:
                shutil.rmtree(resolved, ignore_errors=True)
            else:
                print(f"Smoke staging retained for recovery: {run_dir}", file=sys.stderr)


def main(argv: list[str]) -> int:
    if os.name != "nt":
        print("gui-smoke.py requires Windows.", file=sys.stderr)
        return sandbox_cargo.EXIT_CANNOT_RUN
    root = sandbox_cargo.repo_root()
    built = persistent_output(root) / "target"
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", type=Path, default=built / "release" / "examples" / "gui-smoke.exe")
    parser.add_argument("--fixtures", type=Path, default=built / "tmp" / "gui-smoke")
    parser.add_argument("--out", type=Path, default=root / ".scratch" / "gui-smoke" / time.strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    args = parser.parse_args(argv)
    if not args.exe.is_file():
        print(f"Build the GUI first; no executable at {args.exe}", file=sys.stderr)
        return sandbox_cargo.EXIT_CANNOT_RUN
    if not any(args.fixtures.glob("*.ost")):
        print(f"Generate the smoke fixtures first; none in {args.fixtures}", file=sys.stderr)
        return sandbox_cargo.EXIT_CANNOT_RUN
    code = run(args.exe.resolve(), args.fixtures.resolve(), args.out.resolve(), args.timeout)
    print(f"Smoke output: {args.out}")
    return code


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (OSError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"GUI smoke run could not run: {exc}", file=sys.stderr)
        raise SystemExit(sandbox_cargo.EXIT_CANNOT_RUN)
