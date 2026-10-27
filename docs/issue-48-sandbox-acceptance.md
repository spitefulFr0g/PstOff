# Issue #48: Windows Sandbox cargo wrapper acceptance

Recorded 2026-10-27 on the Windows host, branch
`chore/issue-48-sandbox-acceptance` at `e2d8f57` (the wrapper as merged in
PR #49, unchanged). Unit tests in `tests/test_sandbox_cargo.py` mock the
Sandbox boundary; every result below comes from a real Windows Sandbox run.
Process observations use `Get-Process` names containing `Sandbox`; the
unrelated `codex-windows-sandbox-service` is omitted.

## Full check

`python scripts/check --full` exited 0 in one Sandbox boot (about 1,213
seconds of guest time). Every Cargo step ran in the guest and exited 0:
locked metadata (then the host dependency audit on its output), `fmt --all
--check`, warning-denied workspace Clippy, `test --workspace` including
`large_generated_ost_has_fixed_separate_process_peak`, and the `gui-smoke`
example test. The 30 Python harness tests passed in the guest.

## Guest probe

`python scripts/sandbox-cargo.py --probe` exited 0 after 17 seconds:

```
network=disabled
source_write=blocked
```

The probe tries TCP 443 to `1.1.1.1` and a write to
`C:\PstOffSource\sandbox-write-probe.tmp`; either succeeding exits 1.

## Concurrent calls from two worktrees

`python scripts/sandbox-cargo.py build --locked -p pstoff-format` started at
17:21:45 from this checkout and from a temporary detached worktree under
`%TEMP%` (removed afterwards). Both exited 0:

| Worktree | Lock acquired | Finished | Exit |
| --- | --- | --- | --- |
| This checkout | 17:21:45 | 17:25:38 | 0 |
| Temporary worktree | 17:25:39 | 17:26:09 | 0 |

The second call waited on the host lock until the first released it, then
booted its own Sandbox with its own persistent target directory.

## Sandbox not started by the wrapper

A bare Sandbox (networking and clipboard off, no mappings) was launched
directly with `WindowsSandbox.exe`. With it running, the wrapper exited 3 with
"Windows Sandbox is already running outside this wrapper; leaving it
untouched." The Sandbox processes were the same before and after the call:
`WindowsSandbox` 4904, `WindowsSandboxClient` 9928, `vmmemSandbox` 28184.
That Sandbox was closed by hand afterwards.

## Guest timeout

With `PSTOFF_SANDBOX_TIMEOUT=20`, a build whose guest was still restoring its
build cache (about 200 seconds in the run above) was reported as "Windows
Sandbox timed out; the run could not be completed." and exited 2 after 22
seconds. No `finished.txt` was written. The launched `WindowsSandbox` process
was gone on return; its `vmmemSandbox` worker exited within 10 seconds.

## Worktree unchanged

A SHA-256 manifest of every file in this checkout except `.git` (1,309 files)
was identical before the probe, after the probe and after the concurrent
build. `git status --porcelain --ignored` in the temporary worktree was empty
after its build. Run output went only to
`%TEMP%\PstOffSandboxCargo\<worktree digest>\output-v2`.

## Documentation

`docs/agents/safety.md` and `docs/agents/environment.md` name
`scripts/sandbox-cargo.py` and `scripts/check` as the build route. Nothing in
the repository calls `scripts/cargo-sandboxed`; this delivery removes its last
stale `.gitattributes` entry.

## Observation

After a launched Sandbox is closed, its `vmmemSandbox` worker can outlive the
wrapper by a few seconds. The concurrent run shows a queued call launching
one second after the previous release still succeeds.
