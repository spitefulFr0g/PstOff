# Issue #32: Progress, Cancel and the measured regression gate

Continuation from `a787ceb` on `t3/bounded-node-ownership-32`. The ancestry includes
`cea84a0` and `55a34d4`. Generated synthetic data only.

## Status

All issue #32 acceptance checks passed for the generated corpus below. The fixed
8 MiB process-commit gate observed a maximum of 1,662,976 bytes. Progress,
start/middle/end cancellation, real CLI Ctrl+C cleanup, source immutability and
independent payload readback are covered. The broader issue #20 reservation
and arbitrary-value contracts remain unproven.

Final validation used only the approved Windows Sandbox route:

- Formatting and warning-denied Clippy passed for the workspace/all targets.
- The fixed memory gate passed all four cases and its 32 MiB OS-counter control.
- The full workspace replay passed 216 tests; only the unchanged, previously
  passed memory gate was filtered out of that replay.
- All 17 reader tests passed after correcting their two unordered fixtures.
- The GUI smoke example test and all 26 guest Python harness tests passed.
- The dependency audit passed for 376 Windows packages.

The initial `python scripts/check` run passed the memory gate, GUI smoke example
and Python tests but stopped its workspace test step at the two invalid reader
fixtures. After fixture corrections, the approved wrapper replayed workspace
tests, formatting, Clippy and Windows-filtered metadata/audit. This combined
coverage is reported explicitly; the initial command's nonzero result is not
presented as a passing full-script run.

## Ownership and behavior

Source inventories, recovery inventories, selected Items and candidate-part
inventories now store explicitly encoded 24-byte records in private spools.
NID lookup and sorting use disk radix indexes. Exclusion, omission, lost-folder
and part-folder membership also use disk indexes. Source NBT traversal validates
strictly decreasing levels, exact child minima and disjoint ordered ranges,
without a whole-index visited set. Callback errors propagate.

Local namespaces retain local NIDs and original BIDs. Namespace lookup, ordered
relocation records and recovery traversal pending state use disk storage.
Subnode validation retains only parent/leaf blocks and rejects duplicate keys,
invalid child levels, cycles and aliases. Output subentry emission retains at
most one 340-entry leaf plus the format-limited 510 parent records; it preserves
the existing representability limit rather than lowering it.

Conversion validates hierarchy cycles directly instead of constructing and
then discarding the public inspection display tree. Folder names are not
retained across conversion selection. The public inspection API still returns
its explicit display tree. Created scaffolding names remain a fixed set of at
most five names, rather than one name per source folder.

Error and cancellation `completed_files` now use the same fallible, disk-backed
`Records<OutputFile>` as success. JSON arrays and ordering remain compatible.
CLI iteration reports spool read errors; GUI previews cap every outcome's file
rows at 50 while preserving exact counts. `collect_report` remains an explicit
materializing compatibility operation, outside the bounded ownership route.

Cancellation during source inventory, hashing or selection is translated to a
cancelled outcome. Discarded working spools and staging rollback drop before
handback. Returned `Records` snapshots retain their own delete-on-close spools
until their final owner is released.
Successful progress ends at 1.0; Split trial events preserve the done/fraction
high-water marks. Finished Split parts retain the pre-existing cancellation and
error semantics.

## Acceptance checks

- `progress_finishes_monotonically_and_all_cancellation_boundaries_clean_output`:
  start, middle, after the final Item, before publication, and successful unsplit
  progress; source bytes and empty cancelled destinations are asserted.
- `split_trial_progress_never_rewinds_and_ends_at_one`: monotonic fraction, done
  and part number across trial emission and part boundaries.
- `cancel::ctrl_c_cleans_staging`: real Ctrl+C in an isolated child console;
  exit 130, cancelled diagnostic, and no unfinished destination files.
- `large_generated_ost_has_fixed_separate_process_peak`: separate fresh guest
  processes for generation, conversion and independent readback.
- High-count ownership regressions cover 25,000 nodes, 25,000 local subnodes,
  25,000 failed/cancelled output records, and 65,536 multilevel source NBT entries.
  Existing report JSON/text equivalence, policy, Split and rollback tests remain.

## Process measurement protocol

Run only through `python scripts/sandbox-cargo.py test --locked -p pstoff-engine
--test memory -- --nocapture`, or as part of `python scripts/check`.

The deterministic v36 builder writes payload blocks immediately and creates
multilevel NBT/BBT pages from disk records. Its node metadata and small TC
fixtures belong to the generator process, never the converter. Cases contain
16, 256, 1,024 and 4,096 Items, each with a distinct deterministic 256 KiB binary
property: 4 MiB, 64 MiB, 256 MiB and 1 GiB logical payload. Groups contain at most
64 Items. No real mail or downloaded fixture is used.

The converter child starts from a new Windows process. Its OS
`GetProcessMemoryInfo.PeakPagefileUsage` includes its lifetime high-water mark,
source inventory/hash, recovery, planning, writing, finalization and saved
reports. Nothing is subtracted. Fixture generation and readback run in other
fresh processes. A separate touched 32 MiB allocation control verifies that the
counter includes private allocations and retains their peak after release.

The readback child checks every source payload against the generation formula,
every converted payload with the independent Outlook PST reader, every generated
folder's source/output row identities/counts, the report hash, and the source
SHA-256 before/after conversion.
Only verified samples are retained under the guest Cargo target's
`issue32-memory/<run>/evidence.json`, together with compiler details, Cargo.lock
SHA-256, profile, corpus parameters, source hashes and the measurement metric.

## Fixed gate and initial observation

The gate is **8 MiB of peak private commit**, including the converter test
process's own runtime overhead. It was chosen after the initial debug sweep:

| Items | Logical payload bytes | Observed peak private commit bytes |
| ---: | ---: | ---: |
| 16 | 4,194,304 | 1,191,936 |
| 256 | 67,108,864 | 1,290,240 |
| 1,024 | 268,435,456 | 1,454,080 |
| 4,096 | 1,073,741,824 | 1,601,536 |

8,388,608 bytes provides more than five times the observed maximum for runtime,
allocator and toolchain variation while remaining far below even the 64 MiB
corpus's aggregate payload. Retaining a whole corpus's payload would fail this
gate. The 25,000-record allocation regressions separately guard node/report
ownership that would be too small to detect with this four-case process gate.
The threshold is fixed before the final acceptance run, not derived from that
run's peak or increased automatically on failure.

The initial sweep completed in 941.60 seconds. Later validation uses the same
source structure with distinct per-Item payloads, an independently implemented
table CRC in the builder, indexed cache-row lookup, the OS counter control, and
additional folder/readback/cancellation assertions. Its retained evidence is
the acceptance record; the initial observations above justify threshold choice.

## Gated acceptance measurement

[Retained measurement evidence](issue-32-memory-evidence.json) records the final
four samples, positive control, compiler and dependency-lock identity. The gate
completed in 857.24 seconds under Rust 1.99.1, Windows MSVC, debug profile:

| Items | Logical payload bytes | Gated peak private commit bytes |
| ---: | ---: | ---: |
| 16 | 4,194,304 | 1,159,168 |
| 256 | 67,108,864 | 1,339,392 |
| 1,024 | 268,435,456 | 1,474,560 |
| 4,096 | 1,073,741,824 | 1,662,976 |

The 33,554,432-byte touched allocation control reported a 34,422,784-byte OS
high-water mark, retained after allocation release. The gate includes process
startup overhead and subtracts nothing.

The first full check found two pre-existing reader fixtures whose NBT/SLENTRY
keys were unordered. Their entries were corrected without changing their
intended property/attachment damage cases or relaxing strict source validation.
All 17 reader tests then passed. Only this separate reader test file changed
after the memory gate; the converter and memory harness remained unchanged.
The complete workspace was replayed with only
`--skip large_generated_ost_has_fixed_separate_process_peak`, using the already
observed passing gate above rather than repeating a 14-minute corpus generation.

## Limits

This regression will establish an absolute process-commit ceiling for its stated
corpus and build configuration, not for arbitrary supported source values.
Current PC/row property maps, large individual strings/values and public
inspection display trees still need broader memory-contract work. Growing spool
space also lacks shared per-volume admission accounting.

There is no shared reservation budget in this change. Neither the proposed
16 MiB reservation target nor the proposed general 256 MiB process target in
`issue-20-memory-budget.md` is claimed closed. The broader single-huge-value,
PCL/conflict, concentration and large-store corpus remains a separate contract;
this issue's observed process gate must not be presented as that proof.
