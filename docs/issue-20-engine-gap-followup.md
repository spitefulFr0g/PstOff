# Issue #20 engine follow-up to PR #49

Base: `92f2b870c187f1933da1e621d2f758082bbd4eaa` (remote PR head verified before work).
Branch: `fix/issue-20-engine-cli`. GUI files are unchanged. This is a partial
implementation and does not make PR #49 ready to merge.

## Implemented

- Cooperative cancellation during cache planning and between Item writes. The
  writer returns cancellation through the existing conversion outcome; staging
  cleanup removes unpublished output. Progress reports the current folder,
  actual Items done/total and a monotonic fraction, reserving finalization work.
- XBLOCK `cbTotal` sums actual child lengths, including short non-final blocks.
  The regression first reproduced 9,176 declared bytes for 9,000 actual bytes.
- Synthetic conversion coverage for message, appointment, contact, task, note
  and journal classes. Microsoft outlook-pst reads back every class and binary
  payload; an output-format invariant check verifies XBLOCK totals because both
  bundled readers otherwise tolerate the defect. Tests use `convert` and inspect
  its saved output, without calling writer internals.
- Text reports include part numbers, explicit evidence status, evidence local
  references, Skipped records/reasons, and property differences with local
  references, sources and reasons. Existing JSON structure is preserved.

## Validation

All builds and Rust checks use Windows Sandbox through `scripts/sandbox-cargo.py`
or `scripts/check`. Only generated synthetic inputs were used. No private or
unknown mail inputs were inspected. Each regression was observed failing before
its implementation, then passing. The conversion test target has seven tests,
including collision preservation, report rollback, source byte identity,
per-Item cancellation cleanup, and independent output read-back.

Final full check: `$env:PYTHONUTF8='1'; python scripts/check` passed: dependency
audit (39 packages), formatting, clippy with warnings denied, and 36 tests;
one explicit export test remained ignored. UTF-8 mode avoids a host cp1252
encoding failure while passing dependency metadata to the audit. The first full
run also found one report formatting issue, corrected before this passing run.

## Remaining non-UI blockers

- Unreadable Items/folders still abort conversion. They need isolated validation,
  skip records, repaired folder counts/table rows, and retained readable siblings.
  Rendering Skipped records is implemented, but the engine does not produce them.
- Headers-only, unreadable-content and external-reference evidence checks remain
  Not checked. Q12/Q15 cache conflicts still fail the whole conversion.
- Cancellation is now checked during writer planning/emission. Initial source
  hashing and the preceding inspection pass are still synchronous and cannot
  respond to cancellation until they return. No Split planner exists, so keeping
  finished parts on cancellation still needs implementation and coverage.
- Split planning, exclusions, whole-Item size accounting, oversized-Item outcomes,
  and populated-folder/ancestor selection are not implemented.
- Output free-space preflight and actionable stale-temporary handling are absent.
  Existing temporary names are preserved and reported as collisions.
- CLI option parity, stderr progress, distinct stable status codes, Ctrl+C wiring
  and CLI cancellation smoke coverage are still needed. CLI files are unchanged.
- Broader content/attachment/cache-conflict fixtures and failure injection remain.
  This follow-up has not established bounded memory for large stores.
- Strict SCANPST, libpff and manual Outlook acceptance have not been rerun; no
  Faithful verdict is claimed. The requested validation route was Cargo Sandbox
  wrappers only.
