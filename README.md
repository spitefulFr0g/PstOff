# PstOff

PstOff is an MIT-licensed Windows OST-to-PST converter under construction.
The engine and CLI inspect supported v36 OSTs and perform a first Copy mode
conversion to a conventional Unicode PST. The GUI is a guided wizard over
the same engine calls.

`pstoff convert <ost> --out <dir>` writes `<source-stem>-part001.pst` (and
further numbered parts as needed), plus JSON and
plain-text Conversion reports. The PST display name uses the source file stem.
The source is opened read-only with concurrent writes/deletion denied. Output
is staged in an exclusive temporary directory and renamed without replacing
existing files; the PST receives its final name only after it is complete.
The reports state **Mailbox completeness: unknown**: an OST contains only
what Outlook synced, so content never cached cannot be recovered from it.

The CLI uses the same public engine API as the GUI. Inspect first to see
support status, folder IDs, counts and sizes:

```text
pstoff inspect source.ost
pstoff convert source.ost --out destination --base-name archive --exclude-folder 0x182 --split-limit 21474836480
pstoff convert source.ost --out destination --split-limit off
pstoff convert source.ost --out destination --dry-run
```

`--base-name` defaults to the source stem. Repeat `--exclude-folder` for each
folder ID (decimal or `0x` hexadecimal); descendants follow the exclusion.
`--split-limit` accepts a positive byte count or `off`, and defaults to 20 GiB.
The limit includes finished-file overhead and keeps each Item whole. Report
names are `<base-name>.json` and `<base-name>.txt`, independent of part names.
Progress (percentage, Item counts, part and current folder ID) goes to stderr;
results go to stdout. Ctrl+C cooperatively cancels, removes unfinished staging
and keeps finished parts. Conversion is always Copy mode.

`--dry-run` prints a JSON engine plan without creating the destination, PSTs,
reports or temporary files. The plan includes output paths, whole-Item part
counts, finalized layout sizes, an absolute per-file size tolerance (currently
zero bytes), and a warning if any file exceeds Outlook's default 50 GiB limit.
It reads and decodes the selected source content, so planning can take time.
Conversion recomputes the same plan and verifies each finalized size before
publication. Source changes invalidate an earlier preview. Destination capacity,
collisions and I/O errors can still prevent a planned conversion.

Reports use schema version 3, retain all earlier report fields, and add selected
excluded folder paths and paths alongside local folder/Item references. Older
reports deserialize with empty path/selection fields. See [the planning and
report contract](docs/shared-plan-report.md).

Stable exit codes for both commands:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Other failure (including output collision or invalid engine options) |
| 2 | Command-line usage error |
| 3 | Unsupported input |
| 4 | Input in use |
| 5 | Insufficient space or an oversized Item under Split |
| 6 | Conversion succeeded with Skipped items |
| 130 | Conversion cancelled |

Run `pstoff --help` for the command syntax. See
[the #24 validation record](docs/issue-24-validation.md) for the earlier
milestone's evidence and outstanding Faithful acceptance checks.

## Prepare and check

Use the Windows Sandbox toolchain and mappings described in
[`docs/agents/environment.md`](docs/agents/environment.md). From this repository:

```powershell
python scripts/check
```

Every Cargo command, including the first build and test, goes through the
Windows Sandbox wrapper. It fetches locked dependencies on the host without
running crate code, then maps the source, dependency cache and toolchain
read-only into the guest. `scripts/check` checks dependency licenses/pins,
formatting, Clippy and engine/CLI/GUI tests, including headless GPUI tests of
the wizard. It does not execute the imported fork's upstream tests.

```powershell
python scripts/sandbox-cargo.py test --locked -p pstoff-engine
python scripts/sandbox-cargo.py run --locked -p pstoff-cli
```

All builds follow [safety](docs/agents/safety.md).

## GUI

`pstoff-gui` is a GPUI wizard pinned to the `gpui-pre` 0.3.9 snapshot audited
by the Windows feasibility probe: Add OST files → Choose folders → Output →
Convert → Report. It calls the engine's public `inspect`, `plan` and `convert`,
runs them off the UI thread, and shows the engine's statuses, errors and
Conversion reports. Folder exclusions and Split use the engine's public options.
The file preview uses engine layout measurements, including whole Items and
PST overhead. It waits for the current output options to finish planning on a
worker before enabling conversion. OST paths on the command line are added on start, as when files
are dropped on the `.exe`; `--out <folder>` picks the destination. The GUI is
launched only inside Windows Sandbox: see the smoke run in
[`docs/agents/environment.md`](docs/agents/environment.md).

## Architecture and tests

`pstoff-engine` owns the platform-neutral `inspect`/`plan`/`convert` seam and typed
outcomes, shared by both frontends. Format code stays behind that seam.
`third-party/pstfree` is the MIT upstream base imported at
`b6e2ff31592f74458f0481d4671251c92044d8f0`, with only its library sources and a
PstOff manifest; it is not a copy of the experimental writer spike. Reader
and writer tasks own its subsequent adaptation. The engine normally depends
on Microsoft's unmodified `outlook-pst` at
`cfb721daee538acc50d2bad6faac5ce724f6037d` for allocation maps and independent
readback. Direct registry versions are exact and all transitive versions are
locked. See [dependency notices](THIRD-PARTY.md).

Integration tests in `crates/engine/tests` import only the engine public API
and independent reader. The support module creates generated bytes in a fresh
sandbox temporary directory, cleans them up and never discovers fixtures by
walking the checkout. The negative matrix verifies unchanged source, no outputs, and classified
rejection of ANSI, older Unicode, unknown versions and encoded stores.
Conversion tests use the complete generated v36 fixture builder and Microsoft's
independent reader to compare folder names, hierarchy rows and Item counts
against inspection. They also check source identity, store display name,
encoding/password policy, reports, cancellation and publication collisions.
Full property, recipient, attachment and embedded-message fidelity remains a
later validation gate.

Public fixture additions require a documented immutable URL/revision, SHA-256,
license and generation/source description. Download into a fresh Windows temp
directory with the research User-Agent, review before use and never execute
downloads. Prefer reproducible synthetic fixture generators. Tests must take
explicit public/generated paths; never accept an environment-controlled path
or glob into `testdata/private`. Do not commit mail content, even from public
fixtures. Committed evidence contains only structural IDs, counts and hashes.
No public mail fixture is downloaded by the full-check command.

Positive readback, strict SCANPST, libpff comparison, Windows portable builds,
and human Outlook and GUI checks remain required later acceptance gates.
See [Faithful checks](docs/agents/faithful-checks.md). Windows Sandbox checks cover only the implementation and generated fixtures; they do not establish Faithful conversion.

### Read-only OST inspection

`inspect` accepts v36 OSTs with 4 KB pages and no encoding. Generated fixtures
exercise compressed/uncompressed blocks, the folder hierarchy, counts and Item
sizes, all six Item classes, folder tables, recipients, attachments, named
properties, XBLOCKs and damaged contexts. Unsupported files return a reason;
invalid supported-format headers/indexes return a damaged-source error.
Unreadable folder/Item contexts remain visible as folder markers. Inspection
uses read-only handles; on Windows it denies concurrent writers/deletion and
classifies sharing violations as In use. The Windows-specific sharing test runs through the Windows Sandbox check route.

Engine integration uses `reader::read_source` to retain the classified source
handle, then the fork's PC/TC/subnode readers. PC types and TC column types,
offsets, widths, existence bits and row IDs are preserved for cache policy;
`read_named_properties` retains GUID and numeric/string identity with explicit
errors. `reader::item_properties` validates typed nested contexts without
filtering Item class. These are internal seams, not frontend APIs. Cache
selection, semantic completeness evidence and output readback belong to the
writer/policy integration. No conversion success is reported by the reader.
