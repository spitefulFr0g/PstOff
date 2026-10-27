# Source logical-stream chunk slice

Based directly on `31d31d9858b8a5285d41bbff7a8bc7490bf28a37` from
`t3/bounded-memory-conversion`. This slice does not close #32 or establish the
16 MiB payload reservation target or the proposed 256 MiB private-commit target
in [the memory design](issue-20-memory-budget.md).

## Implemented boundary

`Pst::node_chunks` borrows the original strict `Pst` handle. It resolves BIDs
using the preceding slice's bounded strict BBT lookup, without loading the BBT
map. It keeps at most two decoded index-block frames, with child positions in
the encoded blocks rather than separate child vectors. A consumer processes
one decoded data block at a time; decoded source blocks can reach 65,535 bytes.
This is a bounded traversal interface, not a shared reservation implementation.
The iterator API permits a caller to collect chunks; production relocation does
not do so.

The cursor rejects invalid internal types/levels, zero children, wrong child
kinds, malformed entry arrays, and both underflow and overflow of each level's
`cbTotal`. Existing strict block reads check BBT/trailer lengths, stored CRC,
and exact inflated length, with bounded inflation. Consumers must reach the end
before accepting a stream: an early chunk is not proof that later chunks or
parent totals are valid. Failed cursors stop yielding.

Raw writer relocation packs bytes across source block boundaries into one
8,176-byte buffer and emits XBLOCK groups incrementally. It keeps no complete
logical payload or complete list of leaf BIDs. It retains at most one group of
1,021 leaf descriptors and one group of 1,021 parent descriptors; the existing
PST representability limits and checked `cbTotal` encoding still apply. Heap
page boundaries are preserved when copying an unreconstructed heap.

PC relocation validates subnode payloads without retaining their bytes solely
to reproduce a local NID. Resolution happens against the current context's
subnode tree, and recursive relocation preserves its namespace and original
local references, including embedded messages. Cache planning materializes
subnode values only for exact tags present in contents-table schemas. Other
subnode-backed Item properties remain local references. Required cache values
still materialize, so Q12/Q15/Q16 behavior is preserved without claiming those
algorithms are bounded. Store/system policy reads retain their existing value
materialization.

Recovery validates raw streams through the same cursor. Cancellation is checked
between chunks in raw emission, recovery validation and selective property
payload reads. A cancelled partial plan is discarded; the existing partial-file
guard and outer cancellation result prevent publishing it. No source path is
reopened by the cursor and no writable source handle is introduced.

## Generated regression coverage

- A file-backed 4,096-block stream (28,676,096 logical bytes), multilevel BBT and
  two XBLOCK levels verifies every byte in order, frame bounds, no source BBT
  map initialization and byte-for-byte unchanged source. The test builder holds
  test metadata; it is not the full ceiling-corpus builder.
- Negative cases reject a CRC error in the final payload block, stored-length
  mismatch, child total mismatch and root under/overflow, wrong child
  kind, cycle, invalid level and truncated child array.
- A compressed 65,535-byte decoded block succeeds; an incorrect inflated-length
  declaration fails. Empty cursors and dropping/restarting a partial cursor use
  the same original source handle.
- A writer-level generated 180,013-byte property verifies omitted versus
  explicitly selected bytes, the preserved local NID, exact repacking/decryption,
  output `cbTotal`, and no further output after cancellation.
- The existing independent Microsoft-reader conversion regression now uses
  180,000-byte by-value attachment and 140,000-byte nested property payloads,
  repeated in ordinary and associated Items. It verifies complete output bytes,
  nested local namespaces, recipient tables and unchanged generated source.

These are functional and bounded-state regressions, not process-memory or
reservation measurements, and not SCANPST/libpff/manual Outlook acceptance.

## Validation

Final `python scripts/check` completed with exit 0 in Windows Sandbox:

- Locked dependency metadata/audit: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace`: 170 passed, 2 existing ignored, 0 failed.
- `cargo test --locked -p pstoff-gui --example gui-smoke`: 1 passed.
- Guest `python -I -S -B -m unittest discover -s tests -v`: 26 passed.
- Host `git diff --check`: passed.

An earlier full-check snapshot failed Clippy on missing parentheses in the
generated fixture's BID arithmetic; the final run includes that correction,
the writer cancellation regression and the 4,096-block stream. Rust commands
ran only through the approved Sandbox wrappers. No private or unknown mail,
SCANPST, independent libpff comparison or manual Outlook check was used.

## Remaining memory owners

- Source inspection and reader Item/property/heap/table decoding still
  materialize values; recovery still invokes those readers and materializes
  typed heaps/tables, named-property metadata and evidence collections.
- PC heaps, property descriptor maps and BTH traversal worklists, source
  subnode maps and sorted subnode lists still grow with their contents.
- Cache observations, required-tag sets, complete planned Item maps, selected
  cache payloads, Q12/Q15 comparisons, complete PCL parsing and conflict
  diagnostic strings remain retained allocations. This slice adds no disk
  planner, descriptor spilling or streamed policy operands.
- Source/selection/recovery/output node vectors, membership sets and folder
  metadata remain count dependent.
- Writer `Output::blocks`, NBT/BBT records and tree-building vectors, subnode
  records, generated heap pages/BTH records, table rows and row matrices remain
  unbounded. Bounded raw stream grouping does not bound these owners.
- Returned report/evidence/difference vectors and strings still need an API
  decision and streamed reporting route.
- The pinned finalizer still materializes allocation-map pages. No dependency
  or acceptance-contract change is made here.
- Shared capacity reservations, spill/index caches, staging-space accounting,
  the full file-backed conversion corpus and isolated peak-private-commit
  measurement harness remain unimplemented.

The source BBT map was removed from strict reads by the preceding slice; this
does not remove any of the other owners listed above.
