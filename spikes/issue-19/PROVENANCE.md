# Provenance and licensing

- pstfree: <https://github.com/sp00nznet/pstfree>, commit
  `b6e2ff31592f74458f0481d4671251c92044d8f0`, MIT, copyright 2026
  pstfree contributors. `patches/pstfree-writer.patch` modifies only
  `src/convert.rs` and adds `src/q12.rs`. Apply to a separate copy with `patch -p1`; retain
  [the full MIT notice](patches/LICENSE.pstfree).
- Microsoft outlook-pst-rs: <https://github.com/microsoft/outlook-pst-rs>, commit
  `cfb721daee538acc50d2bad6faac5ce724f6037d`, MIT, copyright Microsoft
  Corporation. Unmodified dependency used to regenerate allocation maps and
  independently open generated PSTs. Full notice: [LICENSE.outlook-pst](probe/LICENSE.outlook-pst).
- Public OST: rjohnsondev/java-libpst `src/test/resources/example-2013.ost`,
  16 MiB, SHA256
  `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
  No fixture binary is redistributed in this branch.
- Public valid control: Microsoft `crates/pst/examples/Empty.pst`, SHA256
  `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Immediate predecessor: issue-17 public corrections at `06e08ac`, imported
  unchanged under `spikes/issue-17`; prior output SHA256
  `485eebf759829911edfadf6c946de58fb02e2fa03ba300a0d56786751fc482ee`.
  Q12 changes are separately scoped under `spikes/issue-18`.
- Starting harness/probes: PstOff issue-16 spike, branch `t3/pst-writer-spike`,
  commit `351c8de`. Baseline reconstructed here has exact SHA256
  `913c8b653ebae62ec19f1268e1c0cdffe91223d0596b57739e3beab68e05a84e`.
  This branch does not depend on an editable prior worktree to replay.
- New PstOff code: public-only runner, pinned-source preparation, controlled
  Q12 writer policy guards, output audit, strict scan gate and sanitized evidence. Probe paths
  explicitly select the dedicated editable pstfree source copy, keeping originals
  and prior baseline artifacts untouched. Generated files live under
  `~/.local/share/pstoff-spike18/output`.

No API-billed models, delegated workers, private fixtures, repaired outputs,
Outlook automation, or third-party runtime outside the isolated public harness
were used. Build dependencies were reused offline with rustc 1.99.1; rustfmt was not installed.


## Post-Q12 diagnosis lineage

The material above records the inherited Q12 experiment. This follow-up imports
issue-17/18 unchanged from `5b3eceb`, then applies `diagnostic-only.patch` after
the accepted baseline patch in a fresh `pstoff-spike19/source/pstfree` copy.
The diagnostic patch modifies `src/convert.rs` and adds `src/q19.rs`; retain the
same MIT notice. Runtime/output and build roots are `pstoff-spike19/output` and
`pstoff-spike19/target`. No root application behavior is changed. Diagnostic
property insertions, cache edits and schema trials are not accepted policy.
