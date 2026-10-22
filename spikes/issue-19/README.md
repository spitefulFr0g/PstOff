# Bounded post-Q12 mismatch diagnosis (PUBLIC only)

**Policy block, partial diagnosis.** Q12 remains the only accepted item-property
reconstruction. Its pinned output still fails the strict SCANPST gate with 17
mismatching rows. Fourteen associated-item failures are causally isolated to the
cached change-key/predecessor-list pair absent from the item PC. The exact cause
of the remaining three normal-item failures is **not established**. There is no
zero-diagnostic output, accepted additional converter behavior, or Faithful claim.

The issue-17 and issue-18 directories are imported unchanged from `5b3eceb`.
This directory uses a separate prepared writer, build target and output root at
`/home/watermelon/.local/share/pstoff-spike19`. Pinned clones, previous prepared
sources/outputs, root prototype and glossary remain untouched. No private fixture
was read or enumerated, and there was no delegation, other model/provider, GitHub
issue/PR operation, Outlook COM, or SCANPST repair.

## Feedback loop and minimized causal result

The first command, before forming hypotheses, reproduced the exact red:

```sh
python3 spikes/issue-18/scan-loop-strict.py \
  /home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst
# exit 1; finished; unchanged original/copy; 17 diagnostics
```

Ranked predictions were: (1) missing cached change-history fields, (2) missing
message/local state fields, (3) differing sender name, (4) structural row/schema
interpretation. Counts alone were not treated as evidence of causation.

The minimized intervention is **one associated item, NID `100028`**. Clearing
only its `65e2` table presence bit leaves the original 17 mismatch rows; clearing
only `65e3` does the same. Clearing both bits changes exactly two CEB bits and
removes exactly that item from the log: 16 mismatches, the other 16 row IDs
unchanged. These are diagnostic cache changes, not proposed output behavior.

A second minimized direction preserves all decoded table rows: inserting both
properties into only `100028` removes exactly that row (16); inserting either
property alone retains the exact original 17 mismatch rows. At full-fixture scale: insert the exact cached
`65e20102` and `65e30102` bytes into otherwise missing PCs. Doing so for all 17
items removes all 14 associated-item mismatch lines, leaving precisely normal
NIDs `200084`, `2000E4`, `200184`. Existing item values are preserved. This is an
experimental item-property policy, **not an accepted technical fix**.

Both properties are absent in all 17 source PCs. Cache observations agree for
same-NID occurrences. The cached keys have valid XID lengths (two 17-byte keys,
fifteen 20-byte keys). Every serialized predecessor list parses completely into
17–24-byte XIDs and contains the paired key: sixteen have one entry and one has
two. This establishes serialized shape and local consistency, not authentic
server lineage or complete history.

## Exact decision and recommendation

**Decision for the parent/user:** may conversion copy `65e20102` (ChangeKey) and
`65e30102` (PredecessorChangeList) from unambiguous same-NID source table caches
into missing item-PC properties, recording each property’s source/provenance?

Recommend a separate, fixture-gated policy permitting that pair only after
validating relevant tables, XID/PCL serialization, pair consistency and agreement
across every occurrence. Preserve any existing property, including conflicting or
differently typed properties; report missing, invalid or conflicting observations
as unresolved. Retain table state and existing item properties. Do not regenerate
history, copy arbitrary cached properties, or treat a successful repair as output.
This recommendation needs authorization beyond Q12 and will **not** resolve or
waive the three still-unexplained normal-item failures.

[Microsoft defines ChangeKey](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxprops/e9ad95c1-83b7-4ee8-9d72-60f8faffc0d2)
as identifying the object's last change. Its
[change-number maintenance rules](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxcfxics/b03168dc-6ad4-4747-b1b5-c95fd436dfce)
connect the key and predecessor list to replica history. Their meaningful state
is why copying or deleting them requires an explicit policy decision.

## Unresolved normal rows and limits of the negative trials

The three normal rows remain after both cache fields are cleared or both missing
PC properties are experimentally populated. Sender-name `0042` differs from the
PC in all three; replacing those cached strings with exact existing PC strings
does not clear the failures. The original contents-cache `3013` values in these
three rows are malformed eight-byte BTH-header data. One item's search-table
occurrence instead contains a valid computed 16-byte ID, so blindly promoting
that field fails same-NID agreement. The diagnostic planner refuses that conflict.

The source PCs have valid conversation indexes and tracking enabled. Rebuilding
those three cached conversation IDs according to Microsoft's
[Conversation ID algorithm](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/a19c8e83-bb3b-4061-b027-aa2e82061283)
(offset 6, length 16), alone or together with sender-name alignment, still leaves
three failures. These discrepancies are observations; neither is established as
a sufficient cause. The cache data was preserved in accepted output.

Other targeted negative trials included cached status `0e17`, local `6612`,
`6628`, pending flag bits, and absent calculated/default cells. None provided a
zero-diagnostic candidate. Microsoft explicitly specifies
[table-derived message status](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/f37c529b-0cd0-4ffd-83ee-c286636de41a),
so absent PC values alone do not establish damage. Type comparisons found no
PC-versus-present-TC type mismatch on message rows. Early column-identity masking
and broad schema-removal trials produced extra diagnostics and cannot establish
causes; their disposable outputs stay local. The label `causal-zero` on one local
trial is a hypothesis label: its actual gate is FAIL/3, never zero.

This bounded follow-up stops at the demonstrated change-history policy block;
it does **not** claim to have diagnosed all 17 failures. Further technical work on
the three normal rows, independent libpff verification and Outlook acceptance
remain separate work for the parent. Mailbox completeness remains unknown.

## Preservation, artifacts, replay

Accepted Q12 output (unchanged):
`/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst`, SHA256
`dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177`.

Experimental pair-promotion output (FAIL/3, not accepted):
`/home/watermelon/.local/share/pstoff-spike19/output/promote-sync.pst`, SHA256
`a4d8070583525c1332dfe111b29a6ec59b45bad1c45fd8fcb60f905ddd5b310d`.
It retains 308 NIDs/parents, all 136 byte-exact subnode streams and all 67 decoded
table rows. All 1,516 existing decoded properties remain exact; exactly 34 item
properties are added, only the change-key/PCL pair. No existing property is
removed or changes. Microsoft's reader opens the diagnostic file with Valid2
maps. These are same-candidate preservation checks and independent opening,
not independent exhaustive semantic validation.

Prerequisites and licensing are inherited from [PROVENANCE.md](PROVENANCE.md).
The baseline patch is unchanged Q12; `diagnostic-only.patch` is explicitly
throwaway code enabled only through diagnostic CLI arguments. It is not wired
into application policy. Rustfmt remains unavailable.

```sh
python3 spikes/issue-19/prepare.py
/home/watermelon/.cargo/bin/cargo build --offline --release \
  --manifest-path spikes/issue-19/probe/Cargo.toml \
  --target-dir /home/watermelon/.local/share/pstoff-spike19/target
python3 spikes/issue-19/reproduce.py --prefix bounded-replay
# checks causal trial expectations + preservation + controls; strict gate remains FAIL
python3 spikes/issue-19/scan-loop-strict.py \
  /home/watermelon/.local/share/pstoff-spike19/output/bounded-replay-promote-sync.pst
# exit 1, FAIL/3; no missing times; scan finished; both files unchanged
```

Writers refuse to overwrite prior candidates: choose new labels for replays.
`targeted.py` exits zero when a diagnostic trial executes; this is **not** the
strict gate. Only `scan-loop-strict.py` exit zero means completed + unchanged +
zero diagnostic lines. `reproduce.py` generates the bounded causal/negative trial set. `collect.py` verifies diagnosis expectations and records
FAIL explicitly. Valid Empty control passes with zero diagnostics/raw exit 1;
invalid zero control fails with an incomplete scan/raw exit 0. All SCANPST calls
use `-silent -no repair`; original and Windows-copy hashes are checked.

Candidate execution uses bwrap with no network/home, cleared environment,
read-only binaries/source fixture, and this public output root writable only for
writers. Raw dumps, history bytes, message values and raw scan logs stay local;
[evidence/results.json](evidence/results.json) contains sanitized aggregate/key
and row-identity evidence. `trials.py` retains rejected exploratory interventions
for reproducibility, clearly diagnostic only.

## Parent verification

A separate strict scan reproduced **FAIL/3**, with a completed scan and unchanged
original/Windows copy. Independent libpff 20180714 reads the same 64 reachable
objects as accepted Q12: all 1,516 existing raw property values match, no value
is changed or removed, and exactly 34 `65e20102`/`65e30102` properties are added.
No reader errors occurred. See
[evidence/libpff-parent-comparison.json](evidence/libpff-parent-comparison.json).
This checks the diagnostic artifact's reachable property graph, not complete
tables/payloads, source-cache authenticity or Outlook acceptance. It does not
authorize the new policy or clear the three remaining failures.
