# PR #49 P1 conversion follow-up

Reviewed implementation: `e8b9c870008e71de2dc7d2187bad5701c81784bf`.
Combined standards base: `457437aa6df585a1c09318d78d68a49fd12cf2d3`.
Acceptance contract: issue #20, particularly stories 31, 35, 44, 45 and the
public-API failure/readback testing decisions. This follow-up does not establish
the Faithful acceptance gate or make the integration PR ready to merge.

## Subsequent P1 follow-up from `b78c1ee`

[Source folder identity](source-folder-identity.md) now defines local EntryID
resolution and the conservative failure behavior when identity
metadata is absent. Recovery, selection and store emission share those IDs;
source display names never establish folder roles. Synthetic regressions
cover localized/custom roles and prevent silent scaffolding of an unidentified
healthy hierarchy.

The memory P1 uses the requested design-refinement route:
[the quantified memory contract](issue-20-memory-budget.md) specifies buffer
reservations, spool encoding, merge/disk/I/O costs, metadata lower bounds and
the missing ceiling-test infrastructure. No memory implementation or passing
large-store ceiling test is claimed; story 35 remains blocked.

Validation of the subsequent follow-up (generated synthetic inputs only, all
Rust commands through Windows Sandbox wrappers after establishing no Sandbox
was active):

- Against `b78c1ee`, `python scripts/sandbox-cargo.py test --locked -p
  pstoff-engine --test convert
  localized_system_folder_entry_ids_retain_source_hierarchy_and_payload --
  --exact` failed as expected: the source subtree EntryID pointed to `0x182`,
  but the original engine published a pointer to generated empty folder `0x222`.
- Final `python scripts/sandbox-cargo.py test --locked -p pstoff-engine --test
  convert`: 34 passed, zero failed. This includes six new test functions, source
  byte preservation, independent Microsoft reader row-ID/payload readback and
  explicit missing/invalid identity cases. English names without role metadata
  also fail safely; no name-based compatibility path remains in the engine.
- Final `$env:PYTHONUTF8='1'; python scripts/check`: dependency audit passed for
  376 Windows packages; formatting and clippy (all targets, warnings denied)
  passed; 96 tests passed, zero failed, two fixture-export tests ignored.

No unowned Sandbox was stopped or modified. Competing sessions delayed checks;
waiting wrappers belonging to this task were stopped rather than left queued.
No private/unknown mail input, Faithful check, PR metadata/comment/merge action,
release build or manual application check was used for this follow-up.

## Cache policy and publication

- Q16 contents rows (normal, associated and search contents) use the Item's
  exact property tag after validated Q12/Q15 recovery. A present cached cell
  without that exact tag is removed, including when the Item has the same ID
  with a different type. Each removal records the table, Item, property tag and
  reason. No non-identity cache-only value is retained. Q17's specific
  `66280014` omission keeps its own reason. Absent cells stay absent; schema
  columns do not imply default values. The validated row ID and row version
  (`67f20003`/`67f30003`) remain table structure, not Item properties.
- Q15 cache planning failures reach the public error as `DamagedSource` rather
  than `Io`. Conflict messages preserve the Q15 category, folder/Item identity,
  property and table, with a retry direction. They contain no property payload.
- An unsplit PST remains provisional after its rename until both reports have
  been written and published. Failure rolls back that PST and any reports
  published by this Conversion, and returns no completed files. A colliding
  file created by another caller remains untouched. Split retains its existing
  contract: finished PST parts survive a later report/part failure.

The public-API regressions generate TCINFO-aliased text cells with a missing,
wrong-typed or exact Item counterpart, in both normal and associated contents.
They independently decode output cells and check their difference records.
There are positive Q12/Q15 recovery and negative Q15 conflict cases. Report
failure injection uses the public progress sink at the publication boundary:
directories at the staging report paths force JSON/text file creation failures;
a late final text collision forces failure after JSON publication. Both split
and unsplit modes check output cleanup, completed-file references, preservation
of the other caller's file, and unchanged synthetic source bytes.

## Bounded memory remains an open blocker

The follow-up [memory contract and spill design](issue-20-memory-budget.md)
refines this proposal with a shared 16 MiB buffer budget, fixed spool records,
merge fan-in and disk/I/O arithmetic, metadata lower bounds, required API
changes and an isolated large-store ceiling-test contract. It does not claim
that the implementation is bounded or that story 35 is complete.

No memory fix is claimed or shipped here. The current ownership model cannot
provide story 35's bound by adjusting the Split limit or deleting one clone.
These are static allocation lower bounds, not measured working-set peaks:

| Owner / operation | Retained or transient payload growth |
| --- | --- |
| `writer/cache.rs::prepare` observations | All materialized cells from every selected contents table, across occurrences; retained until planning ends |
| `cache::prepare` result / `Output::planned` | Every selected top-level Item's complete property bytes, including stream-backed bodies/binaries, until part emission ends |
| `table::read` and `Table::build` | Whole table cells/matrix, then rebuilt cells and matrix; a single folder can contain the entire store |
| `context::properties` / `Pst::node_blocks` | Entire logical payload streams; `concat` can temporarily hold the block vector and concatenated copy together |
| `context_with_cells` overrides | Additional full Item property clones, while the whole-part planned map is still alive |

For example, 10,000 Items with one 1 MiB property each require **at least
10,000 MiB (9.77 GiB)** for the planned property byte vectors alone in an unsplit
Conversion. That excludes observations, collection overhead, nested subnodes,
writer clones and tables. At the default 20 GiB Split limit, the selected trial
can approach 20 GiB of property payload; trial finalization is a disk-size test,
not a RAM limit. Reducing the limit is no solution for unsplit conversion or a
single large Item/table. These numbers are arithmetic illustrations; no large
or private fixture was opened or generated for measurement.

There is also metadata growth after payload streaming: source NDB/BBT maps,
selection/recovery node vectors, writer BBT/NBT records and the returned report
grow with Item/block count. The pinned, unmodified Microsoft finalizer's
`PstFileInner::rebuild_allocation_map` allocates all AMap pages in a vector. The
496-byte bitmap for each 253,952-byte file interval alone costs approximately
40 MiB at 20 GiB and 100 MiB at 50 GiB, before page structs/caches. A strict
whole-process constant bound is therefore also blocked by the mandated current
finalizer and the report API returning full vectors.

### Concrete implementation design

1. Introduce raw property descriptors (tag, inline bytes or a source
   heap/subnode location) and an iterator over logical stream chunks. Keep
   `Cell.bytes` only for bounded policy operands. Validate XBLOCK totals and
   depths while walking; do not concatenate attachment/body streams. Preserve
   attachment-local object namespaces and original local NIDs.
2. Traverse tables by row/page. Persist Q12/Q15 observations to a private
   staging spool keyed by global Item NID, table NID and exact tag; externally
   sort/group with a fixed budget (proposed 16 MiB). Validate identity,
   integrity, types, XID/PCL serialization and agreement across **all**
   occurrences before recording a recovery decision. No payload or decision
   may depend on the cache occurrence visited first. Variable-length PCLs
   must be streamed/spooled too, rather than silently capped or truncated.
3. Replace the whole-store `planned` map with a disk-indexed recovery decision
   stream. During Item emission read one Item's descriptors and decision; during
   Q16 emission fetch each row's exact Item properties on demand. Emit large
   cells/streams incrementally, spooling row matrices and BTH records rather
   than keeping a whole folder table. Retain the same difference semantics.
4. Spill BBT/NBT records, selection membership and report records as necessary.
   Serialize report files from the spool. Story 35 needs either a documented
   metadata bound or a report API that does not return every record in memory.
   The unmodified Microsoft finalizer must gain a separately validated bounded
   API upstream (or the spec must authorize another finalizer). Do not replace
   it incidentally as part of a cache-policy repair.
5. Add generated streaming-fixture support: the current synthetic builder holds
   the file in memory and caps its single-page indexes at 126 nodes/169 blocks,
   so it cannot honestly measure large-store memory. In Sandbox, compare peak
   process committed bytes at fixed maximum Item size for increasing Item
   counts (including one huge folder, repeated search occurrences, a large
   attachment and a large PCL). Measure conversion in its own process so
   fixture generation/reader-test allocations do not contaminate the peak.
   Require stable payload memory at a fixed explicit budget, unchanged source
   hashes and independent logical readback. Account separately for metadata and
   finalizer growth until those are also bounded.

This crosses the source NDB/LTP readers, policy planner, context/table writer,
index emission, report API and pinned finalizer. Removing only `planned` still
leaves whole-table payload materialization and complete stream allocation;
calling that change bounded conversion would be misleading. The design above
is a follow-up, not a completed acceptance criterion.

## Independent synthetic fidelity and its limits

Tests use the pinned Microsoft reader to decode positive recipient row IDs,
recipient types and string fields; attachment-table rows; exact 40,000-byte
nonuniform by-value attachment payloads; and normal/associated Item class and
binary properties. Both Item kinds have positive nested content. Counts alone
are not used as the payload fidelity assertion.

For embedded messages, Microsoft's public NDB APIs traverse the parent,
attachment-local and embedded-message subnode trees, read the embedded PC
stream (including its synthetic subject bytes), and compare the exact
18,000-byte nonuniform embedded property stream. Its public table API decodes
the embedded recipient's fields. The pinned high-level embedded attachment
implementation resolves the object's NID through `message.sub_nodes()` (the
parent namespace) rather than the attachment's subnode tree; its embedded
message entry point is private. Full high-level embedded Message property
readback is consequently still a gap. The fixture is not flattened to suit
that implementation, and the dependency is not modified. Raw PC byte presence
does not claim independent semantic validation of every embedded property.

No SCANPST, libpff, public-fixture, manual Outlook or GUI smoke acceptance is
part of this follow-up. None of those gates is claimed as passed.

## Validation

All Rust commands ran through the Windows Sandbox wrapper, after establishing
that no Sandbox was running. Only generated synthetic mail inputs were used.

- `python scripts/sandbox-cargo.py test --locked -p pstoff-engine --test convert`:
  28 passed, including five new regression/readback tests.
- `$env:PYTHONUTF8='1'; python scripts/check` on the combined `457437a` base plus
  this implementation: dependency audit passed for 376 Windows packages;
  formatting and clippy (all targets, warnings denied) passed; 90 tests passed,
  zero failed, two explicit fixture-export tests remained ignored.

The first full run passed audit, formatting and tests but exposed lints enabled
by the increased Rust minimum. Equivalent `is_multiple_of`/`as_chunks` forms
were applied in the engine and the GUI's number formatter, using APIs available
under Rust 1.89. An unused synthetic convenience constructor was removed. The
final full run above includes those corrections. No GUI behavior or dependency
versions were changed by this follow-up.
