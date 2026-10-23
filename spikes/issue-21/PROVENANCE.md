# Public inputs, implementation and containment

The starting PstOff branch is `origin/t3/normal-row-mismatches`, commit
`24019275a89f7521fbd3c4b68180e4240e8a21da`. Before implementation the issue-17
comments and issue-20 README/PROVENANCE were read. Q16 is the accepted policy
recorded in that issue. New work is confined to `spikes/issue-21/`.

## Pinned source inputs

- [pstfree](https://github.com/sp00nznet/pstfree),
  `b6e2ff31592f74458f0481d4671251c92044d8f0`, MIT.
  The inherited Q12/writer patch and accepted `q15.rs` are retained with
  [the full notice](patches/LICENSE.pstfree). `q21.rs` adds the row-only seam.
- [Microsoft outlook-pst-rs](https://github.com/microsoft/outlook-pst-rs),
  `cfb721daee538acc50d2bad6faac5ce724f6037d`, MIT. Unmodified dependency for map
  regeneration and secondary opening, with [the full notice](probe/LICENSE.outlook-pst).
- [java-libpst public fixtures](https://github.com/rjohnsondev/java-libpst/tree/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/test/resources),
  commit `f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26`.
  Source `example-2013.ost`: SHA256
  `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
  Limited donor `dist-list.pst`: SHA256
  `c86841da106036b5abe5a2141dc7644cbb2bf8b504873515eb35a2efeb8c28ac`.
  Its descriptor-only local extraction has SHA256
  `16e137b25c3ebded5946664fa160b584ebda67d60cb0ff12785fae2c8bb2bc8b`.
- Microsoft `crates/pst/examples/Empty.pst`: SHA256
  `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Accepted prior Q12/Q15 candidate, read-only comparison baseline:
  `/home/watermelon/.local/share/pstoff-spike20/output/q15-validated-public.pst`,
  SHA256 `e65984caded46d3c19603c45e19c54a82efc9b469f05796a2ba866e481d9f906`.

The existing issue-20 `sources.json` and clean pinned clones were reused. Separate
prepared copies under `/home/watermelon/.local/share/pstoff-spike21/source/` retain
intermediate versions; final replay uses `pstfree-final`. No earlier spike's
prepared source or output was changed. Local paths are dependencies only for the
pinned inputs, not another editable worktree.

## Primary specification basis

- [MS-PST contents template, §2.4.4.5.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/f58e1ea9-b592-408d-b89e-53fd4cd6024b)
  identifies the required columns and copying from message PCs.
- [MS-PST search contents template, §2.4.8.6.2.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/cdcf9571-049f-47f5-b075-8374057134ec)
  identifies row/parent NID columns. Row identity also follows the TCROWID mapping.
- [MS-PST get behaviors, §2.5.3.1](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/f37c529b-0cd0-4ffd-83ee-c286636de41a)
  specifies PC retrieval, display defaults, HasAttachments, table-derived status
  and replication fields, zero SecureSubmitFlags/SCL and computed Conversation ID.
- [MS-PST Conversation ID, §5.4](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/a19c8e83-bb3b-4061-b027-aa2e82061283)
  supports extracting bytes 6–21 when tracking is true and the index is valid.
  The unsupported topic-hash fallback preserves/reports the cache instead.
- [Microsoft's 2022-02-15 MS-PST PDF](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-PST/%5BMS-PST%5D-220215.pdf),
  §2.5.2.3 and §2.5.3.1.1 (printed pages 101–104, 110–111), defines calculated
  message properties and the two subject metadata code units. The plain subject
  cases in this fixture do not change. The PDF was downloaded to its own new
  temporary directory and read with the web PDF reader; unavailable host
  `pdftotext` was not installed. No downloaded file was executed.

These rules authorize specific table derivations, not arbitrary PC defaults or
changes to item properties. Unknown columns remain cached and reported.

## Supplemental rejected donor

[libyal/testdata Outlook PST](https://github.com/libyal/testdata/blob/f018eb7c91c8356123b021ecd28ae894dd500210/pst/outlook.pst),
commit `f018eb7c91c8356123b021ecd28ae894dd500210`, SHA256
`df01707f76d0e24ab913cf1ffeffa6eaf9c1d590e02102c7ed26bb3ac51d4e24`.
It was downloaded as data into its own fresh `/tmp/pstoff21-libyal-*` directory,
then copied to a fresh issue-21 output name for a scan. It is FAIL/408 and was
not used as a known-good donor. No binary or content dump is redistributed.
Only descriptor ordering from the local java-libpst donor was used, with the
limits documented in the README.

All download HTTP User-Agents were exactly
`PstOff-research (https://github.com/spitefulFr0g/PstOff)`.

## Runtime and evidence

All Cargo builds, from the very first invocation and including dependency build
scripts, ran with `build.py` in bwrap with network/home excluded. The toolchain
and registry cache were explicitly read-only mounts at sandbox paths. All candidate,
map-regeneration, extraction, audit and guard executions used `run.py` with the
same network/home exclusion and read-only public inputs. Host Python only
orchestrates, hashes, prepares copies and sanitizes reports. No extra model,
provider or worker was launched, and no API-billed model was requested.

The installed Windows SCANPST always receives `-silent -no repair`; its original
and Windows scan-copy hashes are checked. Every scan gets fresh paths, preventing
replacement of old copies/logs. No repaired output is read or used. Maps are
regenerated, never merely marked valid. Candidate and controls are scanned with
the same unchanged strict diagnostic detector. Raw SCANPST logs, PC dumps and
per-row provenance are kept outside Git. Evidence contains counts, property tags,
artifact hashes and scanner status, without source content values.

No private fixture was read, enumerated or mounted. Earlier output artifacts were
neither overwritten nor deleted. No global Git configuration, GitHub issue or PR
was modified. Independent libpff was conditional on a PASS and was not run here.
