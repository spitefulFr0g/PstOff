# Issue #20 CLI contract

Base: `origin/fix/issue-20-split-exclusions` at
`32ad11d8ed94cc351f97af20a10a7364cca0a633`, verified against the remote before
implementation and again before validation. Topic: `fix/issue-20-cli-contract`.

## Implemented

- `convert` passes `--out`, `--base-name`, repeatable `--exclude-folder` IDs
  and `--split-limit <bytes|off>` to the existing public engine API. Defaults
  come from `ConvertOptions`, including 20 GiB Split. `inspect` exposes folder
  IDs for exclusions. Paths remain OS strings; base names require Unicode.
- Progress goes to stderr with percentage, Items done/total, part and folder.
  Output paths, completeness statement and skip counts go to stdout. Report
  paths use the selected base name rather than each numbered PST part's name.
- Stable codes: 0 success, 1 other failure, 2 CLI usage, 3 unsupported, 4 in use,
  5 insufficient space/oversized Item, 6 success with skips, 130 cancelled.
  Inspection uses the same unsupported/in-use status codes.
- A Windows console handler turns Ctrl+C (also Ctrl+Break) into cancellation
  on the engine's shared token. The CLI waits for the engine to clean staging
  and reports finished parts retained on cancellation or fatal errors.
- Synthetic CLI smoke coverage checks defaults, explicit options and folder
  IDs, both exclusion ID encodings, reports, stderr progress, skipped Items,
  oversized Item status, collision preservation, invalid arguments, unsupported
  formats, locked inputs, and actual Ctrl+C with staging cleanup. The Ctrl+C
  driver owns an isolated console so it cannot signal Cargo or the user.
- Engine source/test changes in this branch are formatter output only,
  correcting inherited formatting failures; engine behavior is unchanged.

## Verification

The pre-existing Sandbox (PID 26140) was left untouched. No Cargo/check was
started until it had exited. Rust commands ran only through Windows Sandbox.
All smoke inputs were generated synthetic files. No private or unknown mail
was accessed.

`$env:PYTHONUTF8='1'; python scripts/check` passed (exit 0): dependency audit
(39 packages, GPUI absent), formatting, Clippy with warnings denied, and
58 passing tests, including 11 CLI tests. One existing explicit fixture-export
test remains ignored. The first run passed Clippy/tests but failed formatting;
its audit invocation also hit the known host cp1252 metadata encoding error.
Applying formatter output and using per-invocation Python UTF-8 mode resolved
both issues in the final run.

## Remaining limits

No blocker remains for this CLI implementation. Initial engine source hashing
and inspection still delay cancellation until those synchronous operations
return. The CLI delegates cancellation and completed-part retention to the
engine; it does not interrupt threads or forcibly terminate conversion.

Strict SCANPST, libpff and manual Outlook acceptance were not run here. No
Faithful verdict or completion of the broader #20 GUI/acceptance work is claimed.
PR metadata and merge state were not changed.
