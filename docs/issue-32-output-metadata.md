# Issue #32: bounded output indexes and allocation-map finalization

Base: `t3/bounded-item-planner` at
`66919450e9af33aedc724139429b176afab86602`, whose direct parent is
`73359c5e04cf76ae86678a7a203fa240197123f2`. The implementation branch
`t3/bound-writer-metadata` fast-forwarded from its ancestor `922c8ad` to this
base before editing. Work stayed in the user-designated Windows worktree.

## Output metadata changes

The writer appends each 24-byte BBT leaf record to the existing private Spool.
BIDs increase monotonically, including internal-block flag bits, so these
records already have the original sorted order. NBT records are 32-byte spool
records; the existing four-level byte-radix disk Index orders their offsets by
NID. Duplicate NIDs fail explicitly. Sorted NBT records are streamed to a second
spool and the ordering/input spools are retired before tree construction.

Each B-tree level reads at most 488 entry bytes into one 512-byte page and
appends one 24-byte parent record to a new spool. The next level replaces its
input after completion. Page grouping, key order, page IDs, placement, trailers,
CRCs and root references match the former in-memory algorithm. The selected
node slice is borrowed rather than cloned into Output. The caller-owned source,
recovery and selection node collections are not made bounded by this change.

Spools use checked u64 offsets, create_new, exclusive Windows sharing and
FILE_FLAG_DELETE_ON_CLOSE, with Drop also closing/removing the path. I/O errors
propagate. Cancellation is checked while ordering and building pages; the
existing partial-output guard deletes unsuccessful candidate output. Index
staging needs disk space proportional to records, including simultaneous input
and output levels. Dynamic staging-space reservation/preflight is still open;
this change does not introduce a store-size cap or claim a disk-space bound.

## Correct finalizer owner and dependency impact

The actual finalizer is Microsoft's `outlook-pst`, not vendored pstfree. With
explicit user authorization it is now vendored from the unchanged source pin
`cfb721daee538acc50d2bad6faac5ce724f6037d`; see THIRD-PARTY.md for provenance,
license, import scope and the dependency audit. Runtime dependency versions are
unchanged. Microsoft readback now uses a locally modified separate implementation,
not an unmodified upstream dependency.

AMap records contain 496 bitmap bytes and an eight-byte free counter on disk.
Rebuilding retains one record at a time; BBT/NBT traversal retains the existing
bounded-by-level page stack and decreasing-level/cycle checks. PMap and FPMap
are lazy page iterators. FMap summaries read one disk record at a time. The
128-byte header summary and checked free-byte sum are computed in a bounded
scan. No whole-file map-page vector remains in the production finalizer.

The original inclusive PMap range, FMap/FPMap schedules, final file extensions,
map bit conventions, free counts, density-list handling, IDs, header CRCs and
lock/flush sequence are preserved. The new Unicode finalize_with_cancel entry
point checks cancellation during staging, traversal, summary scans and writes.
The engine drops the finalizer before accepting output; failures and cancellation
retain the existing candidate rollback semantics. The generic lock API also uses
the disk-backed algorithm, without a cancellation callback.

## Generated regressions and validation

All Rust builds/checks ran through the documented Windows Sandbox wrappers.
No private mail, host Cargo build, SCANPST, Outlook or libpff run was used.

- Writer oracle: 4,095 records at each NBT/BBT width, multilevel roots and complete
  output bytes compared against the original base writer's test-only algorithm.
- Large index: 50,000 generated small blocks and reverse-ordered NBT entries,
  production disk ordering and disk-only fixture
  emission, full pstoff-format node/payload readback, plus Microsoft finalization
  and first/last Item block readback. Allocation gates apply separately to writer
  index emission and finalization; materialized readback is outside those gates.
- Finalizer oracle: complete finalized files compared byte for byte with test-only
  pinned upstream rebuilding/marking methods for 1, 8, 129, 625 and 8,193 intervals.
  Sparse generated Windows files cross PMap, recurring FMap and first FPMap
  boundaries without allocating whole-fixture buffers. Both file length and all
  bytes are compared; finalized header status and exact block/node reopening are
  checked.
- Finalizer growth: 16 and 2,048 intervals with a fixed 128 KiB local allocation
  delta gate. These tests measured 8,622 bytes for each case in the targeted debug
  Sandbox run. The final reverse-order large-index regression gates combined
  emission/ordering and finalization separately below 128 KiB.
- Cleanup: malformed/truncated writer records, cancelled tree construction and
  candidate removal; truncated AMap spool I/O and cancellation close/remove its
  path; tested finalizer cancellation points during initialization/scanning/writing
  leave the candidate header invalid. The engine discards any cancelled candidate,
  including cancellation observed after map completion.

Targeted command:
`python scripts/sandbox-cargo.py test --locked -p outlook-pst -p pstoff-engine --lib -- --nocapture`
passed its 14 Microsoft-library and 14 engine unit tests. The final expanded
snapshot then passed `python scripts/check` (exit 0) on Windows x64 MSVC, in the
wrapper's debug/test profile:

- Dependency metadata/license/provenance audit: 376 Windows packages.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace`: 197 passed, zero failed, two ignored.
  The ignored tests are `export_for_libpff` and `write_gui_smoke_fixtures`, the
  opt-in artifact exporters; no acceptance check is inferred from them.
- `cargo test --locked -p pstoff-gui --example gui-smoke`: one passed.
- Guest Python `unittest discover -s tests -v`: 26 passed.

The first full-check snapshot exposed a needless borrow and an incomplete
generated large-index fixture header (`ibAMapLast`); both were corrected before
the passing final run. The final snapshot includes reverse NID disk ordering,
large-index Microsoft finalization/readback and all three cancellation points.
Full guest logs and exit files are retained, uncommitted, under
`.scratch/issue-32-output-metadata/check-final/`. The installed stable MSVC
Sandbox toolchain and locked dependency graph were used; no release-profile
or whole-process memory measurement is claimed.

## Remaining owners and acceptance limits

These allocation deltas are TLS-tracked net live allocations in isolated test
sections. They are neither shared reservation accounting nor process-private
commit measurements. **Neither the 16 MiB reservation contract nor the 256 MiB
whole-process peak contract is established.** No overall ceiling is closed.

Remaining payload owners include complete selected/relocation property cells,
PC descriptor maps, source/recovery property validation and inspection,
per-context subnode collections, and generated/recovered value buffers. The
bounded planner operands and row spools from earlier slices do not establish a
shared budget for these paths.

Remaining metadata owners include source/recovery/selection node vectors and
membership sets, permissive/adopted recovery block maps, subnode maps/vectors,
per-part hierarchy/planning collections and created/omitted state. Output BBT/NBT
records and finalizer maps now spill, but their buffers/traversal stacks do not
participate in a shared reservation permit system. Staging-disk growth still
needs preflight/reservation integration.

Remaining report owners include ConversionReport's file/folder/skipped/difference
vectors, nested Item-reference vectors and Strings, writer differences, inspection
summaries, and saved
report serialization. The public API still requires materialization; it needs
an explicit paths/counts plus streamed/paged-record migration.

An isolated process-from-creation Windows private-commit high-water harness and
the full generated payload/occurrence/PCL corpus remain necessary. There is no
new end-to-end process-memory measurement or Faithful acceptance in this slice.
