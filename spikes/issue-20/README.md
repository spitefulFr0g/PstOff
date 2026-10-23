# Accepted Q15 and bounded normal-row writer investigation

**FAIL/3.** The accepted Q12/Q15 candidate still fails strict scan-only SCANPST
for exactly normal NIDs `200084`, `2000E4`, `200184`. The cause of these three
mismatches is **not isolated**. Ten bounded representation trials do not produce
a clean candidate. There is no Faithful claim or Outlook acceptance result.

Candidate:
`/home/watermelon/.local/share/pstoff-spike20/output/q15-validated-public.pst`

SHA256:
`e65984caded46d3c19603c45e19c54a82efc9b469f05796a2ba866e481d9f906`

The new branch starts at `9d704fc`. Earlier spike directories and artifacts are
unchanged. This directory contains the full replay inputs, scoped patches and
[sanitized evidence](evidence/results.json); raw message values and scan logs
remain outside Git. See [provenance and containment limits](PROVENANCE.md).

## Accepted policy implementation

`q15.rs` replaces the former diagnostic pair-promotion with an unconditional,
policy-guarded conversion plan. It validates heap allocation maps, TC headers,
row-index identity/counts, unique NIDs, column order/bounds/overlap/presence bits,
binary pair descriptors, HNID/subnode resolution, XID lengths, complete PCL
serialization (strict ascending binary namespace GUID order, no duplicate
namespaces or trailing bytes) and containment of the paired key. Structural allocations cannot
serve as pair cells. Existing PC properties are never overwritten. Any relevant
table integrity failure prevents promotion; missing, invalid or conflicting
observations and existing-member conflicts remain reported and unresolved.

Every table exposing either pair descriptor must supply both valid cells for
that item, and all such occurrences must agree. A search table exposing neither
field is not a cache source for this pair; excluding that table does not select
one conflicting value over another. An existing pair member must agree in type
and bytes before its missing partner can be added. Both-existing properties are
kept, with conflicts still reported. Per-item property count, reason, source
table NIDs and occurrence counts are recorded locally and in sanitized evidence.

The PCL serialization check follows [MS-OXCFXICS §2.2.2.2](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-OXCFXICS/%5BMS-OXCFXICS%5D-190618.pdf).
It checks serialized consistency, not authentic server history.

The public fixture adds exactly 34 properties to 17 PCs. The final run reports
no unresolved Q15 observations. Selection guards cover missing/invalid cache,
invalid table state, disagreement, existing type/value conflicts, malformed
XIDs/PCLs, pair inconsistency, one missing member, and preserving existing pairs.
These guards exercise selection; they are not exhaustive parser fault injection.
The new code does not apply the old diagnostic arbitrary-property/cache edits.

Fresh strict baseline: Q12 FAIL/17. Fresh accepted Q15: FAIL/3. Both finish and
leave original/copy hashes unchanged. An early rebuild accidentally omitted the
inherited `PSTOFF_HEADER` flag, producing an extra rgbFP diagnostic; it was
corrected before the clean trial series. That artifact is retained locally and
excluded from conclusions about the three rows.

## Hypotheses and causal results

The ranked predictions were: wrong variable-cell HNID/ownership; incorrect
column offsets/order/CEB interpretation; block-size translation damage; then
interacting source-cache/PC differences. Each representation trial changes a
single structural mechanism while retaining every decoded row and property.
The combined trial explicitly checks interaction between layout and ownership.

1. Heap repack with aligned HNPAGEMAP: FAIL/3.
2. Fresh, distinct allocation for every variable cell: FAIL/3.
3. Reassign CEB indices, keeping required row-ID/version bits: FAIL/3.
4. Rebuild offsets by size group/tag and reassign CEB indices: FAIL/3.
5. Combine canonical layout with fresh variable-cell allocations: FAIL/3.
6. Clear only storage for absent cells and unused CEB bits: FAIL/3.
7. Move the row matrix to a new HID, updating TCINFO: FAIL/3.
8. Move the row-index leaf to a new HID, updating its BTH: FAIL/3.
9. Move all variable cells to a second heap page: FAIL/21, including three
   invalid-XBLOCK-total findings. This introduces a separate defect and is
   excluded as evidence about the original mismatches.
10. Pad the non-final heap page to 8,176 bytes in the second-page trial:
    FAIL/3. All original cell/property bytes and all 67 decoded rows remain exact.

The initial exploratory bitmap trial also moved the mandatory row-ID/version
bits and triggered schema failures. The final trials preserve their required
positions; those exploratory schema failures are not evidence for a cause.
All earlier exploratory outputs are retained locally, never overwritten.

**A separate writer defect is established:** the writer emits a short non-final
heap block into an XBLOCK and sums its short length into `cbTotal`. SCANPST rejects
that layout. Padding the non-final page after its allocation map restores the
expected XBLOCK span without changing any allocation value and removes all new
findings. This is a synthetic writer regression seam, demonstrated by trials 9
and 10; it does **not** explain the original three failures. All six affected
source item/table heaps, including the search table, are single blocks below 3 KiB;
none of the 294 top-level source heaps has a block over 8,176 bytes. No general
heap relocator or production padding fix is claimed: the padding is isolated to
this representation experiment.

**Correction to inherited evidence:** the contents-cache `3013` is not an
8-byte BTH header in all three rows. `200084` and `200184` reference HID `20`,
the row-index BTH header. `2000E4` references HID `80`, a 16-byte allocation also
used by its message-class cell. Its length does not establish a valid
conversation ID. The `200184` search-table occurrence instead uses a separate
16-byte allocation. The source reference/length evidence is recorded without
publishing any value bytes. These aliases are already in the public OST;
the previous ownership fix clones their bytes into independent allocations.
Reallocating them again still leaves FAIL/3. This does not establish that any
cache rewrite is sufficient or authorized.

## Preservation and strict gate

The accepted candidate retains 308 NIDs and parents, all 136 byte-identical
subnode streams, and all 67 decoded table rows. Against accepted Q12, all 1,516
existing decoded property values are exact, none is removed, and exactly the 34
Q15 properties are added. The secondary Microsoft reader opens it with Valid2
maps, reads the root hierarchy and opens the IPM subtree. This is not independent
complete table/payload equivalence.

Every representation trial has the same decoded PCs and rows as the accepted
candidate and passes the graph/subnode/row audit. The synthetic XBLOCK-invalid
trial still fails Microsoft's strict scanner despite those same-writer checks;
that distinction is retained explicitly. No row is dropped or diagnostic hidden.
Maps are regenerated using the Microsoft transaction flush, never merely marked
valid. Every strict scan uses `-silent -no repair`, finishes for candidates,
and verifies unchanged original and Windows-copy hashes. The valid Empty
control is PASS/0; the invalid zero control is FAIL with an incomplete scan.
Raw SCANPST process exit codes are not the gate verdict.

No gate passes, so a fresh independent libpff comparison is not claimed. The
prior independent comparison is unchanged, not relabelled as testing this
candidate. Full payload/table preservation and classic Outlook acceptance remain
unproved.

## Decision and handoff

No new property policy is applied or requested on the basis of these negative
trials. They do not justify rewriting cached sender/conversation/provider state.
If a future isolated fix requires changing meaningful cached values, the exact
field, value source, conflict handling and preservation tradeoff still need a
separate decision. Recommend retaining this FAIL/3 artifact and investigating
which values SCANPST actually compares before authorizing any broad cache
reconstruction. The unrelated short-non-final-heap-block defect is a concrete
writer follow-up, with a red/negative regression and a representation-preserving
control, but fixing it alone will not clear this fixture.

The bounded loop stops here rather than publishing another unsupported cause.
No PR was opened and no GitHub issue was commented on or closed.

## Replay

The original `/tmp` clones were absent; `bootstrap.py` fetches each pinned public
repository into its own new temporary directory. If the local sources config
already exists, reuse it instead of running bootstrap again.

```sh
python3 spikes/issue-20/bootstrap.py        # only for a fresh source setup
python3 spikes/issue-20/prepare.py
python3 spikes/issue-20/build.py            # offline bwrap build
python3 spikes/issue-20/reproduce.py        # exit 1: strict FAIL/3
python3 spikes/issue-20/trials.py
python3 spikes/issue-20/trials.py move-row-matrix move-row-index \
  cells-next-page cells-next-page-padded
python3 spikes/issue-20/collect.py
python3 spikes/issue-20/scan-loop-strict.py \
  /home/watermelon/.local/share/pstoff-spike20/output/q15-validated-public.pst
# exit 1: finished, unchanged original/copy, exactly three diagnostics
```

Writers refuse existing output names. Replays need fresh labels/root contents;
never delete or overwrite earlier outputs. The trial runner defaults to the final validated candidate. Use `--prefix`
and `--source` for separately labelled reruns; `reproduce.py` accepts a new
output label as its positional argument. The strict detector/PowerShell logic is identical to issue-18, with
only the input allowlist extended for the new output/control paths. The literal
issue-18 script rejects the issue-20 directory; it is left untouched as requested.
`collect.py` exits zero for verified experiment expectations, **not** a clean PST.
Only strict gate exit zero means finished + unchanged + zero diagnostics.
