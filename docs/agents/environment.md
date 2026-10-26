# Environment

PstOff is built and released for Windows only ([#43](https://github.com/spitefulFr0g/PstOff/issues/43)). All development happens on the Windows host.

## Checkout

- The one checkout is `C:\Users\spitefulfr0g\dev\PstOff`. Branch, commit and push from here.
- Agent shells are Git Bash and PowerShell. Python is `python` (3.11), so run repo scripts as `python scripts/<name>.py`.
- Authenticate `gh` on this host with `gh auth login`. The push command in `docs/agents/safety.md` depends on it.

## WSL Debian (legacy)

WSL (user `watermelon`) no longer hosts development. It keeps three things:

- **The libpff reader**, at `~/.local/share/pstoff-independent-reader`. Point the check at the Windows checkout through `/mnt/c`:
  ```bash
  MSYS_NO_PATHCONV=1 wsl.exe -d Debian -- python3 /mnt/c/Users/spitefulfr0g/dev/PstOff/scripts/libpff-check.py compare /mnt/c/<source.ost> /mnt/c/<out.pst>
  ```
  For anything longer than one command, write a script file under `$TEMP` and run `wsl.exe -d Debian -- bash /mnt/c/<script>.sh`. Inline quoting through `wsl.exe` breaks, and Git Bash rewrites `/paths` unless `MSYS_NO_PATHCONV=1` is set. Git Bash's `/tmp` (`$TEMP`) is `/mnt/c/Users/spitefulfr0g/AppData/Local/Temp` inside WSL.
- **History from the first #20 implementation run**, in `~/.codex/spec-notes/spitefulFr0g-PstOff-20/`. Its branches are now in the Windows checkout, and its notes and logs are copied to the gitignored `.scratch/spec-notes-20/`. Treat it as read-only.
- **The old checkout** at `~/dev/PstOff`, with the research worktrees. Read-only.

## Windows Sandbox build route

The rules are in `docs/agents/safety.md`. The parts on this machine:

- Rust stable MSVC, installed inside a guest and mapped read-only: `%TEMP%\PstOffRustInstall-8zzvegc3\install`.
- MSVC from Visual Studio 2022 Community (14.44.35207) and Windows SDK 10.0.26100.0, mapped read-only from their install folders.
- Verified guest configs from the first run: `.scratch/spec-notes-20/logs/gui-spike-attempt-6/final-build.wsb` with `final-input/build.cmd` for builds, and `runtime.wsb` for smoke runs.

Ticket #37 replaces these with a maintained wrapper script.
