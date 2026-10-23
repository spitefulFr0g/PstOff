# Provenance and containment

Started from `origin/t3/table-row-rebuild`, commit
`912a88c737718edc2ec0e3ecce0cbb4c88936aa7`, after `git fetch origin` and creating
`t3/subobject-mismatch`. Read GitHub issue #17 with all comments, especially the
accepted Q16 and round-2 results, and spike-21 README/PROVENANCE before editing.
The user requested this final bounded research round and restricted acceptance to
existing Q12/Q15 PC changes. No additional worker/model/provider was launched.

## Pinned public inputs

The existing `/home/watermelon/.local/share/pstoff-spike21/sources.json` was copied
to the new `/home/watermelon/.local/share/pstoff-spike22/sources.json`. All source
preparation and target/output files belong to the new root. Existing clones and
prior accepted artifacts were only read.

- [pstfree](https://github.com/sp00nznet/pstfree), pinned
  `b6e2ff31592f74458f0481d4671251c92044d8f0`, MIT. Inherited writer/Q12 patch and
  Q15 implementation retained; [license](patches/LICENSE.pstfree).
- [Microsoft outlook-pst-rs](https://github.com/microsoft/outlook-pst-rs), pinned
  `cfb721daee538acc50d2bad6faac5ce724f6037d`, MIT, used unmodified for real map
  regeneration and secondary opening; [license](probe/LICENSE.outlook-pst).
- [java-libpst public resources](https://github.com/rjohnsondev/java-libpst/tree/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/test/resources),
  pinned `f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26`.
  `example-2013.ost` SHA256:
  `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
- Microsoft `crates/pst/examples/Empty.pst`, SHA256:
  `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Accepted Q12/Q15/Q16 baseline:
  `/home/watermelon/.local/share/pstoff-spike21/output/q16-final-public.pst`, SHA256
  `32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765`.

`prepare.py` verifies source/dependency revisions and clean tracked source areas,
then makes a new patched copy. It refuses a differing pre-existing prepared copy.
Probe Cargo dependencies/lockfile and licenses are inherited from spike-21, with
only the new package/source path and the relevant diagnostic probes changed.

## Primary specification basis

- [MS-PST recipient table template](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/bb069b2b-80ad-46d5-b86f-33487d16bf0c):
  current online §2.4.5.3.1 identifies `692`, an empty template with 14 required
  columns. The probe checks their full tags against the template and populated
  recipient tables. The section numbering differs from the request's shorthand.
- [MS-PST attachment table template](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/47c336f7-2d9b-4f22-91c7-5bb422aaebbb):
  current online §2.4.6.1.1 identifies `671` and six required columns.
  [Actual message attachment tables](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/db45c8ae-6d38-4ab7-b444-a5cca3010101)
  carry those columns plus extensions. No populated attachment table or attachment
  object occurs under the three messages in this fixture.
- [MS-OXOMSG DisplayTo](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxomsg/0e3c4948-8439-4260-9532-f4ac3b302840)
  defines primary-recipient names separated by semicolons, or an empty string.
  [Microsoft's MAPI canonical-property reference](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/pidtagdisplayto-canonical-property)
  applies the separator convention to To/CC/BCC. All three messages have exactly
  one To recipient and zero CC/BCC recipients, so separator spacing cannot affect
  their comparison. Probe output contains equality booleans, never names.
- [Saving a message](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/saving-a-message)
  documents updating display fields, attachment flags and message size on save.
  This does not authorize this converter to change existing item properties.
- [MessageSize](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/pidtagmessagesize-canonical-property)
  describes a sum of logical property sizes and an approximate transfer size.
  Summing physical decoded stream lengths includes heap structures and is not an
  exact implementation of that definition. The structural probe's negative
  equality result is therefore not evidence for an authorized size correction.
- [Inherited Q16 derivation basis](../issue-21/PROVENANCE.md) links MS-PST get
  behavior and the 2022-02-15 PDF supporting its computed-zero/display/subject
  rules. This round preserves that implementation and tests its results. It does
  not claim that a scanner's rejection overrides the formal calculated-property
  rules or establishes a universal representation policy.

No authoritative meaning or general reconstruction rule was established for
`66280014`. Its full tag and absent-PC/present-table state are observed facts.
The clearing-pair result is experimental causality on this fixture and scanner,
not a specification requirement. The complementary addition trial copies the
same rebuilt-row bytes and is explicitly outside accepted behavior.

## Evidence and execution

All Cargo builds, including dependency build scripts from the first invocation,
used `build.py` with bwrap, unshared network, cleared environment and no home
contents mounted. Explicit read-only toolchain, registry, prepared source and
public Microsoft source mounts are exposed alongside the dedicated writable target.
All conversion, map-regeneration, structural/PC/table audit and guard binaries use
`run.py` with the same network/home exclusion, a read-only allowlisted public input
and dedicated output mount. Host Python only prepares, orchestrates, hashes and
sanitizes. Rust formatting does not build or execute the candidate.

Installed Windows SCANPST is always invoked with `-silent -no repair`; original
and fresh Windows-copy hashes are checked. Collision-free aliases retain earlier
scan files/logs. No repaired output is consumed. The strict diagnostic matcher is
unchanged; a PASS requires a finished log, zero diagnostic lines and unchanged
inputs. `collect.py` reads the raw local logs only to extract structural row IDs,
not to suppress diagnostics. Evidence publishes counts, tags, identities and
artifact hashes without raw message/property values. Controls are retained locally
and hash-checked when reused during evidence collection.

No network download was required: pinned public clones were already available.
No downloaded file was executed outside the required build sandbox. No private
fixture was read, listed, touched or mounted. Earlier spike/output directories,
global Git configuration and GitHub issues/PRs were not changed. The independent
offline libpff run was conditional on an accepted/value-preserving PASS and was
not run. The minimal passing diagnostic is not represented as accepted output.
