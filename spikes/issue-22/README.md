# Final bounded round: table-only presence cause isolated

**Accepted output: FAIL/3. Diagnostic output: PASS/0.** Eleven diagnostic trials
isolate an inclusion-minimal sufficient set: the present contents-table cells
`65C60003` and `66280014`, both absent from each normal message's PC. Clearing
both presences removes all three scanner findings; clearing either alone leaves
all three. Adding both values from the rebuilt rows to those PCs also passes,
but that is diagnostic only and is not converter behavior.

The accepted Q12/Q15/Q16 behavior remains unchanged. Its fresh replay is:

`/home/watermelon/.local/share/pstoff-spike22/output/accepted-replay.pst`

SHA256: `32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765`

The minimal passing **diagnostic**, not approved conversion output, is:

`/home/watermelon/.local/share/pstoff-spike22/output/trial08-clear-pair.pst`

SHA256: `02e2b8d2a671eef76d47398aeda0a2e9fa338a5e628f47c569feaeccf2d899d5`

[Sanitized evidence](evidence/results.json) includes every strict scan, hashes,
exact failing row IDs, controls, structural inspection and preservation audits.
No raw message values or raw scanner logs are published.

## Causal experiment

Every trial begins with the same accepted Q12/Q15/Q16 rebuild, independently of
previous trials. T1 examines cells *after* the rebuild, including computed values
that Q16 makes present. Exact property tags are compared against the post-Q12/Q15
PC. Required `67F2`/`67F3` row identity/version fields are excluded. Same-item search
occurrences are included when the field exists. Clearing changes only the CEB
presence bit; it keeps descriptors, stored cell bytes, rows and heap allocations.
Maps are regenerated through the inherited Microsoft transaction flush.

The actual adaptive sequence was:

1. Clear every present table-only cell: **PASS/0**. This removes 25 cell presences
   across the three normal rows and one search occurrence, in nine tag groups.
2. Clear `0E03001F,30130102,65C60003`: FAIL/3.
3. Clear `0E05001F,0E170003,0E380003,66120003,66280014,67F10003`: FAIL/3.
4. Clear the union of trials 2/3 except `65C60003`: FAIL/3.
5. Clear `0E03001F,30130102,65C60003,0E170003,66120003,66280014`: **PASS/0**.
6. Remove `0E170003` from that clearing set: **PASS/0**.
7. Remove `66120003` from the remaining clearing set: **PASS/0**.
8. Clear only `65C60003,66280014`: **PASS/0**.
9. Clear only `65C60003`: FAIL/3.
10. Clear only `66280014`: FAIL/3.
11. Corroborating T2 diagnostic: add only those two table values to all three
    PCs, retaining table presences: **PASS/0**.

Trials 8–10 establish minimality within this two-tag clearing set, not uniqueness
among all possible fixes. All FAIL/3 trials report precisely `200084`, `2000E4`,
`200184`, with no added diagnostics. Every trial finishes and leaves both original
and Windows scan-copy hashes unchanged. Raw SCANPST exit code is 1 even on these
PASS/0 results; the unchanged strict diagnostic detector determines the verdict.
No diagnostics are filtered or waived.

**Why previous single-field experiments missed this:** accepted Q16 introduced a
present computed-zero `65C60003` cell in each normal contents row. Clearing
`66280014` while leaving that Q16 addition present still fails. Conversely, omitting
the computed field while retaining the cached `66280014` still fails. The findings
are reported per row, so fixing one of two discrepancies need not change the
three-line count. The field pair is a demonstrated sufficient cause; the scanner's
internal comparison algorithm has not been reverse engineered.

The broad T2 add-all trial was unnecessary after T1 passed and its minimal set was
isolated. Trial 11 tests that minimal set directly. No further hypotheses were
spent after this corroboration; no accepted fix was inferred from a diagnostic
PASS.

## Sub-object inspection

The structural probe ran against the pinned public OST and the accepted PST.
All 136 subnode streams are byte-identical through conversion. Each failing normal
message has one recipient TC at `692`, one recipient row, all 14 required template
columns, valid leaf row indexes and row identities, integral matrices, bounded
nonoverlapping columns and distinct in-range CEB bits. Recipient row IDs are
source identities, not required to be sequential. The populated recipient heaps
are single blocks of 476, 496 and 560 bytes with heap-resident matrices, so no
multi-page/4 KiB-to-8 KiB row-matrix translation is exercised for these TCs.

Each message's recipient-derived To display agrees with its PC, and its empty
CC/BCC results agree with absent-PC defaults. Each has zero attachment tables and
zero attachment objects; message attachment flags and HasAttach agree with that.
The store's empty `692` and `671` templates have all required recipient/attachment
columns and no rows. One message additionally has a `6B6` TC (type `16`, five
columns, 17 rows); it has a valid index, matrix and CEB. Other subnodes are data
streams, not recipient/attachment TCs, and are preserved verbatim. An unrelated TC
is not incorrectly required to carry attachment-template columns.

The probe also records decoded stream-plus-subnode byte totals. These do not equal
cached MessageSize. That total includes heap structures and is **not** a validated
MAPI MessageSize calculation. Actual logical message-size recomputation was not
completed after the table-presence cause was isolated. No size value was changed.
The existing row/PC size agreement remains, and the passing diagnostic preserves
both size values. No value-preserving sub-object translation defect was found.
See [specification links and limitations](PROVENANCE.md).

## Preservation, controls and policy decision

The minimal diagnostic versus accepted Q16 has:

- All 308 nodes and parent links, 136 byte-identical subnode streams, and all
  17 byte-identical item PC streams.
- All 64 decoded PCs / 1,550 properties unchanged, with no PC additions/removals.
- All 67 row identities, 64 completely unchanged rows, 808 identical existing cells,
  no changed values or added cells, and exactly six cleared presences: three of
  each isolated tag. Search rows remain unchanged in this minimal trial.
- Required columns, independent cell allocations and header checks passing.
  Microsoft's secondary reader opens the passing diagnostic with regenerated maps.

Trial 11 preserves all existing raw PC tags and values and adds exactly six
properties (`65C60003` and `66280014` in three PCs); it preserves the same graph,
subnodes and row identities. It is explicitly forbidden as accepted behavior.
The accepted replay preserves every PC, row and cell and reproduces spike-21's
exact SHA256. Q12/Q15/Q16 selection guards pass using their intended inputs.
The valid Empty control is PASS/0; the invalid zero control is FAIL/1. Both preserve
original and Windows-copy hashes. Same-writer audits and a secondary open are not
independent complete table/payload verification.

**Recommendation needing a product decision:** authorize a narrow contents-table
rule that leaves these two cells absent when the item PC lacks them: do not
materialize the computed `65C60003` zero, and do not carry the cached `66280014`
value into a present PST cell. Record this exception and retained source evidence
in the conversion report. This leaves item properties unchanged and avoids
inventing properties merely to satisfy the scanner. The meaning and general
handling of `66280014` have not been established, so do not generalize this to all
table-only fields. This proposal changes table-generation policy; it has **not**
been applied to accepted behavior. No new PC promotion or guessed value is needed.

Independent offline libpff was conditional on a PASS with accepted behavior or a
value-preserving writer fix. Neither exists in this round, so it was not run and
no prior comparison is relabelled. Outlook acceptance and broader fixture/payload
coverage remain unproved.

## Replay and containment

Only `spikes/issue-22/` is changed. Branch `t3/subobject-mismatch` starts at
`912a88c737718edc2ec0e3ecce0cbb4c88936aa7`. Earlier spike directories and outputs are
untouched. Existing pinned public `sources.json` is reused in the new output root.

```sh
python3 spikes/issue-22/prepare.py
python3 spikes/issue-22/build.py
python3 spikes/issue-22/reproduce.py fresh-accepted all  # expected FAIL/3
python3 spikes/issue-22/reproduce.py fresh-minimal t1:65c60003,66280014  # diagnostic PASS/0
python3 spikes/issue-22/trials.py --prefix fresh-replay
python3 spikes/issue-22/collect.py
```

Preparation requires `/home/watermelon/.local/share/pstoff-spike22/sources.json`,
a copy of the existing issue-21 pinned public configuration. `collect.py` verifies
the fixed artifact names from this completed round; it does not collect arbitrary
fresh prefixes. It caches the completed controls and checks their hashes rather
than rescanning them on every collection.

Every Cargo invocation, including build scripts, and every candidate/probe execution
ran in bwrap from the first build, with no network, cleared environment, and no
home mount. Only explicit public inputs, read-only compiler/registry/dependency
mounts, and dedicated build/output paths were exposed. SCANPST always used
`-silent -no repair`, fresh scan aliases, and hash checks. No repaired output was
used. No private fixture was read, listed or mounted. No extra model/provider or
worker was launched, no downloads were needed, no global Git configuration changed,
and no PR or issue comment/closure was created.
