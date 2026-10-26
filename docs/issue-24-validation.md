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
