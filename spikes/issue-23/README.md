# Accepted Q17 summary-row presence policy

**Strict gate: PASS/0.** Default Q12/Q15/Q16/Q17 conversion of the pinned public
OST passes without diagnostics or repairs. A fresh confirmation produces identical
bytes and also passes. Valid Empty is PASS/0; invalid zero is FAIL/1. Every scan
finishes and leaves both its original and fresh Windows copy unchanged. SCANPST's
raw exit code is 1 for the passing files; the inherited strict diagnostic detector,
finished-log requirement and hash checks determine acceptance.

Accepted output:
`/home/watermelon/.local/share/pstoff-spike23/output/q17-public.pst`

SHA256: `1a0919f5baaeeecea6a20b7190a6373109bfaebc7db1bbbcf0cf32af5697bb7b`

## Policy and selection guards

Q17 is applied by default within the inherited Q16 rebuild, across contents,
associated-contents and search-contents tables (NID types 14/15/16). The runner
selects only accepted `all` mode; the rebuild rejects diagnostic modes.

- A computed default does not make a source-absent cell present. This includes
  computed zero SecureSubmitFlags/ContentFilterSpamConfidenceLevel, empty display
  fields and computed false HasAttach. Present cells may still be verified or
  overwritten by Q16. Non-default derivations retain Q16 behavior. An empty
  display default means an empty byte vector, not an arbitrary zero-filled string.
- A present `66280014` loses presence only if the same item's post-Q12/Q15 PC
  lacks that exact full tag. With a PC counterpart, ordinary PC-backed Q16
  rebuilding applies. An already absent cell is not logged as an omission.
- Every other unsupported table-only cell is retained, including `0E170003`.
  No PC policy is added. Required row identity fields retain Q16 derivations.

The local `q17-public-provenance.json` records each omission's table, item NID,
full tag and action. Published evidence contains only aggregate Q17 counts.
`q17-guards` passes 25 assertions covering absent/present computed defaults,
non-default derivations, exact PC-backed zeros, 6628 absent/present/PC-backed,
other table-only cells, wrong property type, and empty versus zero-filled display.
Inherited Q12/Q15/Q16 guard suites also pass.

## Preservation against round 3's accepted output

Baseline is spike-22 `accepted-replay.pst`, SHA256
`32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765`.

All 308 nodes and parent links, 136 subnode streams, 17 item PC streams,
64 decoded PCs / 1,550 properties and 67 row identities are preserved. Item PC
and subnode streams are byte-identical. The structural audit also preserves
156 other non-contents/non-store streams and passes required-column,
independent-cell-allocation and header-map checks. Microsoft's secondary reader
opens the candidate.

Exactly ten cell presences differ, with no changed existing values or added cells:

| Full tag | Count | Reason |
| --- | ---: | --- |
| `0E03001F` | 4 | Q16 had added absent empty DisplayCc defaults |
| `65C60003` | 3 | Q16 had added absent computed zero defaults |
| `66280014` | 3 | PC lacks exact cached tag; accepted Q17 omission |

This leaves 63 completely unchanged rows and 804 identical existing cells.
The local `exact-cell-preservation.jsonl` identifies every affected table and row;
the local provenance gives full tags. The extra four DisplayCc absences explain
why this accepted artifact differs from round 3's six-bit diagnostic: Q17's
source-absence rule applies to all computed defaults. No other field is dropped;
message status is preserved. Stored bytes of absent cells are not claimed as
semantic values. The source OST remains unchanged.

## Independent offline libpff

The inherited issue-16/17 pypff method reads 64 identical reachable object IDs on
both sides, matching 1,493 unchanged raw values. Expected differences only:
store `3001001F` and `67FF0003` change; store `35DF0003`, `35E00102`, `35E30102`,
`35E70102` are added; 17 each of accepted Q12 `30080040` and Q15 `65E20102` /
`65E30102` are added. **No missing properties, unexpected differences, or reader
errors.** See [aggregate comparison](evidence/libpff-comparison.json).

Coverage is the reachable property-bearing graph, not exhaustive tables,
recipient/attachment/embedded payloads, broader fixtures or Outlook acceptance.
Same-writer preservation and Microsoft's secondary opening do not replace those
remaining validations.

## Replay

Reuse the pinned public `sources.json` in the new output root, then:

```sh
python3 spikes/issue-23/prepare.py
python3 spikes/issue-23/build.py
python3 spikes/issue-23/reproduce.py q17-public
python3 spikes/issue-23/reproduce.py q17-confirm
python3 spikes/issue-23/independent-reader/compare.py
python3 spikes/issue-23/collect.py
```

Candidate labels must be fresh; reproduction refuses existing outputs.
`collect.py` verifies these completed names and reruns both controls. Every Cargo
build, including build scripts from the first build, and every candidate/probe
run uses bwrap without network or home mounts. The independent reader also runs
offline with read-only public inputs. The issue-22 strict scanner and diagnostic
matcher are inherited unchanged, with the path allowlist extended for issue-23.
Only `-silent -no repair` scans are used. No diagnostic trials were needed.

Only this spike's files and new output root are written. Earlier artifacts,
private fixtures, global Git config, issues and PRs are untouched. See
[provenance](PROVENANCE.md) and [sanitized evidence](evidence/results.json).
