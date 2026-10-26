# Engine Split and exclusions (#20)

Based on `origin/fix/issue-20-recovery-evidence` at
`49614294cb81a289e7386c288be5716599e0fa6f` (remote head verified).

The shared no-write planning follow-up is described in
[shared-plan-report.md](shared-plan-report.md). It replaces the trial-file sizing
implementation described below while retaining the same packing and publication
policy. The validation notes in this document describe the earlier branch.

The earlier Split implementation enumerates readable selected whole Items in global node-ID order. Each
candidate is written and finalized in staging, including Microsoft allocation-map
regeneration, before its actual file size is compared with the inclusive byte
limit. An accepted part is published with a numbered name only when its boundary
is established. No published part exceeds the limit. Split Off keeps the original
single-file naming. Empty selections retain the required system folder shells;
a limit smaller than those shells returns InvalidOptions.

Part selection retains populated folders, ancestors and the five required store
folders. Removed global table references are filtered; nested Item subnodes are
kept whole. Folder properties and hierarchy count caches reflect each part.
Exclusions cascade to descendants, remove their Items from progress totals,
folder mappings, skipped records and completeness evidence, and keep required
system shells empty when their contents are excluded. Unknown folder IDs and zero
limits fail before output creation. All source access remains read-only.

An Item that cannot fit an otherwise empty finalized part returns OversizedItem
with folder/Item node IDs, required bytes and retry guidance. Completed files are
returned in the error and preserved. Cancellation and later publication failures
also preserve completed parts and remove current staging. Successful reports map
folder counts and declared Item sizes to each part. Fatal errors return completed
file records rather than writing a partial success report.

Synthetic public-API regressions cover exact and one-byte-under boundaries,
numbering, independent Microsoft reader folder/count and payload readback,
ancestor retention, cascading exclusions, cancellation, oversized Items,
invalid options and source SHA-256 preservation.

Validation is pending: a read-only process check found WindowsSandbox PID 28548
and WindowsSandboxClient PID 10092 owned by another session. No Sandbox was
launched, stopped or changed, and no host build/test was run. Run `python
scripts/check` when the shared Sandbox is available. Formatting/lint/build/tests
and the separate SCANPST/libpff/manual Outlook Faithful gates are unverified for
this change.

Remaining performance limitation: exact trial finalization rewrites the growing
current part for each Item (quadratic work in Items per part). Payloads are not
accumulated across candidates in memory, but existing recovery/node metadata
still scales with the source tree. Space preflight remains an estimate; candidate
and accepted staging files can coexist. A future sizing interface should retain
these exact boundary tests while removing repeated writes.
