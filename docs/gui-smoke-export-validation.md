# GUI smoke export and check validation

Continuation of #49 from `b78c1ee`, using generated synthetic inputs only.

## Changes

- The guest writes conversions into the dedicated writable mapping, under
  `conversions/`. The host retains nested outputs and generated comparison OSTs
  (`sources/`), plus byte counts and SHA-256 hashes (`artifacts.json`). Guest
  failures, timeouts and early closes retain available files; retention errors
  leave staging available for recovery. Existing evidence is never overwritten.
- The smoke-only example opens the production wizard with the real engine and
  holds a nonzero progress event with a current folder until the screenshot is
  captured. The shipping executable has no gate. The guest publishes its result
  marker by renaming a closed temporary file.
- `scripts/check` checks every workspace crate, including `pstoff-format`, and
  runs the standard-library Python harness tests inside Sandbox with read-only
  Python/Git mappings. Missing check results and Python failures fail the route.
- Source snapshots receive fresh timestamps. Preserving old timestamps could
  otherwise reuse a cached test binary built from an earlier snapshot when an
  edit occurred before that earlier build finished.
- A headless GPUI test checks the rendered Output warning below, exactly at and
  above the 50 GB limit, the Convert warning above the limit, and removal on
  both screens after toggling Split. Only invisible
  test selectors were added to the views; layout, styling and copy are unchanged.

## Regression evidence

The first `python scripts/check` run reproduced the pre-fix export problem in
Sandbox: 19 Python tests ran, with one failure for the guest-local output path
and four retention errors covering success, guest failure, timeout and early
close. Its Rust tests, workspace fmt/clippy and 376-package dependency audit
passed. Failure logs are retained locally in `.scratch/check-red/`.

After the source-timestamp fix, the check visibly ran nine GUI tests, including
the new warning test, and the smoke example's real-conversion hold/release test.
The earlier timestamp-preserving snapshot had silently reused the eight-test
GUI binary. A Python regression covers backdated source edits.

## Commands and results

Run on 2026-10-26 from the task worktree. Every Sandbox launch went through
the shared lock and process checks. An intervening wrapper's Sandbox was left
untouched while this build waited for the lock.

```text
python scripts/check
python scripts/sandbox-cargo.py test --locked -p pstoff-gui --test smoke_fixtures -- --ignored
python scripts/sandbox-cargo.py build --release --locked -p pstoff-gui --example gui-smoke --config "target.x86_64-pc-windows-msvc.rustflags=['-C','target-feature=+crt-static']"
python scripts/gui-smoke.py
git diff --check
```

- Final `scripts/check`: exit 0. Dependency audit: 376 Windows packages;
  workspace fmt/clippy: clean; workspace Rust tests: 151 passed, two explicitly
  ignored; smoke example: one passed; Python harness: 25 passed, including the
  atomic-marker and backdated-source regressions. The workspace run includes
  all 60 `pstoff-format` tests and all nine headless GUI tests.
- Synthetic fixture generation: exit 0, one generator test passed.
- Static release smoke example build: exit 0.
- GUI smoke: exit 0; nine screenshots, two PSTs and four JSON/text reports
  retained. Both JSON reports parsed inside the guest. Visual inspection of
  `5-converting.png` confirmed 7% overall, a current folder, the first OST at
  1 of 6 items and the second OST waiting. Output and Convert screenshots were
  inspected without changing the design.
- Retained-artifact metadata verification: all ten source/output files matched
  the manifest's byte lengths and SHA-256 hashes.
- `git diff --check`: clean.

Within the `scripts/check` guest session, the exact commands were:

```text
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p pstoff-gui --example gui-smoke
C:\PstOffPython\python.exe -I -S -B -m unittest discover -s C:\PstOffSource\tests -v
```

The wrapper then passed the guest metadata to `scripts/audit-dependencies.py`
on the host, as prescribed by the existing route.

Local evidence is retained under `.scratch/gui-smoke/20261026-164546/`, including
the four generated inputs in `sources/`, all six conversion outputs in
`conversions/`, `smoke.log`, `finished.txt` (`0`), the held/released progress
markers and `artifacts.json`. Logs from the final check are retained locally
under `.scratch/`; generated mail files and screenshots are not committed.
The final guest logs and step exit codes are copied to `.scratch/check-final/`;
the host route's output is `.scratch/check-final.log`.

| Generated PST | Bytes | SHA-256 |
| --- | ---: | --- |
| `conversions/1-mailbox.pst` | 29696 | `c7bd20f2740af2f68a40fcb3876a3f5cd9f2991b45a7dd9b87deaca6281b6654` |
| `conversions/2-archive.pst` | 29696 | `78d15a47428c308640671e2189df084a0b6e297751671f233f27ebd7e58217ef` |

Each matching generated source OST is 32768 bytes with SHA-256
`8c790b4b46a315c6901708560a3e5d52c1f8bce81a59ce75af77f322b1781b38`.
The manifest also records hashes and lengths of the reports and invalid-input
controls. These are provenance and retention checks, not Faithful results.

## Unresolved acceptance gates

This work does not establish Faithful conversion. The retained source/PST pairs
still need the separately authorized scan-only SCANPST check, independent
libpff comparison and manual classic Outlook check. None was executed here.
No private or unknown mail source was read, copied or opened.

The topic branch retains the requested `b78c1ee` base. During validation the
remote `spec/issue-20` advanced to `65be12bb457354954389802d45c981b335c3e989`.
No rebase, PR update or merge was performed. Integration with that newer head
and checks on the resulting integration commit remain with the parent task.

`pstoff-format` includes legacy tests that return early when external fixture
files are absent. Those files were not supplied or fetched; its generated and
in-memory tests execute. The check's success does not establish coverage of
those external-fixture cases or complete every acceptance gate on #20/#49.
