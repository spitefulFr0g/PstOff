# Story 35: strict source BBT implementation slice

Base: PR #49 integration HEAD `922c8ad3ec5746dc82ee2cc59ceb4c23eeb2e577`.
This is an incomplete implementation slice, not completion of issue #32,
story 35 or the contract in `issue-20-memory-budget.md`.

Strict `Pst::from_file` block reads no longer call `blocks()` or initialize
the whole-store BBT `HashMap`. Before the first lookup they validate every
index page, including branches unrelated to that lookup. Subsequent lookups
seek through the original source handle. No paths are reopened and no source
bytes are written. Page CRC/type/identity and entry geometry are checked;
strict key order, exact child minima, disjoint inherited ranges and decreasing
child levels reject duplicates, aliases and cycles without a visited-page set.
At most 33 physical index pages are retained during validation: 135,168 bytes
of page payload for the largest supported page layout, plus vector headers,
stack frames and I/O/runtime overhead. Lookup retains one page at a time.
This arithmetic is not a measured process ceiling or a shared reservation.
There is no page cache in this slice: each lookup rereads its BBT path, so
conversion throughput may regress. No throughput benchmark is claimed.

Generated file-backed BBT tests exercise 1, 128, 4,096 and 65,536 records in
each of the ANSI, Unicode 512-byte and Unicode 4-KiB layouts. The builder
writes pages directly, retaining at most one page per tree level. Tests
compare sampled lookups with the legacy full walk, check missing and reserved
low-bit BIDs, and assert that lookup never initializes the map. Separate tests
reject a duplicate separator outside the requested branch, a child alias and
a cycle. These are index-only fixtures, not convertible OSTs. The comparison
test deliberately collects the old index as its oracle; its process memory
does not demonstrate bounded conversion. Existing optional upstream fixture
tests can report success after skipping when their external fixtures are absent.

## Acceptance still outstanding

No 16 MiB shared reservation or 256 MiB whole-process ceiling has been measured
or enforced by this slice. There is no new separate-process conversion memory
regression. No large-folder, 256 MiB attachment, 32-search-occurrence or 2 MiB
PCL acceptance result is claimed. Faithful policy and its acceptance gates are
unchanged. There is no demonstrated architectural impossibility and no new
authorization request: the remaining implementation is unfinished.

Remaining owners that prevent the documented bound include:

- Source NBT vectors, duplicate/visited sets, recovery copies, selection and
  exclusion membership, folder hierarchy and inspection summaries. Conversion
  still opens the inspection reader while retaining its first reader.
- Whole decoded logical streams, `node_blocks`, PCs, property values, heap
  pages, named-property maps and nested subnode collections in NDB/LTP,
  inspection, recovery, context rewriting and table reading.
- Complete tables, row/index collections, structural heap-ID sets and writer
  heap builders, including concentrated folders and attachment contexts.
- Cache observations, per-Item occurrence collections, property comparisons,
  whole PCL operands/parser collections and diagnostic strings; whole-store
  planned PCs and recovery decisions.
- Writer BBT/NBT record vectors and entry allocations, stream child lists,
  subnode entry lists, generated-folder tables and policy state.
- Report differences, skipped records, folders/files, evidence and serialized
  representations, including the public vector-returning report API and
  completed-file lists retained on cancellation/error.
- The pinned Microsoft finalizer's allocation-map vector and its other page,
  index and runtime caches. It is still the conversion finalizer.
- Permissive and explicitly adopted recovery BBT maps, and explicit `blocks()`
  callers. The new bound applies only to strict, non-adopted BBT lookup.
- Staging disk usage, handles and cleanup bookkeeping still lack the coordinated
  resource reservation required by the contract. No disk-backed planner/index
  or bounded report spool has been introduced.
- Frontend-retained inspection trees, GUI selections/visible-row collections,
  queued jobs/results and caller-supplied exclusion lists also depend on input
  counts. A future dedicated-converter process measurement would not establish
  a ceiling for the GUI or for a caller that retains additional engine results.

These owners require the coordinated interfaces and generated conversion
corpus described in `issue-20-memory-budget.md`; eliminating the source BBT
map alone does not satisfy either acceptance contract.

## Validation observed on this slice

All Rust execution used Windows Sandbox. No existing Sandbox was running
before either wrapper session was launched; no unowned Sandbox was closed.
Inputs were generated; the optional external format-fixture directory was
absent. No SCANPST, Outlook, libpff or private-mail check was run.

- `python scripts/sandbox-cargo.py test --locked -p pstoff-format`: passed on
  the initial implementation (64 tests, before three additional regressions).
- Final `$env:PYTHONUTF8='1'; python scripts/check`: exit 0. Dependency audit,
  formatting, workspace/all-target Clippy with warnings denied, workspace
  tests, the GUI smoke example test and all 26 guest Python harness tests
  passed. This includes 67 format tests and 34 engine conversion tests.
  Conversion tests include unchanged-source checks and independent Microsoft
  reader payload/row readback; they are the existing small synthetic corpus.
  Two fixture-export tests remained explicitly ignored. Optional upstream
  fixture tests skipped internally because their fixture directory was absent.
- An earlier full check passed its tests but failed formatting and a test's
  operator-precedence lint. Both were corrected before the final passing run.

Build profile: debug tests (unoptimized with debuginfo), on the existing locked
dependency graph. There are no reservation high-water or private-commit
measurements, and no release-profile large-store performance evidence.
