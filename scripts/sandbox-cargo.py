#!/usr/bin/env python3
"""Run Cargo in an isolated Windows Sandbox with read-only source and tools."""

from __future__ import annotations

import csv
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from contextlib import contextmanager
from collections.abc import Callable
from pathlib import Path

EXIT_SANDBOX_RUNNING = 3
EXIT_CANNOT_RUN = 2
DEFAULT_TIMEOUT_SECONDS = 60 * 60
CHECK_COMMANDS = (
    ("metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"),
    ("fmt", "--all", "--", "--check"),
    ("clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"),
    ("test", "--locked", "--workspace"),
    ("test", "--locked", "-p", "pstoff-gui", "--example", "gui-smoke"),
)
GUEST_ENVIRONMENT = (
    r'set "PATH=C:\PstOffRust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;'
    r'C:\PstOffHostVC\Tools\MSVC\14.44.35207\bin\Hostx64\x64;'
    r'C:\PstOffWindowsKits\bin\10.0.26100.0\x64;C:\PstOffGit\cmd;%PATH%"',
    r'set "INCLUDE=C:\PstOffHostVC\Tools\MSVC\14.44.35207\include;'
    r'C:\PstOffWindowsKits\Include\10.0.26100.0\ucrt;'
    r'C:\PstOffWindowsKits\Include\10.0.26100.0\shared;'
    r'C:\PstOffWindowsKits\Include\10.0.26100.0\um;'
    r'C:\PstOffWindowsKits\Include\10.0.26100.0\winrt"',
    r'set "LIB=C:\PstOffHostVC\Tools\MSVC\14.44.35207\lib\x64;'
    r'C:\PstOffWindowsKits\Lib\10.0.26100.0\ucrt\x64;'
    r'C:\PstOffWindowsKits\Lib\10.0.26100.0\um\x64"',
    r'set "CARGO_HOME=C:\PstOffCache"',
    r'set "RUSTUP_HOME=C:\PstOffRust\rustup"',
    r'set "CARGO_NET_OFFLINE=true"',
    r'set "CARGO_BUILD_JOBS=2"',
    r'set "CARGO_TARGET_DIR=C:\PstOffBuild\target"',
    r'set "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='
    r'C:\PstOffHostVC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe"',
    r'set "GPUI_FXC_PATH=C:\PstOffWindowsKits\bin\10.0.26100.0\x64\fxc.exe"',
)


def check_commands(fast: bool = False) -> tuple[tuple[str, ...], ...]:
    return tuple(
        command + ("--", "--skip", "large_generated_ost_has_fixed_separate_process_peak")
        if fast and command == ("test", "--locked", "--workspace") else command
        for command in CHECK_COMMANDS
    )


class GuestProgress:
    """Report stage transitions and elapsed time without exposing log contents."""

    def __init__(self, output: Path):
        self.output = output
        self.started = time.monotonic()
        self.last_report = self.started
        self.stage = "Starting Windows Sandbox"
        self.completed: set[str] = set()
        print(f"Sandbox logs: {output}", flush=True)

    def __call__(self) -> None:
        now = time.monotonic()
        try:
            stage = (self.output / "stage.txt").read_text(encoding="ascii").strip()
        except OSError:
            stage = ""
        changed = bool(stage and stage != self.stage)
        if changed:
            self.stage = stage
        if changed or now - self.last_report >= 30:
            print(f"[Sandbox +{now - self.started:.0f}s] {self.stage}", flush=True)
            self.last_report = now
        for marker in sorted(self.output.glob("*.exit")):
            if marker.name in self.completed:
                continue
            try:
                code = int(marker.read_text(encoding="ascii").strip())
            except (OSError, ValueError):
                continue
            print(f"[Sandbox +{now - self.started:.0f}s] {marker.stem} finished: exit {code}; "
                  f"log: {marker.with_suffix('.log')}", flush=True)
            self.completed.add(marker.name)


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return Path(result.stdout.strip()).resolve()


def required_path(env_name: str, default: str) -> Path:
    value = Path(os.environ.get(env_name, default))
    if not value.is_dir():
        raise RuntimeError(f"Required Windows Sandbox mapping is missing: {value}")
    return value.resolve()


def temp_root() -> Path:
    return Path(os.environ.get("TEMP", tempfile.gettempdir())).resolve()


@contextmanager
def host_lock(path: Path):
    """Serialize all wrappers; the OS releases the lock if the host process exits."""
    import msvcrt

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+b") as lock_file:
        if lock_file.tell() == 0:
            lock_file.write(b"\0")
            lock_file.flush()
        lock_file.seek(0)
        while True:
            try:
                msvcrt.locking(lock_file.fileno(), msvcrt.LK_NBLCK, 1)
                break
            except OSError:
                time.sleep(0.2)
        try:
            yield
        finally:
            lock_file.seek(0)
            msvcrt.locking(lock_file.fileno(), msvcrt.LK_UNLCK, 1)


def sandbox_processes() -> list[int]:
    result = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq WindowsSandbox.exe", "/FO", "CSV", "/NH"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(
            f"Could not establish whether Windows Sandbox is running "
            f"(tasklist exit code {result.returncode})"
            + (f": {detail}" if detail else ".")
        )
    pids = []
    try:
        rows = csv.reader(io.StringIO(result.stdout), strict=True)
        for fields in rows:
            if not fields:
                continue
            if fields[0].strip().casefold() == "windowssandbox.exe":
                if len(fields) < 2:
                    raise RuntimeError(
                        "Malformed WindowsSandbox.exe record from tasklist."
                    )
                try:
                    pid = int(fields[1])
                except ValueError as exc:
                    raise RuntimeError(
                        "Malformed WindowsSandbox.exe PID from tasklist."
                    ) from exc
                if pid <= 0:
                    raise RuntimeError("Malformed WindowsSandbox.exe PID from tasklist.")
                pids.append(pid)
    except csv.Error as exc:
        raise RuntimeError("Could not parse Windows Sandbox process list.") from exc
    return pids


def close_sandbox(process: subprocess.Popen) -> None:
    subprocess.run(
        ["taskkill", "/PID", str(process.pid), "/T", "/F"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        process.wait(timeout=30)
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("Windows Sandbox did not close after taskkill") from exc


def wait_for_guest(process: subprocess.Popen, marker: Path, timeout_seconds: int,
                   progress: Callable[[], None] | None = None) -> str:
    deadline = time.monotonic() + timeout_seconds
    while True:
        if progress is not None:
            progress()
        if marker.exists():
            return "finished"
        if process.poll() is not None:
            return "closed"
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return "timeout"
        time.sleep(min(1, remaining))


def copy_snapshot(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True)
    files = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=source,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.split(b"\0")
    for raw in files:
        if not raw:
            continue
        relative = Path(os.fsdecode(raw))
        if relative.parts[0] in {".git", "target", "__pycache__"}:
            continue
        if len(relative.parts) >= 2 and relative.parts[:2] == ("testdata", "private"):
            continue
        origin = source / relative
        if origin.is_symlink() or not origin.is_file():
            continue
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        # Each guest sees the same source path while restoring a target cache.
        # An edit made while an earlier snapshot builds can predate its cached
        # artifacts. Fresh copy timestamps force Cargo to check this snapshot
        # rather than silently reusing that earlier workspace test binary.
        shutil.copyfile(origin, target)


def cleanup_run_dir(run_dir: Path, temp: Path) -> None:
    resolved_temp = temp.resolve()
    resolved_run = run_dir.resolve()
    if resolved_run.parent != resolved_temp or not resolved_run.name.startswith("PstOffSandboxCargo-run-"):
        raise RuntimeError(f"Refusing to remove an unexpected Sandbox run directory: {resolved_run}")
    deadline = time.monotonic() + 30
    while resolved_run.exists():
        try:
            shutil.rmtree(resolved_run)
        except OSError as exc:
            if getattr(exc, "winerror", None) not in {5, 32, 33} or time.monotonic() >= deadline:
                raise
            time.sleep(0.25)


def make_guest_script(
    path: Path,
    cargo_args: tuple[str, ...],
    is_check: bool,
    is_probe: bool = False,
    fast: bool = False,
) -> tuple[tuple[str, ...], ...]:
    commands = check_commands(fast) if is_check else (cargo_args,)
    lines = ["@echo off", "setlocal", *GUEST_ENVIRONMENT]
    if is_probe:
        probe_command = (
            'powershell.exe -NoProfile -Command "$ErrorActionPreference='
            "'SilentlyContinue'; $network = Test-NetConnection -ComputerName 1.1.1.1 "
            "-Port 443 -InformationLevel Quiet -WarningAction SilentlyContinue; "
            "if ($network) { Write-Output 'network=available'; exit 1 }; "
            "Write-Output 'network=disabled'; try { "
            "[IO.File]::WriteAllText('C:\\PstOffSource\\sandbox-write-probe.tmp','probe'); "
            "Write-Output 'source_write=allowed'; exit 1 } catch { "
            "Write-Output 'source_write=blocked' }\" > "
            r"C:\PstOffOutput\probe.log 2>&1"
        )
        lines.extend((
            r"echo Network and source-write probe > C:\PstOffOutput\stage.txt",
            probe_command,
            "echo %ERRORLEVEL% > C:\\PstOffOutput\\probe.exit",
            "echo %ERRORLEVEL% > C:\\PstOffOutput\\finished.txt",
            "shutdown /s /t 10",
        ))
        path.write_text("\r\n".join(lines) + "\r\n", encoding="ascii")
        return ()
    lines.extend(
        (
            r"echo Restoring build cache > C:\PstOffOutput\stage.txt",
            r"if exist C:\PstOffOutput\target robocopy C:\PstOffOutput\target C:\PstOffBuild\target /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP > C:\PstOffOutput\restore-target.log",
            r"if errorlevel 8 (echo 2 > C:\PstOffOutput\finished.txt & exit /b 2)",
            r"cd /d C:\PstOffSource",
            "if errorlevel 1 exit /b 2",
            'set "FINAL_EXIT=0"',
        )
    )
    for index, command in enumerate(commands):
        args = " ".join(f'"{arg}"' if " " in arg else arg for arg in command)
        log = f"C:\\PstOffOutput\\cargo-{index}.log"
        code = f"C:\\PstOffOutput\\cargo-{index}.exit"
        lines.extend(
            (
                f"echo cargo-{index}: {command[0]} > C:\\PstOffOutput\\stage.txt",
                f"cargo {args} > {log} 2>&1",
                'set "STEP_EXIT=%ERRORLEVEL%"',
                f"echo %STEP_EXIT% > {code}",
                'if not %STEP_EXIT%==0 set "FINAL_EXIT=%STEP_EXIT%"',
            )
        )
    if is_check:
        lines.extend((
            r"echo Python harness tests > C:\PstOffOutput\stage.txt",
            r'C:\PstOffPython\python.exe -I -S -B -m unittest discover -s C:\PstOffSource\tests -v > C:\PstOffOutput\python-tests.log 2>&1',
            'set "STEP_EXIT=%ERRORLEVEL%"',
            r'echo %STEP_EXIT% > C:\PstOffOutput\python-tests.exit',
            'if not %STEP_EXIT%==0 set "FINAL_EXIT=%STEP_EXIT%"',
        ))
    lines.extend(
        (
            r"echo Saving build cache > C:\PstOffOutput\stage.txt",
            r"if exist C:\PstOffBuild\target robocopy C:\PstOffBuild\target C:\PstOffOutput\target /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP > C:\PstOffOutput\save-target.log",
            'if errorlevel 8 set "FINAL_EXIT=2"',
            r"echo %FINAL_EXIT% > C:\PstOffOutput\finished.txt",
            "shutdown /s /t 10",
        )
    )
    path.write_text("\r\n".join(lines) + "\r\n", encoding="ascii")
    return commands


def add_mapping(config: ET.Element, host: Path, guest: str, readonly: bool) -> None:
    mapped = ET.SubElement(config.find("MappedFolders"), "MappedFolder")
    ET.SubElement(mapped, "HostFolder").text = str(host)
    ET.SubElement(mapped, "SandboxFolder").text = guest
    ET.SubElement(mapped, "ReadOnly").text = "true" if readonly else "false"


def make_config(
    path: Path,
    source: Path,
    input_dir: Path,
    cache: Path,
    rust: Path,
    vc: Path,
    kits: Path,
    output: Path,
    python: Path | None = None,
    git: Path | None = None,
) -> None:
    config = ET.Element("Configuration")
    ET.SubElement(config, "VGpu").text = "Enable"
    ET.SubElement(config, "Networking").text = "Disable"
    ET.SubElement(config, "ClipboardRedirection").text = "Disable"
    folders = ET.SubElement(config, "MappedFolders")
    add_mapping(config, source, r"C:\PstOffSource", True)
    add_mapping(config, input_dir, r"C:\PstOffInput", True)
    add_mapping(config, cache, r"C:\PstOffCache", True)
    add_mapping(config, rust, r"C:\PstOffRust", True)
    add_mapping(config, vc, r"C:\PstOffHostVC", True)
    add_mapping(config, kits, r"C:\PstOffWindowsKits", True)
    add_mapping(config, output, r"C:\PstOffOutput", False)
    if python is not None:
        add_mapping(config, python, r"C:\PstOffPython", True)
    if git is not None:
        add_mapping(config, git, r"C:\PstOffGit", True)
    logon = ET.SubElement(config, "LogonCommand")
    ET.SubElement(logon, "Command").text = r"cmd.exe /c C:\PstOffInput\guest.cmd"
    ET.SubElement(config, "MemoryInMB").text = "8192"
    path.write_bytes(b"<?xml version='1.0' encoding='utf-8'?>\r\n" + ET.tostring(config, encoding="utf-8"))


def fetch_dependencies(root: Path, cache: Path, rust: Path) -> None:
    cache.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["CARGO_HOME"] = str(cache)
    env["CARGO_HTTP_USER_AGENT"] = "PstOff-research (https://github.com/spitefulFr0g/PstOff)"
    cargo = rust / "rustup" / "toolchains" / "stable-x86_64-pc-windows-msvc" / "bin" / "cargo.exe"
    if not cargo.is_file():
        raise RuntimeError(f"Cargo executable is missing from the configured toolchain: {cargo}")
    subprocess.run([str(cargo), "fetch", "--locked"], cwd=root, env=env, check=True)


def print_guest_output(output: Path, is_check: bool = False) -> int:
    exit_files = sorted(output.glob("cargo-*.exit"), key=lambda p: int(p.stem.split("-")[1]))
    if is_check:
        expected = [output / f"cargo-{i}.exit" for i in range(len(CHECK_COMMANDS))]
        expected.append(output / "python-tests.exit")
        if not all(path.is_file() for path in expected):
            print("The guest did not report every prescribed check.", file=sys.stderr)
            return EXIT_CANNOT_RUN
        exit_files.append(output / "python-tests.exit")
    result = 0
    for exit_file in exit_files:
        log = output / (exit_file.stem + ".log")
        if log.exists() and not (is_check and exit_file.stem == "cargo-0"):
            print(log.read_text(encoding="utf-8", errors="replace"), end="")
        try:
            code = int(exit_file.read_text(encoding="ascii").strip())
        except (ValueError, OSError):
            return EXIT_CANNOT_RUN
        if code:
            result = code
    return result


def run_wrapper(
    cargo_args: tuple[str, ...],
    is_check: bool = False,
    is_probe: bool = False,
    fast: bool = False,
) -> int:
    root = repo_root()
    temp = temp_root()
    digest = hashlib.sha256(str(root).casefold().encode()).hexdigest()[:16]
    persistent = temp / "PstOffSandboxCargo" / digest
    persistent.mkdir(parents=True, exist_ok=True)
    lock = temp / "PstOffSandboxCargo" / "sandbox.lock"
    print("Waiting for the shared Sandbox lock...", flush=True)
    with host_lock(lock):
        active = sandbox_processes()
        if active:
            print("Windows Sandbox is already running outside this wrapper; leaving it untouched.", file=sys.stderr)
            return EXIT_SANDBOX_RUNNING

        rust = required_path("PSTOFF_RUST_INSTALL", str(temp / "PstOffRustInstall-8zzvegc3" / "install"))
        cache = temp / "PstOffCargoCache"
        cache.mkdir(parents=True, exist_ok=True)
        if not is_probe:
            print("Fetching locked dependencies (no build scripts)...", flush=True)
            fetch_dependencies(root, cache, rust)
        run_dir = Path(tempfile.mkdtemp(prefix="PstOffSandboxCargo-run-", dir=temp))
        source = run_dir / "source"
        output = persistent / "output-v2"
        output.mkdir(exist_ok=True)
        for old in (*output.glob("cargo-*"), *output.glob("python-tests.*"),
                    *output.glob("probe.*"), output / "finished.txt", output / "stage.txt"):
            if old.is_file():
                old.unlink()
        try:
            copy_snapshot(root, source)
            input_dir = run_dir / "input"
            input_dir.mkdir()
            script = input_dir / "guest.cmd"
            make_guest_script(script, cargo_args, is_check, is_probe=is_probe, fast=fast)
            vc = required_path("PSTOFF_VC_ROOT", r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC")
            kits = required_path("PSTOFF_WINDOWS_KITS", r"C:\Program Files (x86)\Windows Kits\10")
            wsb = run_dir / "run.wsb"
            python = required_path("PSTOFF_PYTHON_ROOT", sys.base_prefix) if is_check else None
            git = required_path("PSTOFF_GIT_ROOT", r"C:\Program Files\Git") if is_check else None
            make_config(wsb, source, input_dir, cache, rust, vc, kits, output, python, git)
            active = sandbox_processes()
            if active:
                print("Windows Sandbox is already running outside this wrapper; leaving it untouched.", file=sys.stderr)
                return EXIT_SANDBOX_RUNNING
            launcher = os.environ.get("PSTOFF_WINDOWS_SANDBOX", "WindowsSandbox.exe")
            process = subprocess.Popen([launcher, str(wsb)])
            progress = GuestProgress(output)
            finished = output / "finished.txt"
            state = wait_for_guest(process, finished, int(os.environ.get("PSTOFF_SANDBOX_TIMEOUT", DEFAULT_TIMEOUT_SECONDS)), progress)
            if state == "closed":
                print("Windows Sandbox closed before writing its completion marker.", file=sys.stderr)
                return EXIT_CANNOT_RUN
            if state == "timeout":
                close_sandbox(process)
                print("Windows Sandbox timed out; the run could not be completed.", file=sys.stderr)
                return EXIT_CANNOT_RUN
            close_sandbox(process)
            with finished.open("r", encoding="ascii") as marker:
                guest_exit = int(marker.read().strip())
            if is_probe:
                print((output / "probe.log").read_text(encoding="utf-8", errors="replace"), end="")
                return guest_exit
            printed_exit = print_guest_output(output, is_check=is_check)
            if is_check:
                metadata = output / "cargo-0.log"
                audit = subprocess.run(
                    [sys.executable, str(root / "scripts" / "audit-dependencies.py")],
                    input=metadata.read_text(encoding="utf-8", errors="replace"),
                    text=True,
                    encoding="utf-8",
                    cwd=root,
                )
                if audit.returncode:
                    return audit.returncode
            return printed_exit or guest_exit
        finally:
            cleanup_run_dir(run_dir, temp)


def main(argv: list[str]) -> int:
    if os.name != "nt":
        print("sandbox-cargo.py requires Windows.", file=sys.stderr)
        return EXIT_CANNOT_RUN
    if not argv:
        print("Usage: python scripts/sandbox-cargo.py <cargo args> | --check | --probe", file=sys.stderr)
        return EXIT_CANNOT_RUN
    if argv == ["--check"]:
        return run_wrapper((), is_check=True)
    if argv == ["--check-fast"]:
        print("FAST checks: skipping only the large process-memory regression.", flush=True)
        return run_wrapper((), is_check=True, fast=True)
    if argv == ["--probe"]:
        return run_wrapper((), is_probe=True)
    return run_wrapper(tuple(argv))


if __name__ == "__main__":
    # Guest logs and metadata use UTF-8 even when host Python defaults to cp1252.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (OSError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"Windows Sandbox Cargo could not run: {exc}", file=sys.stderr)
        raise SystemExit(EXIT_CANNOT_RUN)
