# Issue #24 tracer validation

The implementation starts from integration tip
`1c34f6330d83c41a1f121f63e8295732697fd375`. The engine reuses the rebuilt writer
from `ticket/20-writer` (`49aa687`) rather than the older format converter's
unsafe oversized-heap splitting. The writer emits v23 Unicode PSTs with
512-byte pages, default permutation encoding and password checksum zero.
Microsoft's pinned writer supplies allocation-map finalization.

## Automated acceptance checks

All Rust builds, tests, formatting and lint run through the Windows Sandbox
wrapper. Inputs are deterministic generated v36 OST bytes; the shared test
builder was copied from the existing synthetic-fixture task. It reads no
external source files and is also used by the CLI smoke test.

- Independent `outlook-pst` readback compares every inspected folder's display
  name, hierarchy row count and Item count. Empty folders are included.
- Store property 3001 uses the source file stem, independently of the chosen
  output base name. The source remains byte-identical after conversion.
- Reports deserialize through the public report type and contain the source
  path/SHA-256, PST path/size, every folder's counts and the mailbox-completeness
  statement with its explanation.
- Progress before publication observes no final PST. Cancellation removes
  staging. Existing PSTs, reports and staging entries are preserved. A final
  path created during conversion is preserved, with published reports rolled
  back when exclusive rename fails.
- `pstoff convert <ost> --out <dir>` exits zero and writes exactly the PST and
  two reports. Unsupported input still exits three and writes nothing.

## Recorded run

On 2026-10-26, the full Sandbox check passed: 39 dependency packages audited,
formatting clean, Clippy clean and 31 tests passed. Command on the host:

```powershell
$env:PYTHONUTF8 = '1'
python scripts/check
```

UTF-8 mode avoids the host cp1252 failure when piping Unicode dependency
metadata to the audit. No host Cargo command was run. Red runs observed the
root-folder rejection, unimplemented conversion API and missing reports/CLI
syntax before their implementations; the publication-race test also caught
`std::fs::rename` replacing an existing Windows destination. Exclusive
`MoveFileExW` publication made that test green.

The PR #49 standards follow-up makes the host metadata pipe explicitly UTF-8
and reads audit input as JSON bytes. The wrapper also emits guest logs as UTF-8.
The documented bare `python scripts/check` therefore no longer needs the
`PYTHONUTF8` workaround used in these historical runs.

## Open validation

No public OST fixture is tracked or present in `.scratch/spec-notes-20`.
The issue's public-fixture-specific roundtrip remains **open**. Synthetic
results do not establish public-fixture acceptance. No previous attempt's
SCANPST result applies to this implementation. Strict SCANPST, libpff
comparison and manual classic Outlook checks have not been performed for
this change; no Faithful claim is made.

This first tracer requires a readable source and the supported system-folder
layout. Missing or ambiguous system folders fail conversion. It rejects
exclusions and non-default explicit split sizes, and fails before publication
if the default limit would require splitting. Cancellation is observed before
work and before publication, not within individual writer operations.

## Integration with issue #23

On 2026-10-26, conversion tip
`3b1d7f74a9da94f2e34ff95950279d04b15dcb04` was integrated on top of
`2c4ae0633414f3de6ca6cfb5285b64c24d2825e9`. Conflict resolution preserves
issue #23's synthetic builder, including its numeric PidLidHeaderItem mapping
and deliberate unreadable Item/folder support. Conversion tests default the
additional folder fields and share that builder. The reader retains both
conversion source hashing and the root-only self-parent exception.

No Windows Sandbox app was running before the check. With `PYTHONUTF8=1`,
`python scripts/check` completed through Windows Sandbox with exit code 0:
39 dependency packages audited, formatting and Clippy clean, 34 tests passed,
0 failed, and 1 intentionally ignored libpff export test. This includes the
issue #23 synthetic tests, five engine conversion tests and CLI conversion
smoke test. The staged diff passed `git diff --cached --check`.

The public-fixture roundtrip and Faithful checks described above remain open.
No private mail data or Outlook/PST candidate files were accessed.

## PR #49 standards follow-up

Based on PR head `92f2b870c187f1933da1e621d2f758082bbd4eaa`, the focused
follow-up removes the Linux CI workflow and Linux workspace/build claims,
fixes the host UTF-8 metadata pipe, and routes synthetic validation through
the `libpff-check.py synthetic-check` entry point. CLI usage and error-exit
mapping are shared helpers; engine behavior is unchanged.

On 2026-10-26, bare `python scripts/check` passed with host Python UTF-8 mode
off and the default `cp1252` locale: 39 packages audited, formatting and Clippy
clean, 34 tests passed, none failed, and the opt-in export test ignored.
A separate audit using UTF-8 JSON with non-ASCII metadata also passed with
`PYTHONUTF8=0` and `PYTHONIOENCODING=cp1252`. Python syntax checks and
`git diff --check` passed.

After the other Sandbox session closed, the opt-in `export_for_libpff` test
passed through `sandbox-cargo.py`. The documented WSL
`libpff-check.py synthetic-check` command passed under bwrap with libpff
20180714: two healthy samples and two expected independent read failures.
Only the four newly generated OSTs were read. Their hashes match the existing
[#23 evidence](evidence/issue-23-synthetic.json). No private or unknown mail
files were accessed; the Faithful/public-fixture gates remain open.
