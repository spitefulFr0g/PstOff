# How ready is GPUI for a Windows app developed from WSL?

Research for the ticket "How ready is GPUI for a Windows app developed from WSL?", 2026-10-21.

Sources were read at these revisions:

- Zed at [`cb593458`][zed-rev] (main, 2026-10-21)
- GPUI Kit at [`8b3631f`][kit-rev] (main, 2026-10-21)
- crates.io metadata as of the same day

Nothing was built or run. Every claim that only a build could settle is marked as an inference and listed under [Spike](#spike-what-a-small-build-must-prove).

Recovered from the previously written note in the original research worktree; its original research and attribution are preserved. The decision-critical source checks below were repeated against the cited Zed, Cargo, and GPUI Kit sources. Version, issue-count, and ecosystem snapshots are historical as of 2026-10-21.

## Answer

**Conditional go for a prototype, with caveats.** Zed ships a real Windows app on GPUI, but that alone does not validate PstOff-specific dialogs, accessibility, packaging, or WSL build workflow:

- Zed has been stable on Windows since 2025-10-15.
- At least a dozen non-Zed apps ship Windows builds on it.
- The component library GPUI Kit (Apache-2.0) has every widget PstOff's flow needs: a tree with disabled items, tri-state checkboxes, progress, dialogs and themes.
- GPUI's Windows backend already provides native file dialogs, file drops from Explorer, a background executor and an AccessKit screen-reader bridge.

The caveats:

1. **A Linux-hosted `cargo-xwin` release build is expected to fail, by code inference.** The GPUI Windows build script gates shader generation on its host `target_os = "windows"` cfg, while the release renderer includes the generated file unconditionally for release builds. Cargo documents that build-script cfg reflects the host. No cross-build was run, so the failure remains an inference. Driving Windows-hosted `cargo.exe` from WSL is a plausible route, not verified here. The workable setup has two halves:
   - Install the Rust MSVC toolchain, the Visual Studio Build Tools and a Windows SDK on the Windows side, and drive `cargo.exe` from WSL.
   - Keep the engine and CLI buildable and testable in WSL.
2. **GPUI has no stable release channel.**
   - crates.io `gpui` is 0.2.2 from 2025-10-22, a year old.
   - Current GPUI comes from a git dependency on Zed or from weekly third-party snapshots (`gpui-pre`).
   - GPUI Kit pins an exact snapshot, and every bump can break code.
   - Budget for pinning and occasional migration work.
3. **Windows rough edges are real but not blockers.**
   - There are 110 open Windows issues in Zed, about 10 of them in GPUI itself.
   - A DirectX 11 GPU with a vendor driver is required.
   - VMs and RDP work but may be slow.
   - Screen-reader support is new (May 2026), and Zed's own screen-reader issue is still open.
4. **Small API gaps.**
   - GPUI's file dialog has no file-type filter, so it can't offer "*.ost only".
   - Fix: use `rfd` (MIT) with the window handle, or check the extension after the pick.

Nothing found rules GPUI out for PstOff. GPUI *would* be ruled out if any of these turned out to be needed:

- release exes built on Linux-only CI
- running on machines without a DirectX 11 driver
- proven NVDA support before the first release, if the spike fails it

If those conditions apply, a fallback to evaluate is **egui/eframe**. Its `MIT OR Apache-2.0` license fits without conditions. Slint's license choice (`GPL-3.0-only OR` a royalty-free or commercial license) would need reading first. Neither was evaluated here.

## Windows support

### When it landed and what renders

- Zed for Windows became generally available on 2025-10-15 ([Zed blog, "Windows When? Windows Now"][blog-windows]).
- The renderer moved from Vulkan to DirectX 11 in [zed#34374][pr-34374], merged 2025-07-30.
- The GPUI README says: "Windows — no features are required. Windowing uses Win32 and text uses DirectWrite" ([crates/gpui/README.md:43][readme-43]).
- The renderer asks for Direct3D feature levels 11_1, 11_0 or 10_1 ([crates/gpui_windows/src/directx_devices.rs:8-9, 54-60][dx-devices]) through `IDXGIFactory6` ([directx_devices.rs:18, 41, 92][dx-devices]).
- It detects software adapters (`DXGI_ADAPTER_FLAG_SOFTWARE`, [directx_renderer.rs:826][dx-sw]).
- Inference, unverified: the code may run on the Microsoft Basic Render Driver as a fallback.
- Zed's user docs are firmer about the requirements:
  - "Graphics: A GPU that supports DirectX 11"
  - "Driver: Current NVIDIA/AMD/Intel/Qualcomm driver (not the Microsoft Basic Display Adapter)" ([docs/src/installation.md:121-122][install-121])
  - In a VM, "it will use the emulated adapter … performance may be degraded" ([docs/src/windows.md:66][win-66])
- The developer guide's "Graphics issues" section still says Vulkan ([docs/src/development/windows.md:250][devwin-250]). That text is stale since #34374.

### Minimum Windows version

- Supported versions: "Windows 11, version 22H2 and later" and "Windows 10, version 1903 and later", 64-bit only, on x64 or Arm64 ([installation.md:99-119][install-112]).
- Zed's installer allows install from 10.0.16299, which is Windows 10 1709 (`MinVersion=10.0.16299` in [crates/zed/resources/windows/zed.iss][zed-iss]). That is looser than the docs.
- A PR to support older systems, [zed#57928][pr-57928] (win-legacy-compat), was closed unmerged on 2026-06-04.
  - Its description lists APIs the backend relies on: `IDXGIFactory6` (Windows 10 1803+) and `IDWriteFactory5` (1703+).
  - So Windows Server 2016 and older Windows 10 builds are not supported.
- **For PstOff:** target Windows 10 1903+ and Windows 11 22H2+, matching what Zed supports.

### Known gaps and bugs

- On 2026-10-21, `gh issue list --label platform:windows --state open` on zed-industries/zed returned **110** issues.
- Issues with both `platform:windows` and `area:gpui`:

  | Issue | Problem |
  |---|---|
  | [#65799][i65799] | notification RefCell panic |
  | [#65402][i65402] | Transparent background tint |
  | [#64695][i64695] | mouse-wheel reports |
  | [#62404][i62404] | phantom Alt key |
  | [#61508][i61508] | popup border |
  | [#61469][i61469] | WM_PAINT starvation |
  | [#60140][i60140] | eGPU taskbar |
  | [#58746][i58746] | hover popover |
  | [#52448][i52448] | modal blocks |
  | [#37727][i37727] | GPU load while typing |

- Also [#63812][i63812]: the top border is missing on Windows 10 (`area:gpui/graphics`).
- Remote and virtual displays:
  - [#26692][i26692] (RDP) was closed after the DirectX 11 port.
  - [#37231][i37231] (VMware text rendering) is still open.
- None of these touch a converter-style flow badly. #61469 and #52448 are the ones to watch, since PstOff updates progress constantly and shows modal dialogs.

### How GPUI is consumed

| Channel | Version / date | Notes |
|---|---|---|
| crates.io `gpui` | 0.2.2, 2025-10-22 | A year behind Zed. Zed main still says `version = "0.2.2"` ([crates/gpui/Cargo.toml:3][gpui-cargo]). |
| crates.io `gpui_platform`, `gpui_windows` | not published | The workspace sets `publish = false` ([Cargo.toml:276][ws-publish]), and both crates inherit it ([gpui_windows/Cargo.toml:5][gw-cargo]). Yet the README tells you to add `gpui_platform = "*"` ([README.md:8-12][readme-8]). |
| git dependency on zed-industries/zed | any commit | What Zed itself uses, and what e.g. Baudrun uses ([Baudrun Cargo.toml][baudrun-cargo]). |
| `gpui-pre` / `gpui-pre-windows` | 0.3.9, 2026-10-12 | Weekly third-party snapshots of Zed's crates (0.3.0 to 0.3.9 between 2026-09-03 and 2026-10-12). 0.3.9 is Zed commit `93d7456` (2026-10-10). Apache-2.0. Its `build.rs` is identical to Zed's `gpui_windows/build.rs`. |
| `gpui-ce` | 0.3.0, 2026-10-20 | Community fork ([gpui-ce/gpui-ce][gpui-ce]). Apache-2.0. |
| `gpui-unofficial` | 1.25.0-pre, 2026-10-20 | Another republish ([iamnbutler/gpui-unofficial][gpui-unofficial]). Apache-2.0. |

API stability:

- The README says GPUI "is still pre-1.0. There will often be breaking changes between versions. You'll also need to use the latest version of stable Rust" ([README.md:8][readme-8]).
- GPUI Kit pins an exact snapshot: `gpui = { package = "gpui-pre", version = "=0.3.9" }`. The reason given is that "a caret requirement let the weekly release move applications onto a newer snapshot that gpui-component did not compile with (#3156)" ([gpui-kit Cargo.toml:62-70][kit-cargo]).
- **For PstOff:** depend on `gpui-component` and the `gpui-pre` version it pins, with `=` requirements and a committed `Cargo.lock`. Upgrade both together, on purpose.

## Component library: GPUI Kit (gpui-component)

**Identity and maturity**
- `gpui-component` lives in [longbridge/gpui-kit][kit-repo] ("GPUI Kit"). Version 0.8.0 was published 2026-10-19 under Apache-2.0 ([crates/component/Cargo.toml:1-10][kit-comp-cargo]); a `gpui-kit` 0.8.0 umbrella crate came out the same day.
- Releases are roughly weekly (0.6.0 on 2026-09-03 through 0.8.0).
- Up to 0.5.1 it depended on crates.io `gpui ^0.2.2`. Since then it depends on `gpui-pre =0.3.9`.
- The README claims "75+ documented components", "AccessKit accessibility", "UI integration testing" and "Native Feel: Modern controls inspired by macOS and Windows" ([README.md:16-17, 36-41][kit-readme]).
- It is used by Longbridge Pro, a shipped commercial desktop app ([README.md:37, 110][kit-readme]).
- The README's install line still says `gpui-kit = "0.7"` ([README.md:122][kit-readme]), one release behind.

**Widgets for PstOff's flow**
- **Folder tree with item counts and unticking.**
  - Use `tree(state, render_item)` ([crates/component/src/tree.rs:18-50][kit-tree]). The app renders each row itself, so a checkbox and an item count go inside the row.
  - The tree has no built-in checkboxes.
  - `TreeItem` has `.children`, `.expanded` and `.disabled` ([crates/base/src/tree.rs:116-126][kit-base-tree]). It exposes `Role::Tree` and `Role::TreeItem` to screen readers ([tree.rs:463, 553][kit-base-tree]).
  - The checkbox is tri-state (`Checked`, `Unchecked`, `Indeterminate`, with `.indeterminate(bool)`), and Indeterminate is exposed as `Toggled::Mixed` ([crates/base/src/checkbox.rs:24-38, 109-111][kit-checkbox]). That covers a parent folder with some children unticked.
  - Large trees: GPUI's own `uniform_list` and GPUI Kit's `virtual_list` exist. Their performance on a mailbox with thousands of folders is unmeasured.
- **Progress.**
  - Determinate progress uses `.value(f32)`; indeterminate uses `.loading(bool)`.
  - Both support `.accessibility_label(...)` ([crates/component/src/progress/progress_circle.rs:47-67][kit-progress]). There is a bar variant in the same `progress/` module.
- **Dialogs and notifications.**
  - `Root::open_dialog` ([crates/component/src/root.rs:363][kit-root]), plus `dialog/`, `alert`, `notification`.
  - These suit the In-place mode warning and the backup prompt.
- **Report view.** Use `table`, `list`, `accordion` and `alert` from [crates/component/src/][kit-src].
- **Window chrome.** `title_bar` and `window_border` give a custom-drawn title bar.

## Feature fit for the PstOff GUI flow

### Native file dialogs

- `prompt_for_paths` and `prompt_for_new_path` call the Windows shell dialogs ([crates/gpui_windows/src/platform.rs:784-816][gw-prompt]):
  - the open dialog is `IFileOpenDialog` with `FOS_ALLOWMULTISELECT` and `FOS_PICKFOLDERS` as requested ([platform.rs:1529-1545][gw-open])
  - the save dialog is `IFileSaveDialog`, whose only filter is "All files" ([platform.rs:1576-1611][gw-save])
- `PathPromptOptions` has only `files`, `directories`, `multiple` and `prompt` ([crates/gpui/src/platform.rs:2875-2881][gpui-ppo]). **There is no file-type filter**, so PstOff can't offer an "Outlook data files (*.ost)" filter through GPUI.
- One dialog cannot pick files and folders at once: `can_select_mixed_files_and_dirs` returns false ([platform.rs:813-814][gw-prompt]).
- Picking "Add OSTs" (multiple files) and "Output folder" (directories) works as is.
- For a `*.ost` filter, use [`rfd`][rfd] (0.17.2, MIT) with the parent window handle. GPUI exposes one through `raw-window-handle` (`impl HasWindowHandle for Window`, [crates/gpui/src/window.rs:7616][gpui-rwh]; `impl rwh::HasWindowHandle for WindowsWindow`, [gpui_windows/src/window.rs:595][gw-rwh]).
  - Whether an `rfd` dialog behaves well alongside GPUI's message loop is unverified (spike item).
  - The other option is to accept any file and reject non-`.ost` files with a message.

### Drag-and-drop from Explorer

- `gpui_windows` registers an OLE `IDropTarget` that accepts `CF_HDROP` ([gpui_windows/src/window.rs:1157-1280][gw-drop]).
- It emits `FileDropEvent::Entered { paths }`, then `Pending`, `Submit` and `Exited` ([crates/gpui/src/interactive.rs:758][gpui-filedrop]), which elements receive as external-path drops.
- Windows behaviour, not verified here: a window running elevated does not receive drops from a non-elevated Explorer, because of User Interface Privilege Isolation (UIPI).
  - GPUI's manifest asks for `asInvoker` ([gpui.manifest.xml:5][gpui-manifest]), so PstOff runs unelevated by default.
  - Keep it that way. The plan already settles that a locked OST means asking the user to close Outlook, not elevating.

### Long-running work with progress and Cancel

- `BackgroundExecutor::spawn` "Enqueues the given future to be run to completion on a background thread" ([crates/gpui/src/executor.rs:101-103][gpui-exec]).
- A foreground `spawn` runs on the UI thread ([executor.rs:354][gpui-exec-fg]).
- Task semantics: "If you drop a task it will be cancelled immediately. Calling `Task::detach` allows the task to continue running" ([crates/scheduler/src/executor.rs:399-400][sched-task]).
- Dropping a task stops the *future* only at its next `.await`. A synchronous, CPU- and IO-bound conversion engine won't notice.
- Suggested shape (a design suggestion, not GPUI-mandated):
  1. Run the engine on the background executor, or on a plain `std::thread`.
  2. Pass it an `Arc<AtomicBool>` cancel flag that it checks between items.
  3. Have it send progress events over a channel.
  4. A foreground task drains the channel, updates the view's state and calls `notify`.
- This keeps the engine free of GPUI, which fits the settled "engine avoids Windows-only APIs; CLI runs the same engine".

## Theming and a modern look

**Theme API**
- `Theme::change(ThemeMode, …)` switches light and dark ([crates/component/src/theme/mod.rs:389-394, 807][kit-theme]).
- A default light and dark theme is compiled in ([theme/registry.rs:12, 163-174][kit-registry]).
- `ThemeRegistry::watch_dir` loads JSON themes from a folder ([registry.rs:98][kit-registry]).
- The repo ships 21 sample themes in [`themes/`][kit-themes] (Catppuccin, Solarized, Tokyo Night…).

**Windows look**
- GPUI supports `WindowBackgroundAppearance::MicaBackdrop` and `MicaAltBackdrop` on Windows through DWM ([gpui_windows/src/window.rs:920-937][gw-mica]). That gives the Windows 11 system backdrop.
- Inference, unverified: on Windows 10 it presumably degrades to a plain background.
- GPUI's manifest declares PerMonitorV2 DPI awareness and Common Controls v6 ([gpui.manifest.xml:17-26][gpui-manifest]), so scaling on high-DPI displays is handled.

**Licenses to avoid**
- Don't use Zed's own `ui` and `theme` crates for styling. They are GPL-3.0-or-later.
- GPUI Kit's theme system is the Apache-2.0 alternative.

## Accessibility (screen readers on Windows)

- GPUI depends on AccessKit (`accesskit` 0.24 and `accesskit_windows` 0.34, [Cargo.toml:529-532][ws-accesskit]).
- `gpui_windows` creates an `accesskit_windows::Adapter`, the UI Automation bridge, per window ([gpui_windows/src/window.rs:1084, 1122][gw-a11y]).
- This landed in [zed#56065][pr-56065] (merged 2026-05-27), with follow-ups [#57954][pr-57954], [#60397][pr-60397] (2026-07-13) and [#61926][pr-61926].
- In the GPUI guide, elements are exposed only once they have an `.id()` and a `.role()` ([crates/gpui/src/_accessibility.rs][gpui-a11y-guide]). GPUI Kit components set roles, names and states for you (see the tree and checkbox above), and its README says this is "covered by tests".
- Zed's issue [#41138][i41138], "Windows: Screen reader accessibility missing completely", is **still open**.
  - A maintainer reported on 2026-05-22 that testing with Inspect (the UIA inspector) worked.
  - My reading: the platform plumbing is in, while Zed's own UI coverage is unfinished.
- Nobody has published NVDA or Narrator results for a GPUI Kit tree with checkboxes. This is a spike item.

## Building the Windows exe from WSL

### What GPUI's build needs

| Build | What happens | Source |
|---|---|---|
| Release | The HLSL shaders are compiled by `fxc.exe` at build time into `OUT_DIR/shaders_bytes.rs`. `fxc.exe` is found via `GPUI_FXC_PATH`, then `where.exe fxc.exe`, then the newest Windows 10 SDK in the registry. Failing that, the build panics "Failed to find fxc.exe". | [crates/gpui_windows/build.rs:14-140][gw-build] |
| Debug | Shaders are compiled at runtime with `D3DCompileFromFile` from `env!("CARGO_MANIFEST_DIR")/src/shaders.hlsl`, a path baked in at compile time. | [directx_renderer.rs:1830-1842][dx-runtime] |

- Release builds then include the generated file ([directx_renderer.rs:1868-1869][dx-include]).
- Consequence of the debug design: a debug exe only works on the machine that built it, with the source tree present. A Zed maintainer confirms "For debug build, many materials are embeded at runtime" ([#26692 comment, 2025-08-12][i26692]). The debug executable is not shown to be portable: source-tree shader lookup and runtime behavior need validation before treating a debug build as distributable.
- Finding the SDK automatically came in [zed#38711][pr-38711]. Before that, crates.io `gpui` 0.2.2 users hit "Failed to find fxc.exe" without the 26100 SDK ([#46263][i46263]).
- The `gpui` crate's build script embeds the Windows manifest via `embed-resource` when the default `windows-manifest` feature is on ([crates/gpui/Cargo.toml:21, 47, 199][gpui-cargo]; [crates/gpui/build.rs:19-33][gpui-build]).

### Cross-compiling from WSL (cargo-xwin, msvc target): expected to fail for release

Cargo's docs are explicit that in build scripts the "`cfg!` macro or `#[cfg]` attribute … check the *host* platform, not the *target*" ([Cargo reference, environment-variables.md:401-403][cargo-cfg]). Zed's two build scripts differ on this:

- `crates/gpui/build.rs` reads `CARGO_CFG_TARGET_OS` ([build.rs:19, 31][gpui-build]). It does the right thing when cross-compiling.
  - Its manifest path bug under `cargo xwin check` ([#62522][i62522]) was fixed by [zed#62525][pr-62525] (2026-09-17).
- `crates/gpui_windows/build.rs` wraps `compile_shaders()` in `#[cfg(target_os = "windows")]` and `#[cfg(not(debug_assertions))]` ([build.rs:3-12][gw-build]).
  - On a Linux host that block is compiled out, so `shaders_bytes.rs` is never written.
  - The release `include!` should then fail with a missing file.
  - The same code ships in `gpui-pre-windows` 0.3.9.
- A debug cross-build should compile, but would fail at runtime: the baked-in shader path is a Linux path that doesn't exist on Windows.
- Both conclusions are **inferences from code, not tested by a build**.
- Workarounds would mean forking `gpui_windows`, e.g. switching the build script to `CARGO_CFG_TARGET_OS` and running `fxc.exe` through WSL interop or committing precompiled shader blobs. That is ongoing fork maintenance.
- The `-gnu` target (MinGW) is not a way out: Zed says it "does not support unofficial MSYS2 Zed packages built for Mingw-w64" ([development/windows.md:120-124][devwin-msys]).
- Zed itself uses cargo-xwin only for its remote server, not the GUI ([zed#64787][pr-64787]).
- Every non-Zed app checked (below) builds Windows on Windows CI runners.

### Windows toolchain through interop: the recommended path

What to install on the Windows side, per [docs/src/development/windows.md:18-64][devwin-18]:

- `rustup` (stable, MSVC host)
- Visual Studio or Build Tools with "Desktop development with C++" / MSVC build tools
- a Windows 10/11 SDK of at least 10.0.20348.0 (Zed's component list names `Windows11SDK.26100`)

Zed also asks for the Spectre-mitigated libs. Whether a GPUI-only app needs them is unverified.

A likely workflow is to invoke Windows-hosted `cargo.exe` or PowerShell from WSL, using a checkout on the Windows filesystem (`/mnt/c/...`) or a synced copy. This host arrangement should satisfy the build script cfg, but this was not tested. Whether `cargo.exe` builds reliably from a `\\wsl.localhost\...` path is also unknown and remains a spike item.

**Consequence for the crate split:**
- Only the GUI crate needs the Windows toolchain. The engine and CLI stay buildable and testable with the Linux toolchain in WSL and Linux CI.
- Keep `gpui` dependencies confined to the GUI crate, and keep that crate out of the default workspace members built on Linux.

## Packaging, icon, manifest and size

**Single exe**
- A release build is one `.exe`: shaders are compiled in ([dx-include]) and the manifest is embedded.
- Rust's MSVC targets link the C runtime dynamically by default; `-C target-feature=+crt-static` links it statically ([Rust reference, linkage.md:77-95][rust-crt]).
- Zed sets `+crt-static` for Windows in [.cargo/config.toml:10-16][zed-cargo-config]. PstOff should do the same, to avoid needing the VC++ redistributable.

**Manifest**
- GPUI already embeds one ([crates/gpui/resources/windows/gpui.manifest.xml][gpui-manifest], via `gpui.rc`):
  - `asInvoker`
  - Windows 10 `supportedOS`
  - PerMonitorV2 DPI
  - SegmentHeap
  - Common Controls v6
- PstOff shouldn't add a second one.

**Icon and version info**
- Zed compiles a separate `.rc` with `1 ICON` and a `VERSIONINFO` block, and passes `compile(false)` to skip the manifest because GPUI provides it ([crates/zed/build.rs:211-214][zed-build]; [crates/windows_resources/src/windows_resources.rs:44-84][zed-winres]).
- That crate is GPL-3.0-or-later, so copy the pattern, not the code. Use [`winresource`][winresource] (MIT) or [`embed-resource`][embed-resource] (MIT) in PstOff's GUI `build.rs`.
- Whether PstOff's resource links cleanly next to GPUI's manifest resource is unverified (spike item).

**Installer and signing in practice**
- Zed builds an Inno Setup installer ([zed.iss][zed-iss]: `Compression=lzma`, `MinVersion=10.0.16299`) via [script/bundle-windows.ps1:329-362][zed-bundle].
- It signs with Azure Trusted Signing ([crates/zed/resources/windows/sign.ps1:14-53][zed-sign]).
- Other GPUI apps ship an MSI, a `Setup.exe`, a portable zip, or several of these (table below).

**Binary size**
- No bare-GPUI baseline was measured.
- The compressed Windows x64 downloads of shipping GPUI apps, from each repo's latest GitHub release on 2026-10-21:

  | App | Release | Asset | Size |
  |---|---|---|---|
  | [Baudrun][baudrun] (serial terminal; zed git + gpui-component) | v0.15.2 | `.msi` / portable `.zip` | 7.4 MB / 7.6 MB |
  | [Termy][termy] (terminal; gpui-kit) | v1.0.0-rc.7 | `Setup.exe` | 12.3 MB |
  | [DBFlux][dbflux] (DB client; gpui-pre + gpui-component) | v0.8.12 | `setup.exe` / `.zip` | 34.0 MB / 52.7 MB |
  | [GitComet][gitcomet] (git client; gpui-ce fork) | v0.3.1 | `.msi` / portable `.zip` | 43.3 MB / 44.7 MB |

- Inference: a converter-sized GPUI app likely lands near the low end, under 15 MB compressed. The spike should measure the uncompressed exe.

## Real non-Zed apps shipping on GPUI for Windows

From Zed's curated list [zed-industries/awesome-gpui][awesome-gpui] (the largest app listed has about 941 stars), apps with Windows binaries in their latest releases:

| App | GPUI channel |
|---|---|
| [GitComet][gitcomet] | gpui-ce fork with a `windows-wgpu` feature; MSI and portable |
| [Arbor][arbor] | — |
| [Eleven][eleven] | — |
| [Hummingbird][hummingbird] | gpui-unofficial |
| [DBFlux][dbflux] | `gpui-pre =0.3.9` + `gpui-component =0.7.3` |
| [Codux][codux] | — |
| [Termy][termy] | `gpui-kit =0.7.0` |
| [PicoForge][picoforge] | gpui 0.2.2 + a gpui-component fork |
| [Zaku][zaku] | — |
| [Runner][runner] | — |
| [Monocurl][monocurl] | gpui-ce |
| [DBDelve][dbdelve] | — |
| [Coop][coop] | — |
| [Ropy][ropy] | — |
| [Baudrun][baudrun] | zed git + gpui-component git |

- The biggest outside user is Longbridge Pro, a commercial app built on GPUI Kit ([kit README.md:37, 110][kit-readme]).
- What this shows:
  - the Windows backend is used outside Zed
  - every project builds Windows on Windows CI runners
  - several sign their binaries
  - the ecosystem is young and split across four ways of consuming GPUI

## Licensing

- `gpui` and all `gpui-pre-*` crates are Apache-2.0 ([crates/gpui/Cargo.toml:9][gpui-cargo]; crates.io). So are `gpui-ce`, `gpui-unofficial`, `gpui-component` and `gpui-kit`. All are compatible with an MIT app.
- Apache-2.0 asks that redistributions keep the license and any NOTICE text. Plan a third-party-licenses file in the installer.
- **Avoid** Zed's `ui`, `theme` and `windows_resources` crates: they are GPL-3.0-or-later ([windows_resources/Cargo.toml:6][zed-winres-cargo]). Most of Zed's app crates are GPL too.
- Helpers mentioned here: `rfd` is MIT; `winresource`, `embed-resource` and `cargo-xwin` are MIT.

## Spike: what a small build must prove

A one- or two-day spike on the developer's machine would settle every inference above. Build a throwaway gpui-kit app containing:

- a tree of a few thousand folders with tri-state checkboxes and counts
- a progress bar fed by a background thread
- Cancel
- a modal dialog
- a drop zone
- the file pickers

Then check:

1. **Windows toolchain from WSL.**
   - Install rustup (MSVC), Build Tools and the SDK on Windows.
   - Run `cargo.exe build --release` driven from WSL.
   - Record which checkout location works (`/mnt/c` vs `\\wsl.localhost`) and how long builds take.
2. **Cross-compile check (optional, 30 minutes).** Run `cargo xwin build --release --target x86_64-pc-windows-msvc` in WSL. Source inspection predicts a missing `shaders_bytes.rs`, but this is an inference; if the build passes, inspect and run the exe on Windows.
3. **Size and runtime deps.**
   - Measure the release exe size.
   - Check with `+crt-static` that it starts on a clean machine without the VC++ redistributable.
4. **Rendering targets.** Run the release exe on:
   - the dev machine's GPU
   - over RDP
   - in a VM or on the Basic Render Driver
5. **Accessibility.** With NVDA and Narrator, confirm the tree, checkbox state (including mixed), progress and dialog are announced.
6. **Dialogs and drops.**
   - Check that `rfd` with a `*.ost` filter parented to the GPUI window behaves modally.
   - Check that dropping OSTs from Explorer works.
7. **Responsiveness.** While the background thread floods progress events, the UI stays smooth and Cancel takes effect within one item.
8. **Resources.** An icon and version info added via `winresource` link next to GPUI's manifest.

<!-- Sources -->

[zed-rev]: https://github.com/zed-industries/zed/tree/cb593458e1d6ed998709ac5257cb48ab147a5b12
[kit-rev]: https://github.com/longbridge/gpui-kit/tree/8b3631f020a85675dca2a7ba251fdeff1c867fc7
[blog-windows]: https://zed.dev/blog/zed-for-windows-is-here
[pr-34374]: https://github.com/zed-industries/zed/pull/34374
[pr-57928]: https://github.com/zed-industries/zed/pull/57928
[pr-56065]: https://github.com/zed-industries/zed/pull/56065
[pr-57954]: https://github.com/zed-industries/zed/pull/57954
[pr-60397]: https://github.com/zed-industries/zed/pull/60397
[pr-61926]: https://github.com/zed-industries/zed/pull/61926
[pr-38711]: https://github.com/zed-industries/zed/pull/38711
[pr-62525]: https://github.com/zed-industries/zed/pull/62525
[pr-64787]: https://github.com/zed-industries/zed/pull/64787
[i65799]: https://github.com/zed-industries/zed/issues/65799
[i65402]: https://github.com/zed-industries/zed/issues/65402
[i64695]: https://github.com/zed-industries/zed/issues/64695
[i62404]: https://github.com/zed-industries/zed/issues/62404
[i61508]: https://github.com/zed-industries/zed/issues/61508
[i61469]: https://github.com/zed-industries/zed/issues/61469
[i60140]: https://github.com/zed-industries/zed/issues/60140
[i58746]: https://github.com/zed-industries/zed/issues/58746
[i52448]: https://github.com/zed-industries/zed/issues/52448
[i37727]: https://github.com/zed-industries/zed/issues/37727
[i63812]: https://github.com/zed-industries/zed/issues/63812
[i26692]: https://github.com/zed-industries/zed/issues/26692
[i37231]: https://github.com/zed-industries/zed/issues/37231
[i41138]: https://github.com/zed-industries/zed/issues/41138
[i46263]: https://github.com/zed-industries/zed/issues/46263
[i62522]: https://github.com/zed-industries/zed/issues/62522
[readme-8]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/README.md?plain=1#L8-L12
[readme-43]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/README.md?plain=1#L43
[gpui-cargo]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/Cargo.toml
[gpui-build]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/build.rs#L19-L33
[gpui-manifest]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/resources/windows/gpui.manifest.xml
[gpui-ppo]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/platform.rs#L2875-L2881
[gpui-rwh]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/window.rs#L7616
[gpui-filedrop]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/interactive.rs#L758
[gpui-exec]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/executor.rs#L101-L103
[gpui-exec-fg]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/executor.rs#L354
[gpui-a11y-guide]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui/src/_accessibility.rs
[sched-task]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/scheduler/src/executor.rs#L399-L400
[gw-cargo]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/Cargo.toml#L5
[gw-build]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/build.rs#L3-L140
[gw-prompt]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/platform.rs#L784-L816
[gw-open]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/platform.rs#L1529-L1545
[gw-save]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/platform.rs#L1576-L1611
[gw-rwh]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/window.rs#L595
[gw-mica]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/window.rs#L920-L937
[gw-a11y]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/window.rs#L1084-L1122
[gw-drop]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/window.rs#L1157-L1280
[dx-devices]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/directx_devices.rs
[dx-sw]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/directx_renderer.rs#L826
[dx-runtime]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/directx_renderer.rs#L1830-L1842
[dx-include]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/gpui_windows/src/directx_renderer.rs#L1868-L1869
[ws-publish]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/Cargo.toml#L276
[ws-accesskit]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/Cargo.toml#L529-L532
[zed-cargo-config]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/.cargo/config.toml#L10-L16
[zed-build]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/zed/build.rs#L211-L214
[zed-winres]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/windows_resources/src/windows_resources.rs#L44-L84
[zed-winres-cargo]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/windows_resources/Cargo.toml#L6
[zed-iss]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/zed/resources/windows/zed.iss
[zed-bundle]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/script/bundle-windows.ps1#L329-L362
[zed-sign]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/crates/zed/resources/windows/sign.ps1#L14-L53
[install-112]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/installation.md?plain=1#L99-L119
[install-121]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/installation.md?plain=1#L121-L122
[win-66]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/windows.md?plain=1#L66
[devwin-18]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/development/windows.md?plain=1#L18-L64
[devwin-msys]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/development/windows.md?plain=1#L120-L124
[devwin-250]: https://github.com/zed-industries/zed/blob/cb593458e1d6ed998709ac5257cb48ab147a5b12/docs/src/development/windows.md?plain=1#L250
[kit-repo]: https://github.com/longbridge/gpui-kit
[kit-cargo]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/Cargo.toml#L62-L70
[kit-readme]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/README.md
[kit-comp-cargo]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/Cargo.toml#L1-L10
[kit-src]: https://github.com/longbridge/gpui-kit/tree/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src
[kit-tree]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src/tree.rs#L18-L50
[kit-base-tree]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/base/src/tree.rs
[kit-checkbox]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/base/src/checkbox.rs#L24-L111
[kit-progress]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src/progress/progress_circle.rs#L47-L67
[kit-root]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src/root.rs#L363
[kit-theme]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src/theme/mod.rs#L389-L394
[kit-registry]: https://github.com/longbridge/gpui-kit/blob/8b3631f020a85675dca2a7ba251fdeff1c867fc7/crates/component/src/theme/registry.rs
[kit-themes]: https://github.com/longbridge/gpui-kit/tree/8b3631f020a85675dca2a7ba251fdeff1c867fc7/themes
[cargo-cfg]: https://github.com/rust-lang/cargo/blob/master/doc/book/src/reference/environment-variables.md?plain=1#L401-L403
[rust-crt]: https://github.com/rust-lang/reference/blob/master/src/linkage.md?plain=1#L77-L95
[rfd]: https://crates.io/crates/rfd
[winresource]: https://crates.io/crates/winresource
[embed-resource]: https://crates.io/crates/embed-resource
[gpui-ce]: https://github.com/gpui-ce/gpui-ce
[gpui-unofficial]: https://github.com/iamnbutler/gpui-unofficial
[awesome-gpui]: https://github.com/zed-industries/awesome-gpui
[baudrun]: https://github.com/packetThrower/Baudrun
[baudrun-cargo]: https://github.com/packetThrower/Baudrun/blob/v0.15.2/Cargo.toml#L22-L34
[termy]: https://github.com/lassejlv/termy
[dbflux]: https://github.com/0xErwin1/dbflux
[gitcomet]: https://github.com/Auto-Explore/GitComet
[arbor]: https://github.com/penso/arbor
[eleven]: https://github.com/Zfinix/eleven
[hummingbird]: https://github.com/hummingbird-player/hummingbird
[codux]: https://github.com/duxweb/codux
[picoforge]: https://github.com/librekeys/picoforge
[zaku]: https://github.com/buildzaku/zaku
[runner]: https://github.com/yicheng47/runner
[monocurl]: https://github.com/monocurl/monocurl
[dbdelve]: https://github.com/ShayanAbbas1/dbdelve
[coop]: https://github.com/lumehq/coop
[ropy]: https://github.com/StudentWeis/ropy
