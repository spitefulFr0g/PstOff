# Issue #20: implementable memory contract after the P1 review

Design refinement against `b78c1ee`, not a memory fix or a passing story 35 gate.
The allocation inventory in [the P1 follow-up](issue-20-p1-followup.md) still
applies. No large-store working-set or committed-memory measurement is claimed.
This document specifies the missing interfaces, resource arithmetic and tests
needed to turn the earlier streaming/spill proposal into an enforceable bound.

## Two distinct acceptance contracts

1. **Payload working memory:** at most 16 MiB of explicitly reserved buffer
   capacity across source decoding, policy planning, sorting and writing,
   independent of Item size, folder size, number of search occurrences and PCL
   length. This is a proposed implementation target, not the current behavior.
2. **Whole-process private commit:** a fixed ceiling requires bounded metadata,
   streamed reports and a bounded finalizer as well. A payload-only fix cannot
   pass this gate. `ConversionReport` currently returns all records in `Vec`s;
   the pinned, unmodified finalizer materializes all allocation-map pages.

Do not hide these owners in an unbounded "metadata allowance", subtract them
from a measured process peak, cap supported Item sizes, or reduce Split limits
and then declare story 35 complete. A job-object memory limit would terminate a
conversion; it would not provide the required large-store conversion algorithm.

## Necessary lower bounds beyond property payloads

The following are field/payload arithmetic, excluding allocator overhead,
collection spare capacity and hash-table control arrays. They are **lower
bounds**, not a process-memory estimate or upper bound. MiB is 1,048,576 bytes.

| Current owner | Necessary retained storage | Example |
| --- | --- | --- |
| Source BBT in `Pst::block` | One `u64` key plus `Block` per entry; 32 bytes on the Windows x64 target | 2,621,440 blocks: 80 MiB before hash-table overhead |
| Writer `Output::blocks` | `(u64, Vec<u8>)` headers plus 24-byte BBT entry; 56 bytes per block on Windows x64 | Same block count: 140 MiB before vector spare capacity and per-entry allocations |
| Source/recovery/selection node vectors | 24 bytes per `Node`, per live full copy, plus membership collections | One million nodes: 22.89 MiB per copy |
| Returned differences | Two `String` headers alone occupy 48 bytes per `Difference` on Windows x64, before other fields or actual text | One million differences: at least 45.78 MiB of string headers alone |
| Finalizer AMap bitmaps | `496 * ceil((F - 0x4400) / 253952)` bytes for file end `F > 0x4400` | At 20 GiB: approximately 40.00 MiB; at 50 GiB: 100.00 MiB, excluding page objects and caches |

The block-count example corresponds to 20 GiB / 8192-byte slots; it is an
illustrative possible population, not a measurement or an assertion that every
20 GiB source has that block count. The source BBT and writer records coexist
during emission. The writer is dropped before finalization, so its 140 MiB must
not simply be added to the later finalizer peak. Source metadata and returned
report records remain relevant across phases. Even replacing all payload owners
with 16 MiB of buffers leaves multiple independent, unbounded count terms.
There is also a two-reader peak: `conversion_source` retains its `Pst` while
`reader::inspect(path)` opens a second one. Both load their own BBT through
property reads. In the example population, that is at least 160 MiB of BBT
entries during inspection, before its Item properties, folder summaries and
hash-table overhead. Streaming only emission would not remove that peak.

Giving just the finalizer's **bitmap bytes** 64 MiB permits at most 135,300
AMap intervals: `(64 * 1024 * 1024) // 496`. That covers about 32.00 GiB, before
any other finalizer allocation. A constant whole-process bound for arbitrary
store size is impossible with that retained vector. Raising a limit only moves
the failure point.

## Interfaces that must change together

Keep the source's existing read-only, deny-writer handle alive throughout the
conversion. A descriptor references that handle's immutable data, never a path
reopened later. Its location includes its **subnode namespace**, not just a NID:
attachment-local and embedded-message NIDs are not globally unique.

Replace `Cell.bytes` for unbounded values with one of:

- Inline bytes, with a maximum of eight bytes.
- A validated source heap allocation: context/namespace identity, heap root,
  page/allocation location and length.
- A logical source stream range: context/namespace identity, root BID, offset
  and length.
- A staging blob range: spool identity, offset and length.

Use checked 64-bit offsets and lengths in the spool even where an individual
PST structure uses 32 bits. The streaming cursor validates block CRCs, inflated
lengths, XBLOCK levels, child kinds and `cbTotal` before accepting a range;
truncation is a source error. Source blocks can inflate to a 16-bit length, so
budget for a 64 KiB decoded block, not only the writer's 8176-byte block size.
There must be no `node_blocks(...).concat()` on this path. An object reference
retains its original local NID and namespace, not an inlined object copy.

The cursor retains only the current block and a depth-limited traversal stack.
A depth limit alone is insufficient if each stack frame owns a whole child
vector. Page lookup must use a bounded cache or disk-backed index, rather than
initializing `Pst::bbt` on the first cursor read.

`reader::item_properties`, `reader::inspect` and `recovery::validate` currently
materialize values too. Converting only the writer to descriptors would still
allow the initial inspection or recovery phase to exceed the budget. Their
evidence checks need streamed type/shape validation and bounded operands.
PC tag ordering/uniqueness must be validated incrementally; one Item's descriptor
set must also spill if it does not fit its reservation. Moving a whole PC into
a descriptor `BTreeMap` is not an exemption from the shared budget.

For table reading, separate TCINFO/column validation, BTH traversal, row
traversal and value resolution. A row visitor must not own a `Table` containing
every row. Source row width is a 16-bit field: one 64 KiB row buffer suffices,
including oversized source rows that emission will reject as unrepresentable.
Validate row/index agreement by sorting `(row NID, row ordinal)` records on
disk. Preserve absent-cell bits, row versions and exact property tags. Table
structure/heap alias checks also need a disk index; replacing the row vector
while retaining an unbounded `BTreeSet` of structural HIDs is insufficient.

## One shared reservation budget

Proposed maximum simultaneously reserved capacities:

| Use | MiB |
| --- | ---: |
| In-place sort arena, containing fixed-width descriptors only | 8 |
| Source decoded blocks and bounded source-page cache | 2 |
| Merge input/output buffers and merge heads | 2 |
| Writer block, row, heap-page and output buffers | 1 |
| Disk-index page caches | 1 |
| Policy comparison/parser buffers | 1 |
| Traversal stacks, descriptor headers and reserve | 1 |
| **Total** | **16** |

Reservations must precede allocation and include vector **capacity**, buffer
headers, sort scratch, caches and temporary copies. A shared permit follows
ownership; a clone cannot duplicate capacity without another permit. An
in-place sort avoids hidden merge-sort scratch. Exceeding a sub-budget causes
spill/cache eviction or a smaller run, never silently borrowing an unlimited
allocation. A single huge value is streamed; it must not produce a "record
larger than sort budget" exception. This budget is not yet enforced anywhere.

## Cache planning and spool costs

Use a fixed 48-byte encoded observation descriptor, independent of payload
size: Item NID (4), table NID (4), exact tag (4), schema/presence flags (4),
namespace/context spool ID (8), source-or-blob locator ID (8), value offset (8)
and value length (8). Schema flags must distinguish an absent policy cell from
an absent policy column. Store validated row identity/version separately from
policy values; they are not Item properties. The byte encoding is explicit,
not `size_of`/Rust struct serialization.

An 8 MiB arena holds at most 174,762 descriptors. Sort by Item NID, table NID
and exact tag. Merge at fan-in 16 with 64 KiB per input and 64 KiB output
buffers (1.0625 MiB, leaving the rest of the merge budget for heads and state).
Re-sort additional passes when there are more than 16 runs; never open or
buffer every run simultaneously.

For ten million descriptors: 480,000,000 bytes (457.76 MiB) on disk, 58 initial
runs, and two merge passes (58 -> 4 -> 1). Initial run writing plus two full
read/write passes is five descriptor volumes, about 2.24 GiB of I/O. A pass can
temporarily hold both old and new runs, so reserve two descriptor volumes,
915.53 MiB, rather than assuming a 16 MiB memory budget also bounds disk usage.
Retire input runs only after their merged output is complete; cancellation and
I/O errors clean up the private staging directory under the existing rollback
contract. Handle exhaustion and disk exhaustion must be explicit I/O failures.

Do not spool body/attachment bytes merely to index them: validated immutable
source ranges can be reread. Generated/recovered bytes without a stable source
range use a blob spool. If `V` bytes must be spooled and recovery descriptors
occupy `D` bytes, a conservative additional staging reservation is
`2 * 48 * O + V + D`, where `O` is the descriptor count. Output candidate and
last accepted Split part, row/BTH/index spools and report spools are additional
terms; the current source-size/declared-Item-size preflight does not cover them.
During scanning, checked counters must grow that reservation or fail with
actionable free-space guidance before filling the destination.

Grouping must not load every occurrence for an Item. Compare all Q12 values
with the bounded first validated operand. For Q15, parse SizedXids incrementally
and compare complete keys/PCLs in chunks across **all** occurrences and existing
Item properties before publishing any recovery decision. Keep only namespace
ordering state and one XID (at most 24 bytes), never a complete PCL. PCL length
must neither be capped to 1 MiB nor truncated to fit the sort arena. A conflict
in the final search occurrence must fail exactly as a conflict in the first.

The current difference diagnostic formats every table ID into a `String`.
That is another occurrence-count allocation. Replace it with a fixed-size
record/count and a streamed evidence reference; do not retain a `Vec` of table
IDs in the new planner. Reports must still identify conflicts with their table,
Item and property, without payload text.

Persist only validated recovery decisions, keyed by Item NID and exact tag.
`Output::planned` becomes a disk lookup of descriptors, not complete Item PCs.
Q16 reads only the exact Item tags needed for the current row and merges any
validated recovery decision; cache-only values still follow the current removal
and difference rules. Row matrices, row-index BTH records, heap pages and BBT/NBT
entries must be emitted or spooled incrementally. Replacing `planned` alone
leaves both `Table::rows` and `Table::build` unbounded.

## Whole-process blockers and required decisions

The current public API guarantees full vectors in `ConversionReport`. Preserving
that API preserves count-dependent memory even after reports are serialized.
A bounded alternative returns report paths/counts plus paged/streamed records;
an explicit opt-in `collect_report` can offer the existing materialized result
without claiming the same memory guarantee. Callers in CLI/GUI and saved-report
schema compatibility need a deliberate migration, not an incidental change to
an error path.

The mandated finalizer also needs an upstream bounded allocation-map API that
walks/writes one AMap interval at a time with a fixed page cache. It must retain
the existing final-file sizing, independent readback and rollback behavior.
Otherwise the acceptance contract must explicitly permit a store-size-dependent
finalizer bound and state a supported size domain. Neither changing the pinned
dependency nor weakening the whole-process contract is authorized by this
design note. No such change is made here.

## Ceiling regression to implement after the interfaces exist

The current synthetic builder owns all bytes and permits only 126 NBT entries
and 169 BBT entries. It cannot generate a meaningful large-store ceiling test.
A separate deterministic file-backed builder must write data blocks immediately,
use multilevel NBT/BBT indexes, spool table matrices and generate payload bytes
from `(Item NID, offset)` without a whole-store `Vec`. Validate that builder on
small cases with both readers before using its outputs as performance evidence.

Run fixture generation, conversion and independent readback as three separate
processes **inside the Windows Sandbox wrapper**. Measure the converter from
process creation, including source inspection, recovery and finalization. Use
Windows `GetProcessMemoryInfo` peak pagefile usage (private commit high-water
mark), not RSS samples, source file size, average allocation size or a counter
that excludes the finalizer. A dedicated child avoids fixture-generation or
independent-reader allocations contaminating the result. Do not start a child
that inherits another converter's high-water mark.

| Corpus axis | Required generated cases |
| --- | --- |
| Growing store, fixed Item size | 256, 1,024 and 4,096 Items with distinct 256 KiB binary streams (64 MiB, 256 MiB, 1 GiB logical payload) |
| Folder concentration | All Items in one folder, then distributed over 64 folders |
| Search occurrences | Same Items in 1 and 32 search-contents occurrences, with the last occurrence conflicting in a negative case |
| Unbounded single value | 256 MiB by-value attachment, including nested attachment-local namespaces |
| PCL parser | 2 MiB valid PCL; cross-chunk length/order errors; conflict in its last chunk |
| Metadata/finalizer growth | Many small blocks/Items and increasing finalized file sizes, independent of body size |

For the payload phase, enforce the shared reservation high-water mark <=16 MiB
for **every** case, and verify exact streamed hashes, counts, source SHA-256 and
Q12/Q15/Q16 outcomes. It is not enough for increasing Item counts to have a
similar process-memory slope while a single huge attachment still materializes.
For the whole-process gate, propose an absolute 256 MiB private-commit ceiling
for this corpus only **after** report and finalizer changes. That number is an
acceptance target to validate, not a proven ceiling of any current code. Record
both private commit and reservation high-water marks, with build profile,
toolchain/dependency revisions and fixture parameters. A passing small fixture
does not establish a bound beyond the tested domain.

There is no large-store memory-ceiling regression in the current change and
none is reported as passed. The next implementation must land the file-backed
builder and isolated measurement harness alongside the streaming readers,
spooled planner/writer, report API decision and finalizer route above. Story 35
remains a P1 blocker until the applicable gates pass.
