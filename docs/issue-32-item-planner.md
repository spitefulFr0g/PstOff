# Issue #32: bounded Item policy planner slice

Based on `73359c5` (`t3/bounded-table-rows`), parent `1ad8b1f`.
Development uses the isolated `t3/bounded-item-planner` worktree. Generated
fixtures only; no private/unknown mail or Faithful acceptance.

## Ownership and semantics

`cache::prepare` no longer retains a whole-store property map, occurrence
vectors, a union of required Item tags, or complete ChangeKey/PCL operands.
A disk radix index maps global Item NIDs to linked, explicitly encoded
80-byte observation records. Each record includes its table, schema/presence
bits and three fixed 18-byte `(kind, offset, length)` blob descriptors.
The schema bits distinguish an absent column from an absent cell. All offsets
and lengths are checked 64-bit values. The existing spool implementation uses
exclusive creation/sharing, Windows delete-on-close and RAII cleanup.

The planner-specific table read copies policy subnode chunks directly into a
blob spool. Heap operands are bounded by a source heap allocation. It skips
non-policy payload materialization; it still validates row/index identity,
versions, policy column ambiguity and structural aliases. Source Item policy
PC heaps are spooled block by block; their BTH walk retains at most one
allocation per depth level (maximum 32), and only three operand descriptors.
Subnode locators are resolved using the original PC/table subnode namespace
while the immutable source remains held; saved blob ranges cannot confuse
identical local NIDs from separate contexts.

Each Item walks every occurrence before any decision is published. Q12 checks
all applicable FILETIMEs against one fixed operand. Q15 parses complete
SizedXids using an 8 KiB input buffer, a previous 16-byte namespace and one
24-byte XID; equality uses two 8 KiB buffers and compares the entire value.
No PCL size cap or truncation is introduced. Existing Item pair members must
agree too. Reversed disk-chain traversal changes which occurrence is the
reference, never the agreement requirement. Conflict diagnostics identify
the table, Item, folder and exact policy tag without mail payload text.
Recovery diagnostics retain a count and one evidence table rather than a
string containing every occurrence's table ID.

Only validated recovery descriptors and their chunk-copied payload ranges persist
in `cache::Plan`; the input observation blob is dropped after planning. Emission reads
the current Item PC and applies those recoveries. Q16 selects the exact tags
from the current contents-table schema, loads the current Item on demand and
merges its recoveries. Source-local references remain local for relocation;
recovered bytes become output-owned values. All decisions precede output
creation, including conflicts discovered at the final search occurrence.
Cancellation interrupts reads, disk-chain traversal, PCL parsing and chunked
comparisons. Partial plans and observation/blob/index spools drop on errors
or cancellation. I/O failures remain fatal I/O failures.

## Regression scope

Synthetic disk-index tests grow 16/128/1,024 Items with 32 occurrences each and
assert less than 64 KiB additional thread-local live heap allocation for the
planner's disk ownership and comparison operations. Full source planner tests
use those same counts and occurrence populations, check recovered exact bytes,
and bound additional live allocation below 2 MiB **including** the existing
count-dependent differences vector for this tested domain.

Large-PCL tests stream and compare 90,000 SizedXids (2,250,000 bytes), reject
truncation, ordering errors spanning a buffer boundary and a final-byte
conflict, and exercise cancellation. A generated source with two Items and
three search occurrences verifies full recovery and rejects a conflict in the
last occurrence; planner allocation stays below 1 MiB, source SHA-256 is
unchanged, and emitted recovery operands retain every PCL byte. Separate
source-PC tests use equal local NIDs in distinct namespaces with different
payloads. Existing conversion/Q16 tests cover output readback, exact-type
removal, row versions, associated Items, rollback and cancellation.

These scoped allocation assertions include allocator capacity but are neither
a shared reservation budget nor a whole-process private-commit measurement.
Fixture generation and independent readback are outside those scopes.

## Exact remaining owners

- Source inspection/recovery: `reader::item_properties`, `ltp::read_node_pc`,
  `ltp::read_pc` and format-reader heap/property collections still materialize
  individual PCs and payloads. Source/recovery/selection node vectors,
  membership collections and `Pst::subnodes` maps remain count-dependent.
  The planner's calls to the latter still inherit that source-metadata owner.
- Current Item/row emission: `context::read_properties`, `selected_properties`,
  relocation heap/property maps, `Plan::apply`, normal `table::read`,
  `Rows::visit`, Q16 replacement/cloning and PC `HeapBuilder::bth` still own
  individual property sets or variable values. A huge single output PCL/cell
  can still materialize there. The new policy-read mode bounds planner
  operands only; it does not fix these writer/reader interfaces.
- Writer/index metadata: `Output::blocks`, `Output::nodes`, NBT/BBT
  `Output::tree` entry/parent vectors, `subtree`/`subentries` vectors and
  created-folder maps remain. Table/BTH/matrix spools from the preceding slice
  do not replace these owners.
- Reports: `Output::differences`, recovery evidence/skips, public
  `ConversionReport` vectors, `conversion::folder_mapping`, JSON serialization
  and text report buffers remain. Counts/single-table policy diagnostics remove
  the occurrence-sized table-ID string, not the report API's count dependence.
- Finalizer: the pinned, unchanged `outlook-pst` allocation-map page collection
  still scales with output size. An upstream bounded finalizer API is needed.
- Reservations/disk: no shared 16 MiB permit system exists. Planner blob,
  observation/decision and radix files add temp-volume disk costs. Source
  policy values are copied to disk per occurrence. There is no staging-space
  reservation/growth accounting for these files or simultaneous writer spools;
  disk exhaustion propagates as I/O failure. `preflight_space` does not reserve
  these temp-volume owners.
- Measurement: a general file-backed corpus builder and isolated converter
  process/private-commit harness remain necessary, including writer, report,
  finalizer and independent readback coverage.

Neither the 16 MiB payload target nor the proposed 256 MiB whole-process ceiling
is closed. Story 35 remains blocked by the owners and measurements above.

## Validation

`python scripts/check` passed on the final implementation snapshot in Windows
Sandbox: dependency audit, formatting, workspace Clippy with warnings denied,
workspace tests (including all 11 engine unit tests), GUI smoke-example tests
and all 26 Python harness tests. This is generated-data regression evidence,
not a GUI smoke run, a Faithful check or a whole-process memory measurement.
