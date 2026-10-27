# Issue #32: table traversal and Q16 row emission slice

Based on `1ad8b1f` (parent `31d31d9`). Generated data only. Neither the
16 MiB shared payload reservation target nor a whole-process private-commit
ceiling is closed by this slice.

For the subsequent disk-backed Item planner and current remaining owners, see
[the planner follow-up](issue-32-item-planner.md).

## Implemented storage boundary

`writer/table_storage.rs` stores source heap blocks, row matrix blocks, block
directories and decoded rows in private seekable files. Directories contain
explicit 64-bit offsets. A visitor owns one decoded row at a time, preserving
exact tags, absent cells, original row order and row versions. Source heap HID
decoding uses the source version, including v36's expanded allocation index.

`writer/spool.rs::Index` replaces table row-identity, ordinal, duplicate and
structural-HID collections with a four-level disk radix index. Each lookup
reads eight-byte slots; ordered traversal retains at most four 2 KiB pages.
The existing depth-limited source BTH traversal remains bounded by its maximum
allocation size and 32 levels. Alias checks complete before payload acceptance.

`writer/table.rs::build` writes matrix blocks incrementally to disk, builds
row-index BTH levels in bounded groups and feeds an iterator to output stream
emission. `context::HeapBuilder` retains its current page and allocation offsets;
finished pages and fill-level patches live on disk. No table matrix, table
row-index record vector or complete heap-page vector is retained. Q16 transforms
one row and still uses exact Item property tags. Policy observations retain only
the three policy property IDs rather than every non-policy cache cell.

Inspection preserves its previous permissive schema contract, separately from
strict conversion validation. Top-level and nested inspection tables discard
each validated row instead of calling collecting `ltp::read_tc`.

Staging files use exclusive creation, exclusive sharing and delete-on-close on
Windows, with RAII cleanup on other targets. Cancellation is checked during
source chunk reads, table/index traversal and row visits. Partial plans are
discarded by existing conversion cancellation/rollback handling. Staging I/O
failures propagate as fatal I/O failures rather than recovered source skips.

## Regression scope

The file-backed generated fixture has 4,096 rows, 83 columns, a matrix larger
than 2 MiB, multiple BTH/BBT levels and more than 128 heap pages. It generates
rows independently, reverses identity order, varies row versions, leaves cells
absent and checks exact fixed/binary values after output readback. It also
checks the separate format-reader path, outside the allocation assertion.

A test-only thread-local allocator asserts less than 2 MiB additional live
allocation during each table emission, traversal and inspection phase. The
former complete matrix alone exceeds that allowance; the former decoded row
collection adds substantially more. Fixture construction and independent
readback are outside those scopes. These are scoped allocation regressions,
not shared reservations, conversion memory gates or private-commit measures.
Separate tests cover Q16 exact-type removal/version preservation, disk-index
ordering/cancellation, staging cleanup and source-stream relocation. Existing
conversion tests cover Q12/Q15 conflicts, Q16 aliases, skips, cancellation,
Split publication and rollback.

## Exact remaining owners

| Owner | Remaining work |
| --- | --- |
| Per-Item values and descriptors | `reader::item_properties`, `ltp::read_node_pc/read_pc`, `context::read_properties`, PC `HeapBuilder::bth`, `Rows::visit` and `Table::rebuild_contents` still materialize individual PCs, selected values, a row's variable values and Q16 clones. A huge single cell/PCL is not bounded by its fixed-width row. Source subnode maps and per-Item descriptor collections still need bounded lookup/spill. |
| Policy/PCL and planned Items | `cache::prepare` retains `observations`, occurrence vectors, `required`, all planned selected Item properties, complete ChangeKeys/PCLs, parser state/collections and diagnostics. It needs disk observations/decisions and chunked comparisons/parsing across every occurrence. Removing non-policy observations does not close this owner. |
| Writer indexes and source metadata | `Output::blocks`, `Output::nodes`, NBT/BBT `Output::tree` entry/parent vectors, `subentries`/subtree vectors, `source.subnodes`, recovery/selection node vectors, membership sets and created-folder maps remain count-dependent. The table radix index does not replace these. |
| Reports | `Output::differences`, recovery evidence/skips and public `ConversionReport` vectors/strings remain count-dependent, including per-row Q16 differences, `conversion::folder_mapping`, `serde_json::to_vec_pretty` and `report_text` output buffers. A streamed/paged report API and caller/schema decision remain necessary. |
| Finalizer | The pinned unmodified `outlook-pst` finalizer still retains allocation-map pages. It needs the upstream bounded finalizer route described in `issue-20-memory-budget.md`; no dependency or acceptance-contract change is made here. |
| Reservations and disk space | No shared permit/reservation budget exists. Vec capacity, caches, transient copies, table/heap/directory/radix/row/BTH spools, simultaneous rewrite files, planner spools and output candidates need accounting. Table spools currently use the process temporary directory; `conversion::preflight_space` runs after inspection/recovery and does not reserve that volume or account for these spools. Disk exhaustion is an I/O failure, not an enforced reservation. |
| Measurements | The separate-process generated conversion/private-commit harness and the large corpus in `issue-20-memory-budget.md` remain unimplemented. No process ceiling, 16 MiB reservation high-water mark, Faithful acceptance or large-store conversion result is claimed. |

The existing small synthetic Store builder still has its old whole-store and
index-size limits. The table-only file-backed fixture is not the general
large-store fixture builder or the required three-process measurement harness.

## Validation

`python scripts/check` passed on the final implementation snapshot in Windows
Sandbox: dependency audit (376 Windows packages), formatting, workspace Clippy
with warnings denied, workspace tests, the GUI smoke-example tests and all
26 Python harness tests. The five engine unit tests include the generated
table allocation/roundtrip gate, Q16, row-rewrite cancellation/rollback and
staging cleanup. This was not a GUI smoke run or a Faithful check.
