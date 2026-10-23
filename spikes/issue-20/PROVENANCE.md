# Provenance and licensing

All inputs are public. No fixture binary is redistributed.

- pstfree: <https://github.com/sp00nznet/pstfree>, pinned commit
  `b6e2ff31592f74458f0481d4671251c92044d8f0`, MIT. The inherited
  `patches/pstfree-writer.patch` adds Q12 and the previous writer corrections.
  Full notice: [LICENSE.pstfree](patches/LICENSE.pstfree).
- Microsoft outlook-pst-rs: <https://github.com/microsoft/outlook-pst-rs>,
  `cfb721daee538acc50d2bad6faac5ce724f6037d`, MIT. Unmodified map regeneration
  and secondary reader. Full notice: [LICENSE.outlook-pst](probe/LICENSE.outlook-pst).
- java-libpst public fixture:
  <https://github.com/rjohnsondev/java-libpst>, pinned repository commit
  `f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26`,
  `src/test/resources/example-2013.ost`, SHA256
  `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
- Microsoft valid Empty control: `crates/pst/examples/Empty.pst`, SHA256
  `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Starting PstOff branch: `origin/t3/diagnose-scanp-mismatches`, `9d704fc`.
  Issue-16 through issue-19 directories are untouched. No dependency on another
  editable PstOff worktree is introduced.

New code is restricted to this spike. `q15.rs` implements accepted pair-copy
policy; `q20.rs` implements explicitly selected representation experiments.
The old issue-19 arbitrary-property diagnostic patch is **not applied**.
The dedicated source/build/output root is
`/home/watermelon/.local/share/pstoff-spike20`.

The old `/tmp` pinned clones were gone. `bootstrap.py` records the replacement
clones, one fresh temporary directory per repository, in the new root's
`sources.json`. Downloads are not executed. Git HTTP requests use only
`PstOff-research (https://github.com/spitefulFr0g/PstOff)` as User-Agent.

No other model/provider or delegated worker was launched. Candidate runtime is
in bwrap with no network/home and explicit read-only public inputs. Initial
offline Cargo builds used the inherited host-side command, including dependency
build scripts; this was a containment deviation. Final rebuilds use `build.py`
to isolate compilation/build scripts as well, without network or home contents.
The compiler toolchain and registry cache are mounted read-only at separate
sandbox paths. SCANPST is the installed Windows Microsoft binary, scan-only.
No private fixture was read, listed, or mounted. No SCANPST repair, Outlook COM,
GitHub issue mutation, PR, or global Git configuration change was performed.
