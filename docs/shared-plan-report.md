# Shared layout planning and report contract (#35, #29, #31)

Based on PR #49's `origin/spec/issue-20` head `b78c1ee`. Topic branch:
`fix/pr49-shared-plan-report`.

## Planning

`pstoff_engine::plan(source, options)` reads the source through the same
classified read-only boundary, recovery, cascading exclusions and system-shell
selection as `convert`. It creates no files or directories, including temporary
files. It does not need an existing destination or a writable destination.
`pstoff convert ... --dry-run` returns this plan as JSON on stdout and uses the
usual error exit codes. It does not publish reports.

The writer's no-write mode executes the actual property/table rebuilds, nested
subnode traversal, heap layout, block alignment, allocation-map reservations and
NBT/BBT index layout, discarding emitted bytes. It models the pinned Microsoft
finalizer's in-place map/density-list writes and the inclusive extra PMAP write
at an eight-AMAP boundary. Conversion checks both the predicted finalizer size
and the shared part estimate against the finalized file size before publication.
The stated tolerance is **zero bytes per file**, for unchanged source/options
and this pinned writer/finalizer. A mismatch is fatal and removes the current
unpublished part; earlier completed split parts remain available.

Both `plan` and `convert` use `planner::next_part`: greedy global-NID order,
whole Items with all nested content, selected populated folders/ancestors and
required system shells. The split limit is inclusive of finalized overhead.
Empty selections still plan one system-shell PST. An Item too large for an
otherwise empty part returns `OversizedItem`; a shell-only part larger than the
limit returns `InvalidOptions`. The planner never divides declared Item bytes by
the limit to infer a part count.

`OutputPlan` carries `files` (path, estimated_bytes, part, items),
`size_tolerance_bytes` and `over_outlook_limit`. The warning applies to any part
strictly larger than 50 * 1024^3 bytes. The warning boundary is exercised using a
simulated estimate; no huge file is generated. Existing destination files do
not prevent measurement. Conversion still checks collisions and free space and
can fail because the source or destination changed after preview.

The GUI requests engine plans on workers. Its cache is keyed by all conversion
options, rejects stale results, and permits at most one in-flight worker per
source. Missing/currently pending plans disable conversion. The GUI presentation
only adds collision/duplicate-name checks; it performs no packing arithmetic.

Layout measurement still decodes content and repeats growing candidate layouts
for Split (quadratic work in Items per part). It retains existing node/cache
metadata and selected Item property values but does not accumulate complete candidate PSTs
in memory. This work does not establish bounded memory for large stores. Planning is
not a fast metadata-only operation. Conversion plans each part immediately before
writing it, preserving the existing completed-part behavior on later errors.

## Report schema 3

Every earlier report field remains, including source identity, files, folder
counts/declared bytes, skipped reasons, property differences, and all three
independent completeness evidence statuses and references.

Additions:

- `Inspection.mailbox_completeness_statement` returns the same explanation as
  the conversion report, including unsupported/in-use inspection outcomes. CLI
  inspection prints it, and the GUI's pre-conversion explanation reads it.
- `ConversionReport.excluded_folders`: the explicit selected folder IDs and
  paths, deduplicated in selection order. Descendants are excluded by policy;
  they are not confused with additional user selections.
- `ConversionReport.folder_paths`: ID/path lookup for source and generated
  folders, including skipped folders and folders omitted from outputs.
- Each Item reference includes `folder_path`; each folder-part row and skipped
  record also carries its folder path. The existing `LocalRef::Folder` ID
  representation is retained, with its path alongside it on the skipped record
  and in the lookup. Paths are root-first, separated by ` / `; unreadable names
  use `(unnamed)` and absent identities use a node-ID fallback.

Text reports include selected exclusions and paths in folder mappings, evidence,
unchecked evidence, skipped records and Item property differences. GUI consumers
prefer saved paths and retain the inspected-tree fallback for older reports.
The GUI report shows the selected exclusions separately from skipped records.
No subjects, addresses, bodies or external targets are added to references.

All new persisted fields have serde defaults. Schema-2 reports retain their
schema version and prior data when deserialized; absent paths/selections stay
empty. The compatibility regression strips the new fields and checks an exact
JSON value round trip of all previous fields.

## Validation status

Synthetic fixtures only. Static `git diff --check` passes. The Sandbox formatter
output was applied. The first `python scripts/check` run passed the dependency
audit (376 Windows packages), formatting and Clippy with warnings denied. CLI
regressions and the public no-write/whole-Item plan agreement regression passed.
Two added engine fixture assertions failed: the large-payload test exceeded the
builder's one-page BBT capacity, and the unreadable-folder test expected a name
that the damaged fixture cannot expose. The builder now supports two BBT levels
without changing small fixtures, and the assertion checks the `(unnamed)`
fallback. A full rerun of those corrections is pending because another session
owns Windows Sandbox (PID 28996 at the latest availability check).

No existing Sandbox was stopped or changed. Rust builds, formatting, lint and
tests ran only through the wrappers. An earlier waiting formatting wrapper was
stopped before it acquired the lock; formatting was then run after a process
check found Sandbox free. The other session acquired Sandbox again after the
first full run ended. No Rust build, formatter, lint or test ran on the host.

Required next check when Sandbox is free: `python scripts/check`. Added
regressions cover public plan/output agreement, split boundaries, cascading
exclusions, empty selections, oversized Items, misleading declared sizes,
multi-map nested payloads, no-write CLI dry runs, report paths and prior schema
data, and GUI stale-plan behavior.

No SCANPST, libpff or Outlook Faithful gates were attempted. This implementation
record does not claim those acceptance gates or validation completion.
