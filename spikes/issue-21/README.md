# Q16 item-row rebuild: bounded result

**FAIL/3.** Rebuilding cached item rows under accepted Q16 does not clear strict
SCANPST on the public fixture. The same normal NIDs `200084`, `2000E4`, `200184`
remain. Twelve diagnostic trials also fail with exactly those three rows.
**No minimal sufficient column set or exact remaining cause was isolated.**
This is a negative bounded experiment, not a valid-PST or Faithful claim.

Candidate: `/home/watermelon/.local/share/pstoff-spike21/output/q16-final-public.pst`

SHA256: `32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765`

The final cleaned source independently reproduces that exact hash at
`output/q16-final-source-replay.pst`. Both strict runs finish, preserve original
and Windows-copy hashes, and return strict exit 1 with three diagnostics.
[Aggregate evidence](evidence/results.json) records the candidate, all trials,
controls, policy guards and preservation checks. Raw values/logs stay local.

## Policy and writer seam

`q21.rs` plans overrides only for contents, associated-contents and search-contents
tables. It reads each item's PC **after** applying the same accepted Q12 timestamp
and Q15 pair plans used by the writer. No Q16 path writes a PC. Exact property-tag
matches copy raw PC bytes, not display strings or re-encoded values. The default
conversion mode is `all`; experimental modes require an explicit CLI argument.

Documented computed cases implemented here are row/parent NIDs, the tracked-index
branch of Conversation ID, Unicode subject metadata removal, display fields,
HasAttachments from message flags, and SecureSubmitFlags/ContentFilterSpamConfidenceLevel
zero results. See [primary sources and provenance](PROVENANCE.md). All three normal
PCs use the tracked Conversation ID branch; their subjects are plain, so the
subject rule makes no change. This is not a complete implementation of every
MS-PST calculated property: unsupported derivations, including the Conversation
ID topic-hash fallback, retain and report the cache. There is no guessed default
for an arbitrary absent PC property.

Status and replication-table-only properties retain their table state. Unknown,
missing, differently typed or unsupported-width properties likewise retain their
cached presence/value and receive a reason. A missing PC is an error, not a
reason to discard a row. The provenance log identifies each table, item and tag
locally; published evidence aggregates counts and tags without content values.

Before editing, the inherited validator checks TC signatures, row dimensions,
leaf row indexes, row identity and counts. Q16 additionally checks column bounds,
unique CEB bits and nonoverlapping storage. It preserves all descriptors, row
IDs, index entries and unrelated allocations. Changed variable cells receive new
HIDs rather than overwriting shared or structural allocations. External row
matrices retain their source block boundaries; unsupported allocation expansion
fails closed at 8,176 bytes rather than splitting a heap. Inherited Q15 guards
remain active. Thirteen Q16 selection checks cover missing/type-mismatched PC
properties, table-only precedence, computed IDs/defaults, malformed subject and
conversation metadata, and attachment flags. These are scoped selection checks,
not exhaustive malformed-file parser coverage.

Across eight populated tables, 17 distinct items have 18 row occurrences:
263 column occurrences are regenerated/verified, and 781 retain cached state
(59 present, 722 absent). Thirteen cells differ from the accepted Q15 baseline:

- `0042001F`: three existing sender-representative names replaced by exact PCs.
- `30130102`: three existing Conversation IDs rebuilt from tracked PC indexes.
- `0E03001F`: four formerly absent display-CC cells become documented empty strings.
- `65C60003`: three formerly absent SecureSubmitFlags cells become documented zeros.

These operations are within Q16 and leave item properties unchanged. The initial
PC-copy/index-ID rebuild changed only the first six cells and also stayed FAIL/3.
Adding the other documented calculated cells still stays FAIL/3.

## Bounded diagnosis and causal finding

Each numbered trial affects only the three diagnostic normal items (including
same-item search-table occurrences where that column exists). Single-column trials
start from the Q12/Q15 source behavior, not cumulatively from the previous trial.
All twelve finish, preserve input hashes and remain **FAIL/3**:

1. Rebuild subject only: no semantic change; these PCs have plain subjects.
2. Rebuild `0042001F` only.
3. Rebuild `30130102` only.
4. Rebuild those two columns together.
5. Rebuild display `0E03001F` only.
6. Rebuild `65C60003` only.
7. Rebuild HasAttachments `0E1B000B` only (already agrees).
8. Rebuild flags `0E070003` only (already agrees).
9. Rebuild size `0E080003` only (already agrees).
10. Use public donor descriptor order with the full supported rebuild, retaining
    every source column/value and appending columns absent from the donor.
11. Diagnostic only: supported rebuild plus clearing presence of `66120003` and
    `66280014` on the three rows.
12. Diagnostic only: the previous trial plus clearing `0E170003` presence.

Trials 11/12 are explicitly **outside accepted output behavior**: they test cache
presence, not a supported derivation, and are never selected by default. They
change no PC and invent no replacement value. Their failures provide no rationale
to delete these cache fields in conversion. Required row identity/version fields
and all rows remain present; scanner diagnostics are never filtered or waived.

**Donor limitation:** no wholly clean normal-item fixture was available in the
bounded search. Public `dist-list.pst` is FAIL/133, `passworded.pst` FAIL/155, and
a supplemental libyal Outlook fixture FAIL/408. The selected normal donor row
in `dist-list.pst` is not reported as mismatching, unlike its other three normal
items, but its file has missing-schema diagnostics. Thus trial 10 tests that
row's descriptor ordering, not a proven globally valid normal-item row shape.
It preserves the target's additional columns and does not copy another item's
values or presence mask. A strict PASS donor transplantation is **not claimed**.

The established finding is insufficiency: the supported sender/conversation/
computed-cell corrections, and these bounded layout/cache-presence combinations,
do not resolve the three failures. No column value is established as the cause.
Untried column interactions, calculated rules and reader/writer issues remain
possible. The experiment stops at the requested trial bound, not at a claim that
all Q16 implementations must fail. No concrete further PC-property decision is
justified yet; recommend retaining this artifact and requiring an isolated cause
before authorizing any broader item-property change.

## Preservation and controls

Against the accepted Q12/Q15 candidate:

- 308 NIDs and parent links remain; 136 subnode streams are byte-identical.
- All 17 item PC streams are byte-identical. All 1,550 decoded properties across
  64 property-bearing objects are exact, with no additions/removals or changes.
- All 67 table-row identities remain. Sixty-three rows are semantically identical;
  the three normal contents rows and one search occurrence change as listed above.
- 801 existing table cells are identical, six change, seven are added, none vanish.
- Required columns, independent cell allocations and header fingerprint checks pass.
  Microsoft's secondary reader opens the candidate with Valid2 maps, reads the
  root hierarchy and opens the IPM subtree.

Every trial also passes the graph/subnode/PC-stream/row-identity audit. This is
same-writer preservation checking plus a secondary open, not independent complete
payload/table validation. The valid Empty control is **PASS/0**, strict exit 0.
The invalid 4 KiB zero control is **FAIL/1**, strict exit 1 (its log does not finish).
Both preserve original and Windows-copy hashes. Raw SCANPST exit codes are not
the gate verdict.

No candidate passes, so the conditional independent offline libpff comparison
was not run. Prior comparisons are not presented as testing this candidate.
Outlook acceptance and full payload equivalence remain unproved.

## Replay and containment

Only this directory is changed. Branch `t3/table-row-rebuild` starts at
`24019275a89f7521fbd3c4b68180e4240e8a21da`. Issue-16 through issue-20 and their
outputs are untouched. Existing public pinned clones/configuration were reused;
all new preparation, target files, provenance and outputs use the issue-21 root.

**Every Cargo invocation from the first build, including dependency build scripts,
and every candidate/probe run used bwrap, unshared network and a cleared environment.**
No user home contents are mounted; only the explicit read-only compiler/registry,
public source inputs, and the dedicated build/output paths are exposed. Formatting
and Python orchestration do not compile or run the candidate on the host.
SCANPST is the installed Windows binary, always `-silent -no repair`.
Allocation maps are regenerated by the Microsoft library transaction flush.
The strict gate's detector is unchanged from issue-20; only its public allowlist
and collision-free scan paths differ. Each Windows scan uses a fresh alias and
refuses an existing scan path, preserving earlier scan copies/logs too.

```sh
python3 spikes/issue-21/bootstrap.py  # reuse existing pinned public sources first
python3 spikes/issue-21/prepare.py
python3 spikes/issue-21/build.py      # the ONLY Cargo build entry point
python3 spikes/issue-21/reproduce.py fresh-q16-label
# strict exit 1: FAIL/3; choose an unused label on every run
python3 spikes/issue-21/trials.py --prefix fresh-diagnostic-series
# 12 bounded diagnostic modes, not production policy
python3 spikes/issue-21/scan-loop-strict.py \
  /home/watermelon/.local/share/pstoff-spike21/output/fresh-q16-label.pst
```

`collect.py` verifies the specifically named artifacts from this run and writes
sanitized evidence; it exits zero for verified experiment expectations, **not**
a clean PST. Fresh prefixes require changing its input-label list to collect a
separate run. `donor-download.py` optionally reproduces the rejected supplemental
libyal donor download into its own new `/tmp` directory. No downloaded data is
executed. Preliminary aborted outputs and intermediate candidates are retained
locally under their original names; they are not used as final evidence.

No private fixture was read, listed or mounted. No earlier output was overwritten
or deleted, no global Git configuration changed, and no issue comment, issue
closure or PR was created. No additional model/provider or worker was launched.
