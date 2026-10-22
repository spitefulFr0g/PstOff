# Issue #17 — targeted public writer corrections

Verdict: structural defects are corrected on the pinned public fixture, but the
unrepaired output still **fails** the zero-diagnostic SCANPST gate. No Faithful
claim, Outlook claim, application implementation, PR, or issue closure is made.

## Result

The original red command was run before edits:

```sh
python3 /tmp/pstoff-scan-loop.py ~/.local/share/pstoff-spike16/output/hybrid-map-rebuild.pst
# exit 1, gate FAIL, DiagnosticLineCount 326, input unchanged
```

The replay regenerates that exact baseline hash. Controlled changes, each against
the same public source and Microsoft allocation-map rebuild:

- Baseline: 326 diagnostic lines.
- Contents schema only: 93.
- Header maps only: 325.
- Heap ownership only: 302.
- Schema + header: 92.
- Schema + header + heap ownership: **34**.

The earlier narrow alias probe (structural allocations only) reached 63; allowing
separate allocations for every repeated variable cell reference reached 34.
That exploratory intermediate is not the final patch or replay variant.

Final output:
`~/.local/share/pstoff-spike17/output/schema-header-exclusive.pst`, 229,888 bytes,
SHA256 `485eebf759829911edfadf6c946de58fb02e2fa03ba300a0d56786751fc482ee`.
It is converter output plus allocation-map regeneration, never repaired output.

The final scan has zero missing-column, failed-addref, rgbFP, or invalid-content-count
findings. It has 17 missing modification times and 17 table/object mismatches.
The inherited gate counts the `!!Message` and mismatch lines (34); its regex does
not separately count the 17 `Missing PR_LAST_MODIFICATION_TIME` detail lines.
We did not change that gate or suppress any findings. Gate FAIL remains the result.

## Corrections and preservation

1. Add absent required contents-table columns and resize/repack row layouts while
   preserving existing column descriptors, presence bits, row IDs and cell bytes.
   Six tags are considered: `0E300102`, `0E330014`, `0E340102`, `0E3D0102`,
   `0E3C0102`, `0E380003`. There were 244 missing descriptors across 41 tables.
   Added cells remain **absent**, with unset existence bits. Zero-filled unused
   slots are structural storage defaults, not invented replication identifiers or
   user property values.
2. Initialize deprecated header maps before recomputing checksums. Microsoft's map
   regeneration subsequently populates rgbFM itself; rgbFP stays `FF` throughout.
3. Replace repeated heap-cell references with separate allocations holding exactly
   the same bytes. Six aliases include cells pointing at the table's BTH header
   and cells sharing another cell's allocation. Preserve all existing HIDs and
   allocations, append clones, and update only affected cell references. This
   addresses actual SCANPST addref failures without interpreting suspect bytes as
   new meaningful values.

The output audit is red on the original baseline (244 missing columns, six aliases,
stale rgbFP) and green on the final output. It exercises actual emitted files.
All 308 indexed node IDs and parents remain; 266 streams outside the store and
contents TCs and all 136 recursive subnode streams are byte-identical. All 67
candidate-decoded TC rows match. 1,493 decoded properties match; no property key is
missing. Only the **pre-existing upstream store conversion** changes its display
name/password and adds four routing/mask properties. This follow-up adds no item
properties and removes no opaque state, subtree, folder, or message.

These comparisons use pstfree to traverse/decode both files. They are stronger
than an item-count check but are not complete independent semantic equivalence.
Microsoft's reader opens the final output with Valid2 maps and the IPM subtree;
that is independent opening evidence, not exhaustive property verification.
The source's suspect cached values remain suspect even when their bytes match.

## Product decision left open

All 17 message PCs (three normal, fourteen associated) lack property `30080040`.
Each has a corresponding source table row, keyed by the same message NID, that
contains a cached `30080040` timestamp. SCANPST reports the missing PC timestamp
and a corresponding table/object mismatch for every message. The log does not
prove timestamp promotion alone would eliminate every remaining mismatch.

**Decision:** may conversion promote an existing cached table timestamp into an
otherwise missing item property, with a per-item recorded reason and provenance?
Recommendation: permit only exact, unambiguous source-backed values matched by
NID; preserve existing item properties, and block/report missing or conflicting
cache values. Never use current time, a guessed timestamp, or repaired output.
This would change meaningful item property presence, so it was not implemented.
Dropping associated/provider items or relaxing the Faithful definition is not a
technical correction and is not proposed here. Parent owns this decision and any
final Outlook validation.

## Replay

Prerequisites: original pinned upstream clones/public fixtures, cached Cargo
dependencies, Rust, bubblewrap, WSL/PowerShell, and the parent's installed Office
SCANPST. All paths are the established local spike paths; no private input is read.

```sh
python3 spikes/issue-17/prepare.py
~/.cargo/bin/cargo build --offline --release \
  --manifest-path spikes/issue-17/probe/Cargo.toml \
  --target-dir ~/.local/share/pstoff-spike16/target
python3 spikes/issue-17/reproduce.py
# Structural regressions pass; clean SCANPST gate remains FAIL.
python3 spikes/issue-17/scan-loop.py \
  ~/.local/share/pstoff-spike17/output/schema-header-exclusive.pst
# exit 1, FAIL, 34 diagnostics
```

`prepare.py` verifies revisions and clean pinned sources, then patches a separate
editable copy. It refuses to overwrite differing local edits. `reproduce.py`
regenerates six candidates, checks hashes/zero drops, runs scan-only validation,
checks controls, runs the output audit, and saves aggregate results. It exits zero
when those exploratory expectations hold; that **does not mean** the clean gate
passed. `scan-loop.py` retains the real zero-diagnostic gate and exits nonzero.

Runtime uses `bwrap --unshare-all`, a cleared environment, read-only public input,
no home mount or network, and one writable public-output directory for writers.
Readers mount that output directory read-only. Source hashes are checked before
and after each candidate invocation. Windows validation scans disposable copies
using `-silent -no repair`, verifies unchanged hashes, and never opens Outlook.
Valid Empty.pst: PASS, zero diagnostics despite raw exit 1. Zero-filled negative
control: FAIL, one diagnostic/incomplete scan despite raw exit 0.

Raw PC/table dumps and SCANPST logs remain local. Committed evidence has only
aggregate counts, structural property keys, hashes and sanitized verdicts.

## Limits and primary references

This is a fixture-bounded experimental patch, gated by `PSTOFF_TC`,
`PSTOFF_HEADER`, and `PSTOFF_ALIAS`. It is not a general heap relocator. It rejects
subnode row matrices, padded rows and expansion beyond a PST block. Multi-level
row-index ownership and arbitrary larger fixtures need separate coverage.

[MS-PST HEADER](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/c9876f5a-664b-46a3-9887-ba63f113abf5)
requires deprecated map initialization. [Contents Table Template](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/f58e1ea9-b592-408d-b89e-53fd4cd6024b)
defines required columns and copied item properties. There is a concrete version
objection: the published table calls `0E30` Integer32, while installed SCANPST
16.0.20527.20088 demands `0E300102` (binary). This experiment emits the demanded
absent binary column, records that discrepancy, and makes no blanket
specification/Outlook-version compatibility claim. The allocation-ownership fix
is supported by controlled runtime evidence, not a claim that this page defines
all undocumented OST sharing semantics.

See [PROVENANCE.md](PROVENANCE.md) for revisions, licenses and artifact lineage,
and [evidence/results.json](evidence/results.json) for the measured matrix.

## Parent verification and remaining policy gate

The parent independently reran the scan gate and confirmed FAIL. An additive stricter validator (`validate-public-windows-strict.ps1`, `scan-loop-strict.py`) also counts lines containing `Missing`, which the inherited pattern did not count. It reports **51 diagnostic lines**: the prior 34 lines plus 17 explicit missing-modification-time detail lines. These are repeated lines describing 17 affected messages, not 51 independent defects. The original replay and historical 34-line figures remain intact. The stricter check passes the valid Empty.pst control and fails the invalid zero-filled control; scan input hashes are unchanged.

Independent Debian pypff/libpff (20180714), run offline with a read-only public input and no home mount, enumerates the same 64 property-bearing objects in the corrected output as in the source. All 1,493 unchanged raw properties match. No source property disappears; the same two upstream store properties differ and four are added, with zero reader errors. See `evidence/libpff-comparison.json`. This still excludes unenumerated internal tables and complete embedded/attachment/recipient payload coverage.

The 0E30 compatibility concern has additional first-party evidence: Microsoft's MFCMAPI [generated tag table](https://github.com/microsoft/mfcmapi/blob/2c59cc71512c56393671738690fa1c88db2a4d1f/core/interpret/genTagArray.h#L653) maps PR_REPL_ITEMID/PidTagReplItemid to **0x0E300102**, the binary type required by the installed SCANPST. This supports the experiment's compatibility choice for this target. The conflicting MS-PST template wording is retained as a documented discrepancy; it does not establish compatibility with every Outlook version.

**Open product choice:** permit an explicitly reported, exact copy of an unambiguous source-cached last-modification timestamp into a missing property on the same item. Existing properties must be preserved; absent, invalid or conflicting cache values must be reported instead of guessed. Microsoft documents [PidTagLastModificationTime](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/pidtaglastmodificationtime-canonical-property) as the object's last-modified timestamp, and the [MS-PST contents template](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/f58e1ea9-b592-408d-b89e-53fd4cd6024b) describes its cached table copy. A cache value remains a source observation, not proof of server history. No timestamp promotion has been applied. Even if accepted, its effect on the remaining mismatch findings must be tested.
