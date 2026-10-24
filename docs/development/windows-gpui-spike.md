# Windows GPUI feasibility gate

Status: **conditional go; required acceptance remains unresolved**.

Issue #6 research at `t3/recover-research-notes:docs/research/gpui-windows.md`
was a source investigation, not build evidence. Attempt 3 added a separate
`spikes/windows-gpui` workspace pinned to `gpui-pre` and `gpui-pre-platform`
0.3.9 (published snapshot of Zed 93d745668805029476e6d7fbb19652116fed51de).
The engine workspace still resolves independently of GPUI. No framework was
substituted. The root `pstoff-gui` binary remains a failure stub; this separate
probe is not the full wizard.

## Measured results

The build and runtime results below describe earlier source revisions. The
#44 scope update removed custom accessibility and keyboard/focus support from
the probe after those checks. No tests, builds or smoke checks were run for
that update; prior build/runtime evidence is stale for the revised source.
Historical attempt-4 evidence remains unchanged.

Windows-hosted debug and release builds passed in an offline Windows Sandbox
using Rust stable MSVC 1.99.1, MSVC 14.44.35207 and Windows SDK 10.0.26100.0.
The debug probe opened a real window, displayed synthetic background progress
at 41%, and closed cleanly. These checks used public/generated inputs only.

A second guest, with only the executable and our smoke script mapped in,
exposed a deployment failure in the initial release executable: it imported
`VCRUNTIME140.dll`, absent from that guest, and exited with `0xC0000135` before
opening a window. Its size was 10,361,856 bytes; SHA-256 was
`1651409e38c708327def6260d65cecbc801d6faacf339050ba38c086145c3365`.
Static CRT linking removed that direct VC-runtime dependency. A subsequent
unsigned release opened in a fresh offline guest without checkout/cache/toolchain
mappings. Native multiple-file Browse (spaces, Unicode and uppercase extension),
invalid-extension rejection, Explorer multi-file drop, destination selection and
cancellation, synthetic progress/cancel/restart, and unchanged source hashes were
observed. Earlier source exposed named, focusable buttons and a named status
to UI Automation and included keyboard/tab support. Those observations are
historical; the #44 update removes the probe's custom accessibility metadata,
focus/tab support and keyboard handlers. Exact source revisions, final retests
and artifact hashes belong to the task report.
Do not distribute the earlier dynamically linked executable as self-contained.

Attempt evidence includes source SHAs, guest commands/configurations, exit
codes, captures, Windows-filtered Cargo metadata, dependency declarations,
notice provenance and direct PE imports. The task report distinguishes each
source revision; a successful build is not a claim that all GUI gates passed.

## Probe and adapters

The probe never opens selected files or converts mail. It exercises native
multi-file and directory prompts, extension validation and Explorer drop
callbacks. A synthetic worker runs off the UI thread; a foreground observer
marshals progress and completion. Cancel and close request cooperative stop.
The probe is mouse-driven: click the visibly labeled buttons to browse, choose
a destination, start or cancel work, or drop synthetic paths into the window.
Progress and status remain visible.

Per #44, PstOff v1 has no accessibility features. The product does not add
custom screen-reader narration, explicit accessibility labels/roles, or
accessibility-specific keyboard/focus behavior, and has no accessibility
acceptance requirements (including Narrator/NVDA, contrast/scaling or human
accessibility review). This scope decision makes no claim that Windows itself
or all GPUI defaults have been made unreadable to assistive technology.

`pstoff-engine::platform` classifies Win32 sharing/lock violations (32/33)
separately from disk-full errors (39/112), retaining unknown error codes.
Its available-space trait passes only paths and byte counts across the engine
boundary. Checked addition prevents payload plus overhead from wrapping.

`pstoff-gui::windows` opens sources read-only with FILE_SHARE_READ and queries
GetDiskFreeSpaceExW's caller-available bytes. Keep the source handle alive
through reading; a probe followed by reopen has a race. Space preflight cannot
reserve capacity; writes must still handle disk-full. These adapters are not
yet wired into conversion policy.

Three Windows-only synthetic runtime tests passed: incompatible sharing maps
to InUse and source bytes stay unchanged; missing files remain distinct; and
Unicode directory/free-space queries work while embedded NUL is rejected.
Real quota-limited volumes and access-denied behavior remain unverified.

## Reproduce safely on Windows

Windows is the runtime/distribution target. Builds and smoke checks run only
inside fresh Windows Sandbox guests, never on the host. Never execute an
installer on the host. Record a clean source SHA and export only tracked
source with `git archive`; do not map private mail or a whole user directory.

Before creating the build guest, use Windows Cargo outside the Sandbox only
to fetch dependencies, with a task-specific Cargo cache under host temp and
HTTP User-Agent `PstOff-research (https://github.com/spitefulFr0g/PstOff)`:

```powershell
# Run from the exported engine workspace, then from spikes/windows-gpui.
# CARGO_HOME must name the task's dedicated host-temp cache.
cargo fetch --locked --target x86_64-pc-windows-msvc
```

Fetching must not execute crate build scripts. Configure the fresh guest with
networking and clipboard redirection disabled. Map tracked source, fetched
Cargo cache, prepared Rust installation, MSVC VC directory and Windows SDK
read-only. Map only a dedicated output folder writable. Enable vGPU for GUI
checks. Run a logon command from a read-only input folder.

Set PATH, INCLUDE and LIB to the mapped Rust/MSVC/SDK locations, including the
MSVC x64 compiler/linker and SDK ucrt/shared/um/winrt includes and x64 libraries.
Set `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER` to the mapped `link.exe` and
`GPUI_FXC_PATH` to the mapped SDK's x64 `fxc.exe`. Set `CARGO_HOME` to the
read-only fetched cache and `CARGO_NET_OFFLINE=true`.

Use a dedicated **guest-local** `CARGO_TARGET_DIR`, such as
`C:\PstOffBuild\target`, then mirror all artifacts to the writable mapped
output folder before shutdown. Running Cargo build-script executables from
a Sandbox mapped output folder failed with Windows error 1392; using local
guest staging avoided that failure without making source or caches writable.

```powershell
# From the read-only engine workspace:
cargo test --offline --locked --target x86_64-pc-windows-msvc -p pstoff-engine -p pstoff-gui
cargo fmt -p pstoff-gui -- --check
cargo clippy --offline --locked --target x86_64-pc-windows-msvc -p pstoff-gui --all-targets -- -D warnings

# Change directory so Cargo discovers the probe's static-CRT configuration.
cd spikes/windows-gpui
cargo fmt -- --check
cargo build --offline --locked --target x86_64-pc-windows-msvc
cargo build --release --offline --locked --target x86_64-pc-windows-msvc
```

Capture each exit code and complete log. Copy build output to the mapped
output folder, hash the executable and inspect imports. A separate fresh
runtime guest must map only the executable/test scripts read-only and a
writable capture folder; no checkout, cache or toolchain mappings. Copy the
executable into a guest-local directory before launching. Use only generated
synthetic files for native selection/drop checks. Retain screenshots and
actual observations, not inferred passes.

## License and remaining gates

Windows-filtered metadata for the pinned probe resolves 334 third-party
packages. Its declared license expressions contain no GPL or LGPL. Exact
expressions include MIT/Apache alternatives, BSD, Zlib and Unicode licenses;
this is not a substring-based license approval. Preserve all applicable
copyright, license and NOTICE material when packaging. The project and
preserved pstfree fork's existing notices also remain binding where used.

Notice collection recovered files from crate archives and, where omitted,
upstream roots at the packaged VCS SHA for 332 packages, including British-spelled
`LICENCE` files. Exact-revision notice collection remains unresolved for
`seahash 4.1.0` and `simd_helpers 0.1.0`, both declaring MIT. Do not call redistribution cleared
until missing attribution and applicable notice obligations are resolved.

Required remaining checks include real Explorer multi-file drop; native
selection/cancellation including Unicode and invalid paths; mouse operation
and resizing during work; quota-limited space/access-denied cases; clean offline
static-release startup; runtime imports/assets; and the supported Windows/GPU/driver matrix.
Record any completed subset explicitly. A single Sandbox does not validate
the whole deployment matrix. No upload or release is
permitted from this spike, and no push/PR is permitted before acceptance.
