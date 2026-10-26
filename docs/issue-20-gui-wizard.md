# GPUI wizard continuation (#20, #37–#40)

The shared engine preview and report-schema follow-up is recorded in
[shared-plan-report.md](shared-plan-report.md); the notes below describe the
earlier GUI milestone.

Worktree: `t3-8239c749`, branch `t3/issue-20-gui-wizard`.
The substantial inherited uncommitted GUI work was preserved. Integration
fast-forwarded the branch from `92f2b870` to the engine/CLI chain at `7cc035f2`,
then restored the GUI work without conflicts. A named preservation stash remains
available locally. No private or unknown mail inputs were used.

## Design plan and review

The required Claude frontend-design skill was read before UI edits. The subject
is a careful copy of a mailbox cache, with an explicit statement that the full
mailbox cannot be established from an OST.

- Base tokens: Mist #F2F4F6, Paper #FFFFFF, Slate ink #1D2733, Pewter #5A6573,
  copy teal #0B7A6F. Amber #9A5B00 on #FFF3DC marks warnings; failure #B42318;
  keyboard focus #2B63D9.
- Segoe UI Variable carries headings and ordinary text. Cascadia Mono is reserved
  for actual paths, filenames and property identifiers.
- Variant A: 248px step rail, left-aligned content up to 760px, pinned footer.
  Folder selection places the tree beside a 224px totals panel. Conversion
  progress stays in the content column; Start conversion lives in the footer.

```text
Add / Output / Convert / Report       Choose folders
+------------+------------------+     +------------+-----------+--------+
| step rail  | question         |     | step rail  | tree      | totals |
|            | content         |     |            |           |        |
| Copy mode  | Back        Next|     | Copy mode  | Back           Next|
+------------+------------------+     +------------+-----------+--------+
```

Review before building rejected the inherited violet palette, repeated rounded
settings cards and centered conversion progress. Settings now use unboxed
sections; white panels hold file/tree/report objects. The completeness statement
is the prominent element and keeps the engine's exact wording. Screenshot review
and comparison against the original Variant A remain a separate verification
step; source inspection is not a visual acceptance result.

## Implementation

The wizard owns source identities, asynchronous inspection results, folder
selection, output choices, navigation and the queue. GPUI calls only the engine's
public inspect/convert seam. Inspection uses the background executor; conversion
uses a worker thread and a 50ms progress poll. Closing while converting asks for
Cancel; cancellation preserves completed output and marks queued sources not
started. Reports retain local references, incomplete evidence and completed files
from failures. Split preview names match the integrated engine even for one part.
Buttons expose keyboard focus and Enter/Space activation.

The GUI smoke runner uses a separate clean Sandbox with only the executable,
synthetic fixtures and guest script mapped read-only, and a dedicated writable
output directory. It leaves an existing Sandbox untouched. It clicks the pinned
footer, screenshots the stages and requires reports from both generated inputs.
Its conversion screenshot may capture the report if the tiny fixtures finish
before the screenshot; a deterministic in-progress runtime capture is still needed.

## Verification and remaining gaps

`PYTHONUTF8=1 python scripts/check` passed (exit 0): the 376-package
Windows dependency audit, formatting, Clippy with warnings denied, and 85 tests.
The GUI adds 19 model tests and eight headless GPUI tests. Two fixture-export
tests remain explicitly ignored. The full check includes the integrated engine
and CLI regressions. The keyboard regression first exposed that GPUI tracked
focus handles need their own tab-stop flag; the passing test now exercises
Tab/Enter opening Browse. The responsiveness regression holds a real conversion
at its initial progress event, asserts continued frames, and confirms Cancel
cleanup and not-started queue outcomes.

The static-CRT release build passed in Sandbox (8m 03s). The executable is
12,018,176 bytes, SHA-256
`aa40f60f8c52968bea0e264f647a79ca8c6c0b6eca4f782b1520c31fb4b0d107`.
A local copy and receipt are under `.scratch/gui-release/`.
The explicit synthetic smoke-fixture export also passed.

The clean-Sandbox smoke run passed: both generated supported OSTs produced a
PST and JSON/text reports, the wizard navigated Add → Folders → Output →
Convert → Report, and the app exited with code 0. The first attempt exposed a
harness bug: sizing beyond the guest working area let the taskbar cover the
footer. The smoke script now uses the guest working area. The executable did
not need to change. Evidence is under `.scratch/gui-smoke-workarea/`. The final expanded smoke run
also passed (exit 0) with nine screenshots under `.scratch/gui-smoke-reviewed/`,
including scrolled status/settings/report captures; those three additional
captures were visually reviewed too. The footer remains pinned while scrolling.

Visual review of all six initial passing captures confirmed the rail, tree and
totals panel, left-aligned content and pinned footer; no overlapping app text or
controls were seen. Add queues and long settings/reports scroll above the footer.
Compared with round-2 Variant A source on `prototype/converter-frontends-2`, the
intentional differences are the approved teal/Mist palette, 248px rail instead of
250px, bounded 760px content, unboxed settings, 224px totals sidebar, the stronger
completeness band and Start conversion in the footer. The original prototype's
centered in-progress screen is replaced by left-aligned progress in the source;
these tiny native smoke fixtures finish before that screen is captured. The
headless gated-conversion test verifies its progress rows and responsiveness.

- The output preview estimates part count and sizes from folder bytes. There is
  no public engine dry-run API; whole-Item boundaries and final PST overhead can
  change both. The saved report is authoritative.
- Native Browse/drop and saved-report launch still need practical Windows
  integration evidence; synthetic headless events alone do not establish them.
  Startup, conversion, report publication and clean exit passed in a clean guest.
- The basename field currently supports append/backspace and ending editing. It
  is not a full native text editor (selection, caret navigation, paste and IME).
- Custom tree/settings controls remain mouse driven; full accessibility-specific
  behavior is outside v1 per #44. Buttons support ordinary keyboard activation.
- The 50 GB warning is covered by the planning-model regression; a dedicated
  GPUI assertion of its rendered warning is still absent. The Python smoke
  configuration tests are added but are not included in `scripts/check`.
- A deterministic in-progress screenshot remains needed for full #39 screenshot
  acceptance. Final visual critique and intentional Variant A differences are
  recorded above; ordinary screen captures passed.
- Release notice packaging still belongs to #41. The reconstructed notice for
  seahash/simd_helpers explicitly distinguishes package-author attribution from
  an upstream notice and makes no claim about upstream copyright years.
- The inherited engine limits remain: initial hashing/inspection delay Cancel,
  trial-finalized Split can do quadratic work, large-store memory is unproven,
  and strict SCANPST/libpff/manual classic Outlook Faithful acceptance is separate.

No PR metadata was changed and no PR was merged.
