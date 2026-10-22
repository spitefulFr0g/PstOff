# Accepted Q12 source-cache timestamp spike (PUBLIC only)

Q12 is implemented. The strict SCANPST gate still **FAILS**: diagnostics fall
from 51 to 17, all remaining lines reporting table/object mismatches. There are
zero missing-modification-time findings. This is an unmerged fixture experiment;
it makes no Faithful, server-history, or Outlook acceptance claim.

Output: `/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst`,
230,400 bytes, SHA256
`dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177`.
It is converter output plus Microsoft's allocation-map rebuild, with no repair.

The prior issue-17 artifacts are imported unchanged from `06e08ac`. The separate
prepared source, output root and probe package live under `pstoff-spike18`; the
original pinned clones and prior worker/output trees are untouched.

## Policy and writer seam

Only missing `30080040` on indexed message NIDs (normal type 4, associated type 8)
is eligible. Inspect raw source contents/associated/search contents TCs (types
14, 15, 16), including external source row matrices. Validate TC signatures and
lengths, row-ID descriptor/type/length/presence, row-index-to-row identity and
count, timestamp descriptor `30080040`, eight-byte length, bounds and presence.
A usable FILETIME is nonzero and at most `i64::MAX`; zero and signed-high sentinel
values stay unresolved. Every occurrence for the same NID must be valid and
byte-identical. An unreadable/unsupported relevant table conservatively blocks
all missing-item promotions, since it could conceal another occurrence.

An existing item property with ID `3008` always wins, including a different type
or invalid value. Nothing overwrites it. Missing, invalid and conflicting cache
observations produce unresolved per-item diagnostics rather than guessed values.
The local report records source NID, normal/associated kind, reason, source table
NIDs and occurrence count for each item, without publishing timestamp values.
Seventeen messages (3 normal, 14 associated) qualify; one appears in two tables,
so 18 validated occurrences agree with the 17 promoted item values.

The `PSTOFF_Q12` experiment flag enables planning before emission. The actual
writer checks existing records again, inserts exactly one sorted PC record and
one eight-byte heap allocation, retaining all previous HIDs, allocations and
record bytes. It aligns the heap map and refuses expansion past a PST block.
The source cache observation does **not** establish the true server history.

## Exact verification and remaining findings

- Prior strict RED: finished scan, unchanged original/copy, **51** diagnostics.
  The new output-presence regression is also RED on that prior file (17 missing).
- New policy guards: **PASS** on all 17 actual source PC heaps; exact emitted
  bytes, existing allocation/record preservation, existing-property precedence,
  missing/invalid/conflicting observations and invalid-table rejection. Seven
  mutations of actual source TC bytes test wrong type/length, absent cell,
  invalid FILETIME, row identity, missing descriptor and duplicate descriptor.
- Whole-file existing-property test: reconvert the new output with Q12 enabled;
  all 17 are classified preserved, and every message stream stays byte-exact.
- Preservation: **308** indexed NIDs/parents; **136** recursive subnode streams;
  **67** decoded rows; **1,493** existing raw-decoded property values match.
  No property key disappears. All existing item values match. Exactly 17 item
  properties are added (`30080040`), and only those 17 item streams change.
  The other 249 non-store/non-contents/non-message streams stay byte-identical.
  Inherited upstream store behavior changes `3001`, `67ff` and adds `35df`,
  `35e0`, `35e3`, `35e7`; these are unchanged from issue-17's conversion policy.
  Table schema/header/ownership corrections are inherited from issue-17.
- New strict scan: **FAIL, 17 diagnostics**, finished scan, unchanged original
  and Windows copy. Missing modification times, required columns, failed addref,
  rgbFP, invalid content count and bad heap map counts are all **zero**.
- Valid Empty control: **PASS**, zero diagnostics, raw exit 1. Invalid zero
  control: **FAIL**, incomplete log/one diagnostic, raw exit 0. Raw exit status
  remains unsuitable as the diagnostic gate.
- Microsoft's reader opens the output with Valid2 maps and the IPM subtree.
  Complete independent libpff comparison and Outlook assessment belong to parent.

Bounded diagnosis: all 18 cached modification times now match item properties
exactly. Other source cache/item discrepancies remain unchanged: 18 cache rows
have `0e17` and `6612` absent in the PC, 17 have `65e2`/`65e3` absent; less common
absences also remain, and `0042` differs in three rows. These observations do not
identify which comparison SCANPST rejects. No other cached fields are promoted or
removed; resolving those findings requires a separate technical investigation
and possibly another explicit policy decision.

The first experimental Q12 emission had six misaligned heap maps and 71 strict
diagnostics. Aligning HNPAGEMAP fixed those writer-introduced defects; that trial
is retained locally as `q12-cache.pst`. The final output has zero such findings.
No repaired output was used at any stage.

## Reproduce

Prerequisites: pinned clones and public inputs from [PROVENANCE.md](PROVENANCE.md),
offline Cargo registry, installed rustc 1.99.1, bubblewrap, WSL PowerShell and the
existing Office SCANPST 16.0.20527.20088. No rustfmt is installed or needed.

```sh
python3 spikes/issue-18/prepare.py
/home/watermelon/.cargo/bin/cargo build --offline --release \
  --manifest-path spikes/issue-18/probe/Cargo.toml \
  --target-dir /home/watermelon/.local/share/pstoff-spike16/target
python3 spikes/issue-18/reproduce.py
# exit 1: policy/preservation checks PASS, strict zero-diagnostic gate FAIL
python3 spikes/issue-18/scan-loop-strict.py \
  /home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst
# exit 1, 17 diagnostics, finished/unchanged
```

Replays refuse to overwrite generated candidates; use `--label q12-repeat-1`
for another isolated replay. The report is written even when the gate fails.
Unlike issue-17's exploratory replay, **exit 0 means the strict gate passed**;
expected remaining findings do not turn a failed gate into success.

Every candidate runtime uses the existing `bwrap --unshare-all` runner with
read-only input, cleared environment, no home or network, and only this PUBLIC
output root writable for writers (read-only for probes). Windows copies PUBLIC
input to TEMP and invokes `SCANPST -silent -no repair`. Both original and copy
hashes are checked. Raw dumps/logs and timestamp values stay local. No private
file access, other provider, delegation, issue changes, PR or main mutation is
part of this work. Existing subscription-authenticated Codex billing constraint
is preserved.

See [evidence/results.json](evidence/results.json) for sanitized per-item
provenance, gate summaries and exact changed/added property keys.
