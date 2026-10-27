# Issue #32: bounded report ownership continuation

Base: `cea84a0` (direct parent `55a34d4`). Synthetic data only.

Conversion success now returns `StreamedReport`: each record collection owns a
reference-counted, delete-on-close spool and a count, rather than a record vector.
Iteration is fallible and owns one decoded record at a time. Clones share a read
snapshot; subsequent writes copy records to a new spool. JSON retains schema 2
and its arrays, with streaming serde serialization. Text is written incrementally
through the same fixed-size buffered, cancellation-aware output route. Neither
formatter retains the complete output bytes. Disk/read/serialization failures
propagate through the existing staging rollback: unsplit output rolls back;
finished Split parts remain. The explicit `collect_report` compatibility API
allocates full vectors and has no bounded-memory guarantee.

Recovery skips, evidence references/unchecked references, candidate/accepted
writer differences, accumulated differences, output records and folder/part
mappings use disk-backed records. GUI previews retain at most 50 records per
category; exact item/folder/skip/difference and evidence counts survive truncation.
The distinct-folder summary uses a disk index. CLI output iterates file records.
Report record Debug shows counts only. Engine and GUI difference Debug omit
source/reason.

The writer no longer prepares and retains an arbitrary store property map before
cache planning and throughout the output part. Prepared store descriptors are
spooled immediately and the property map is reconstituted only when emitting the store node, preserving diagnostic ordering.
Store policy comparisons request bounded operands;
unrelated payload subnodes remain local references. Current-node property maps
still exist, as do the remaining payload owners below.

## Verification scope

The synthetic report regression generates 25,000 records per category directly
into spools. It checks streamed JSON byte equivalence with the materialized schema,
text equivalence with the prior formatter, disk-backed deserialization and exact
summary counts. Separate tests cover snapshot isolation, final-owner deletion,
truncation failures, redacted Debug, serialization cancellation at the start,
midpoint and end of a 25,000-record stream, cancelled filtering/copying, I/O
failure and staging/unsplit cleanup. Existing conversion cancellation, Split and report-failure
rollback regressions remain enabled. These are ownership/schema regressions,
not a converter process-memory measurement.

Final validation: `python scripts/check` passed through Windows Sandbox:
workspace formatting, Clippy with warnings denied, workspace tests, GUI smoke
example test and all 26 Python harness tests. The dependency audit passed for
376 Windows packages. This is synthetic regression evidence only; no private or
unknown mail, SCANPST, manual Outlook acceptance or process-peak gate was run.

## Continuation

The next ownership slice and issue #32 acceptance work are documented in
[Progress, Cancel and the measured regression gate](issue-32-progress-cancel.md).
The lists and verification statement below describe the `a787ceb` base; consult
that continuation for the current status.

## Still open at this base

- Source node vectors, `Recovery::nodes`, `Selection::items` and candidate-part
  node copies remain count-dependent, along with inspection folder trees,
  exclusion/omission sets and source-name/named-property collections.
- Subnode namespace maps, sorted subnode vectors, local subentry vectors and
  recovery traversal pending lists remain fan-out dependent.
- Current PC/row property maps, materialized policy application (including PCL),
  individual heap values and source string owners remain. Folder/display names,
  external-reference strings, named-property names and decoded inspection
  strings (including `Recovery::created`/`Output::created` names) have no shared
  reservation enforcement. Removing the part-lifetime
  store map does not establish the payload bound.
- Cancellation/error `completed_files` is still an explicitly materialized
  vector required by that legacy outcome API. Success retains disk records.
- Report spools currently use the OS temporary volume. Destination preflight
  does not reserve their growing bytes there, nor does it cover all policy/blob,
  table, index and Split staging spools. I/O failure is explicit, but per-volume
  checked incremental disk reservations and actionable admission remain work.
- The shared reservation budget must precede allocations and account for buffer
  capacity, caches, parser operands, headers and simultaneous clones. Fixed 8 KiB
  report I/O buffers are not evidence that the shared 16 MiB contract is enforced.
- A deterministic file-backed multilevel large-store builder and separate guest
  processes for generation, conversion and independent readback are still needed.
  Measure converter peak private commit from process creation with Windows
  `GetProcessMemoryInfo` / `PeakPagefileUsage`, alongside shared reservation
  high-water marks, across
  the complete memory-contract corpus. Include inspection, recovery, writing
  and finalization. Do not infer a bound from this report-only regression.

Neither the 16 MiB payload target nor the proposed 256 MiB whole-process ceiling
is closed by this change. Both require separate-process end-to-end
measurement and the outstanding owners/reservations above to be resolved.
