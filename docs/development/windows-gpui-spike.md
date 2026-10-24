# Windows GPUI feasibility gate

Status: **unresolved; do not treat GPUI as validated**.

Issue #6 and its comment were inspected on 2026-10-23. The recovered research
at `t3/recover-research-notes:docs/research/gpui-windows.md` examines Zed
cb593458 and GPUI Kit 8b3631f. It explicitly reports no build or runtime test.
Its Linux cross-build limitation is a source inference, not a measured failure.
No alternate framework has been selected.

## Useful source work

`pstoff-engine::platform` classifies Win32 sharing/lock violations (32/33)
separately from disk-full errors (39/112), retaining unknown error codes.
Its available-space trait passes only paths and byte counts across the engine
boundary. Checked addition prevents payload plus overhead from wrapping.

`pstoff-gui::windows` opens sources read-only with FILE_SHARE_READ and queries
GetDiskFreeSpaceExW's caller-available bytes, respecting quota limits. Keep the
source handle alive through reading; a probe followed by reopen has a race.
Space preflight cannot reserve capacity; actual writes must handle disk-full.
These adapters are available for reader/conversion integration, not yet wired
into the foundation's unimplemented inspect/convert functions.

Linux tests exercise classification and space arithmetic. Windows adapter
compilation and runtime behavior remain unverified. No GPUI dependency was
added to the Linux workspace; its dependency audit still rejects GPUI.

## Required authorized Windows procedure

Repository safety requires explicit permission before any Windows build,
because Windows has no build sandbox. Approval was requested; no Windows
build was run while it remained pending. Windows interop executables are
present on this host; toolchain availability has not been verified.

After approval, use a Windows-owned checkout of the exact committed source,
Rust MSVC, Visual Studio C++ Build Tools, Windows SDK and a DX11 driver.
Do not build against private mail. Keep the minimal GPUI spike in a separate
Windows-only Cargo workspace so Linux resolution stays independent. Select
and audit exact GPUI/platform/component revisions together, commit its lockfile,
and fetch before building. Do not copy the historical spike implementation.
A release build must use Windows-hosted cargo, not cargo-xwin.

The current GUI executable is a deliberate failure stub. Building it is not
GPUI acceptance. A real spike must demonstrate all of the following before
replacing the conditional go with a practical decision:

- Debug and release build, window startup and close on Windows.
- Native multiple-file Browse and directory selection; cancellation; Explorer
  file drop, including spaces/non-ASCII paths and invalid extensions.
- Synthetic background work with progress marshalled to the UI; resize,
  keyboard navigation and cancellation remain responsive throughout.
- Keyboard focus order, accessible names/roles, Narrator or NVDA announcements
  and contrast at supported scaling settings, checked by a human.
- Generated file held with incompatible sharing yields InUse; access denied
  remains a distinct I/O failure. Query a destination under user quotas.
- Copy unsigned release exe and required runtime assets to a clean supported
  Windows machine; launch offline without development tools. Record Windows,
  GPU/driver, Rust version, source SHA, artifact hashes and runtime imports.

## Portable packaging and license gate

Do not upload or release from this spike. An exe alone is not proven portable.
Inspect runtime imports and shader/font/resource requirements from the selected
GPUI revision; record any required VC runtime or bundled assets. Preserve MIT
project/pstfree notices and the independent reader's notices. The historical
research identifies GPUI/GPUI Kit as Apache-2.0 candidates; this is not a
transitive dependency audit. Generate metadata for the Windows lockfile,
review every license expression and required notices, reject GPL and satisfy
any LGPL conditions before acceptance. No new dependency or license claim is
approved by this document.

Required acceptance remains blocked on Windows approval and actual GPUI
implementation/build/start/deployment/accessibility evidence. Linux checks do
not discharge it, and there are no runtime screenshots to review.
