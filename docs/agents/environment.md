# Environment

PstOff is built and released for Windows only ([#43](https://github.com/spitefulFr0g/PstOff/issues/43)). All development happens on the Windows host.

## Checkout

- The one checkout is `C:\Users\spitefulfr0g\dev\PstOff`. Branch, commit and push from here.
- Agent shells are Git Bash and PowerShell. Python is `python` (3.11), so run repo scripts as `python scripts/<name>.py`.
- Authenticate `gh` on this host with `gh auth login`. The push command in `docs/agents/safety.md` depends on it.

## WSL Debian (libpff only)

WSL (user `watermelon`) no longer hosts development. It keeps three things:

- **The libpff reader**, at `~/.local/share/pstoff-independent-reader`. Point the check at the Windows checkout through `/mnt/c`:
  ```bash
  MSYS_NO_PATHCONV=1 wsl.exe -d Debian -- python3 /mnt/c/Users/spitefulfr0g/dev/PstOff/scripts/libpff-check.py compare /mnt/c/<source.ost> /mnt/c/<out.pst>
  ```
  For anything longer than one command, write a script file under `$TEMP` and run `wsl.exe -d Debian -- bash /mnt/c/<script>.sh`. Inline quoting through `wsl.exe` breaks, and Git Bash rewrites `/paths` unless `MSYS_NO_PATHCONV=1` is set. Git Bash's `/tmp` (`$TEMP`) is `/mnt/c/Users/spitefulfr0g/AppData/Local/Temp` inside WSL.
- **History from the first #20 implementation run**, in `~/.codex/spec-notes/spitefulFr0g-PstOff-20/`. Its branches are now in the Windows checkout, and its notes and logs are copied to the gitignored `.scratch/spec-notes-20/`. Treat it as read-only.
- **The old checkout** at `~/dev/PstOff`, with the research worktrees. Read-only.

## Local Windows Sandbox build route

The rules are in `docs/agents/safety.md`. For builds and checks on this Windows machine, run Cargo through `python scripts/sandbox-cargo.py <cargo args>`; run the full audit, fmt, clippy and test suite through `python scripts/check`. Use `python scripts/sandbox-cargo.py --probe` to verify guest networking is disabled and the source map rejects writes. The wrapper stages each worktree snapshot under `%TEMP%`, keeps a separate persistent target directory per worktree, and serializes Sandbox sessions with a host lock. Its Cargo cache is shared under `%TEMP%`. The parts on this machine:

- Rust stable MSVC, installed inside a guest and mapped read-only: `%TEMP%\PstOffRustInstall-8zzvegc3\install`.
- MSVC from Visual Studio 2022 Community (14.44.35207) and Windows SDK 10.0.26100.0, mapped read-only from their install folders.
- The wrapper uses MSVC 14.44.35207 and SDK 10.0.26100.0 from those paths.

## Linux portable Cargo CI

GitHub Actions may run portable Cargo checks for the engine, CLI and format
crates on Ubuntu, using the workspace's supported Rust version 1.89. The Linux
lane checks formatting, Clippy and platform-independent tests. Engine tests
that reach Windows disk preflight are ignored on non-Windows (but still
compiled); CLI conversion tests are Windows-gated. Conversion/preflight tests
remain enabled for Windows Sandbox checks. Linux CI does not build the Windows
GUI or establish Windows conversion/GUI/release acceptance. It also does not
establish Faithful acceptance, which requires the checks in
`docs/agents/faithful-checks.md`. Local Windows builds and checks still use
Windows Sandbox as described above.

## Full local check coverage

`scripts/check` uses `fmt --all`, `clippy --workspace --all-targets` and
`test --workspace`, including the `pstoff-format` crate. It also tests the GUI
smoke example and runs `unittest discover -s tests -v` **inside the guest**.
For this route only, the installed Python runtime (`sys.base_prefix`, override
`PSTOFF_PYTHON_ROOT`) and Git (`C:\Program Files\Git`, override
`PSTOFF_GIT_ROOT`) are mapped read-only. Python uses `-I -S -B` to isolate it
from user configuration and site packages and to avoid writing bytecode into
the source mapping. No harness test launches a real nested Sandbox; process
and ownership tests mock that boundary. Python results go to
`output-v2/python-tests.log` and `.exit` alongside the Cargo logs; any missing
step or nonzero exit fails the check.
Staged source files receive fresh timestamps so restored Cargo targets cannot
hide edits made while an earlier snapshot was still compiling.

## GUI smoke run

The GUI is never launched on the host. `python scripts/gui-smoke.py` boots a second, clean Windows Sandbox that maps only the built `.exe`, generated OSTs and `scripts/gui-smoke-guest.ps1` read-only, plus one writable output folder: no checkout, crate cache or toolchain. Networking and the clipboard are off. The guest script launches the wizard on the fixtures, clicks through it with the guest's own mouse, screenshots each screen, closes the window and records its exit code. It shares the cargo wrapper's host lock and refuses to start while another Sandbox runs.

1. Build the self-contained synthetic smoke driver (static CRT, so a clean guest needs no VC runtime). This example opens the same wizard with the real engine, holds a nonzero progress event with a current folder until the guest captures it, and then resumes the real conversion. The shipping executable has no smoke gate:
   ```
   python scripts/sandbox-cargo.py build --release --locked -p pstoff-gui --example gui-smoke --config "target.x86_64-pc-windows-msvc.rustflags=['-C','target-feature=+crt-static']"
   ```
2. Write the smoke fixtures into the wrapper's target folder:
   ```
   python scripts/sandbox-cargo.py test --locked -p pstoff-gui --test smoke_fixtures -- --ignored
   ```
3. `python scripts/gui-smoke.py`. Screenshots and `smoke.log` land in `.scratch/gui-smoke/<time>/`, or an empty `--out <dir>`. The guest writes PSTs and JSON/text reports directly to the dedicated writable mapping's `conversions/` directory. The host retains that tree, copies the generated comparison OSTs into `sources/`, and writes byte counts and SHA-256 hashes to `artifacts.json`. Failures, early closes and timeouts retain available artifacts too; check `finished.txt` and the exit code before using them. If retention itself fails, staging is left in temp for recovery. Existing evidence is never overwritten.

The smoke route is generated-data evidence only. It does not perform any
Faithful check. Keep the retained source/PST pairs for the separately authorized
SCANPST, independent libpff comparison and manual Outlook acceptance gates.
