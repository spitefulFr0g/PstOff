# Provenance and containment

Fetched origin and created `t3/q17-verify` from `origin/t3/subobject-mismatch`,
commit `b0852d42f5d34c5807c633795b9348db9602392c`. Read issue #17 with comments,
especially round 3 and accepted Q17, and spike-22 README/PROVENANCE before editing.
The task explicitly authorizes this narrow policy; no additional decision is
inferred from scanner acceptance. No additional model/provider or worker was used.

The existing spike-22 pinned public `sources.json` was copied unchanged into
`/home/watermelon/.local/share/pstoff-spike23/`. Prepared sources, build targets,
outputs, local logs and inventories all belong to this new root. Existing source
clones and accepted artifacts were read only. No downloads were needed.

## Pinned public materials

- pstfree `b6e2ff31592f74458f0481d4671251c92044d8f0`, MIT; inherited writer/Q12
  patch, Q15 policy and Q16 rebuild. [License](patches/LICENSE.pstfree).
- Microsoft outlook-pst-rs `cfb721daee538acc50d2bad6faac5ce724f6037d`, MIT;
  unmodified map regeneration/secondary reader. [License](probe/LICENSE.outlook-pst).
- java-libpst public resources `f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26`,
  `example-2013.ost`, SHA256
  `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
- Microsoft public `Empty.pst`, SHA256
  `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Round-3 accepted baseline hash is verified in `collect.py`.
- Independent Debian libpff/pypff 20180714: reused existing extracted reader at
  `/home/watermelon/.local/share/pstoff-independent-reader/root`, with the
  issue-16 inventory method copied from `origin/t3/pst-writer-spike`.
  [Inherited package digest provenance](independent-reader/debian-package-provenance.json).
  No package installation or maintainer script execution occurred in this task.

`prepare.py` checks the pinned writer/Microsoft revisions and tracked source
cleanliness, copies sources and applies inherited patches plus the accepted
policy. It refuses differing pre-existing prepared sources. During development
only the new spike-23 prepared copy was recreated after tightening the empty
string selection guard. Both candidate runs produce the same SHA256.

## Policy basis

The user's accepted Q17, recorded in issue #17's final comment, is the authority
for this implementation. The existing [Q16 provenance](../issue-21/PROVENANCE.md)
records the MS-PST get/default and computed-column basis; the [round-3
provenance](../issue-22/PROVENANCE.md) records the controlled evidence isolating
the pair. No new external specification claim is needed here. The undocumented
`66280014` exception is deliberately restricted to that full tag without a PC
counterpart. Scanner acceptance does not establish its general meaning.

## Execution and evidence

All Cargo builds and build scripts were sandboxed from the first invocation by
`build.py`: bwrap unshares network and other namespaces, clears the environment,
mounts only explicit read-only compiler/registry/source inputs and a dedicated
writable target, and exposes no home contents. `run.py` applies the corresponding
public-input sandbox to conversion, guards, audits and Microsoft map/reader tools.
The libpff runner mounts the independent library root and inventory script
read-only, exposes one read-only public input and no home/network, and asserts
input hashes before/after each inventory.

Windows SCANPST uses only `-silent -no repair`, fresh collision-free aliases and
unfiltered inherited strict matching. Both original and Windows-copy hashes are
verified. Raw logs remain local. Evidence includes scanner completion, raw exit
codes, strict diagnostic counts, unchanged-input booleans and hashes. Controls
are rerun rather than relabelled from an earlier spike. No repaired output is
consumed. The second candidate confirmation also passes with identical bytes.

Every omission is recorded locally with table/item/full tag. Sanitized evidence
contains only aggregate omission/default counts; local detailed provenance and
exact-cell audit are not committed. No message/property values or raw logs are
published. Input hashes and baseline preservation are rechecked by collection.
Libpff comparison explicitly checks expected and unexpected change/addition sets,
missing properties, identical object IDs, and reader errors.

No private fixture was read, listed, touched or mounted. Earlier spike directories
and outputs are unchanged. No global Git setting, GitHub issue or PR was changed.
The requested branch is committed and pushed without opening a PR. Outlook
acceptance and complete independent payload/table equivalence remain unproved.
