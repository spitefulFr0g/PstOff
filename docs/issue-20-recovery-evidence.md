# Issue #20: recovery, evidence and publication preflight

Base verified after fetch: `origin/fix/issue-20-engine-cli` at
`13c82c5653c91dc796d47bdd9663ab6728559476`.
Topic branch: `fix/issue-20-recovery-evidence`.
This continues the historical checkpoint in `issue-20-engine-gap-followup.md`.

## Implemented behavior

- Validate typed source contexts, tables and nested subnodes before writing.
  Unreadable Items are skipped individually. Unreadable folder properties or
  folder tables skip that folder's subtree. Every indexed affected Item/folder
  has a local reference and a structured `UnreadableItem`, `UnreadableFolder`
  or `UnreadableAncestor` code plus a fixed, content-free reason.
- Remove skipped Items/folders from contents/hierarchy table rows; update retained
  folder counts. Readable siblings survive. If a required PST system folder is
  skipped, create empty output scaffolding and record that difference explicitly.
  Scaffolding does not claim to recover source mail. Broken store indexes,
  ambiguous system identities and unreadable store metadata remain fatal.
- Report schema 2 adds structured skip codes and `Evidence::Incomplete`, which
  keeps positive counts/references and lists unchecked Items. The completeness
  statement remains unknown. Unreadable evidence covers indexed Items omitted
  because of unreadable content or folder boundaries; it cannot enumerate Items
  absent from a damaged/unavailable index.
- Resolve PidLidHeaderItem using GUID and numeric LID from the source name map;
  require PT_LONG and value 0 or 1. Unrelated GUIDs/LIDs/property IDs do not count.
  Invalid marker types/values remain unchecked rather than a checked zero.
- Inspect attachment method and typed reference properties, including nested
  attachments. Accept absolute file paths for methods 2/3/4 and HTTP(S) URL shapes
  for web-reference method 7. References with a payload, missing/invalid targets,
  or unsupported methods stay unchecked. Count Items with valid references once;
  references are informational and are never added to unreadable/missing counts.
  Reference targets are never opened or contacted. This is structural validation,
  not a claim that an external target exists or is accessible.
- Query destination free bytes available to the caller using GetDiskFreeSpaceExW.
  Require twice the greater of source size and retained declared Item sizes,
  plus configurable reserve (64 MiB by default), before creating staging/output.
  A shortage returns InsufficientSpace with required/available bytes. This is an
  estimate; concurrent disk changes and unusual format expansion can still fail.
- Flag any reserved `.BASE.pstoff-partial` path in the destination as a possible
  stale or active conversion, with its path and review/retry instruction. Leave
  all such paths untouched, including files, directories and other base names.
  A live concurrent run is never assumed stale and never deleted.

Property definitions: [HeaderItem](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/pidlidheaderitem-canonical-property)
and [attachment methods](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/pidtagattachmethod-canonical-property).

## Synthetic regression coverage

The engine API conversion tests exercise unreadable Item isolation, damaged
embedded content, unreadable folder subtrees, required-folder/table replacement,
retained counts and table rows through Microsoft's independent PST reader,
source byte identity, a global-NID/local-recipient-row collision (pending), name-map remapping and unrelated identities, malformed
evidence, external references separate from unreadable content, deterministic
insufficient-space rejection using the reserve option, and cross-base stale-stage
preservation. Existing publication-race and cancellation tests remain in place.
No test calls recovery/writer internals. All inputs are generated synthetic bytes.

Sandbox validation checkpoint: `PYTHONUTF8=1 python scripts/check` passed the
39-package dependency audit, formatting, clippy with warnings denied, and
45 tests (one explicit synthetic export test ignored). The conversion target
passed all 16 tests, including the new recovery/evidence/preflight cases.

After that checkpoint, final inspection found that global omitted NIDs must
not filter local recipient-table row IDs. The final commit scopes filtering to
global folder tables and adds a synthetic recipient-row collision regression
(the conversion target now contains 17 tests). It also permits whitespace in
absolute file-reference paths while still rejecting whitespace in web URLs.
These final changes and their manual formatting have NOT been checked.

A separate Sandbox was running GUI tests. Queued local wrappers were stopped
before they started a guest; the other Sandbox was left untouched. The user
explicitly requested leaving final checks pending. Rerun
`PYTHONUTF8=1 python scripts/check` after verifying no Sandbox is active.
Only synthetic data was used. No private or unknown mail was accessed.

## Still open for later work

- Split/exclusions, whole-Item finished-size accounting, oversized-Item outcomes,
  populated-folder/ancestor selection, and keeping completed parts on cancellation.
- Remaining CLI option parity, stable distinct status codes (including skips,
  locked input and insufficient space), Ctrl+C wiring, and cancellation smoke tests.
- Q12/Q15 cache conflicts still fail the conversion; this slice does not guess
  disputed policy values. Broader cache-conflict/failure-injection coverage remains.
- Bounded memory for large stores and responsive cancellation during initial
  hashing/inspection are not established by this slice.
- Strict SCANPST, libpff and manual classic Outlook acceptance were not run.
  No Faithful verdict or readiness to merge is claimed.

No PR, issue metadata, or merge changes are part of this implementation.
