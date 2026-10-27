# Current work

Checkpoint: 2026-10-27. This is the authoritative current handoff; older issue
notes describe their recorded revisions. Verify live PR state with `gh` and
agent activity from task events.

## Latest delivery

Active issue: [#48](https://github.com/spitefulFr0g/PstOff/issues/48),
Windows Sandbox cargo wrapper for every build and test. Branch:
`chore/issue-48-sandbox-acceptance`, based on `e2d8f57` (`main` after PR #51).

The wrapper (`scripts/sandbox-cargo.py`), `scripts/check` routing, doc updates
and removal of `scripts/cargo-sandboxed` landed in `main` through PR #49
(`92b550d`). Unit tests mock the Sandbox boundary. This delivery records real
host evidence for each acceptance criterion and fixes any defect that evidence
exposes.

Acceptance and proof:

- `python scripts/check` passes with every Cargo step in Windows Sandbox:
  a full check run on this branch.
- A guest probe shows networking off and a source-mapping write failing:
  `python scripts/sandbox-cargo.py --probe` output.
- Two concurrent calls from different worktrees both succeed, serialized:
  two simultaneous wrapper runs (this checkout and a temporary worktree) with
  lock-wait and completion timestamps.
- A Sandbox the wrapper did not start yields the distinct exit (3) and stays
  running: a separately launched Sandbox, wrapper exit code and the Sandbox
  process before and after.
- An unfinished guest is closed after the timeout and reported as exit 2:
  a run with a short `PSTOFF_SANDBOX_TIMEOUT`, exit code and no Sandbox
  process afterwards.
- The worktree is byte-identical after a run: a SHA-256 manifest of every
  worktree file (excluding `.git`) before and after a wrapper run.
- Safety and environment docs name the wrapper and nothing calls
  `scripts/cargo-sandboxed`: grep.

Evidence is recorded in [issue-48-sandbox-acceptance.md](issue-48-sandbox-acceptance.md).

Stopping point: a PR with the evidence (and any fixes) that closes #48.
GUI launch and screenshot smoke runs stay in #37.

## Implementation and validation

Every acceptance criterion is evidenced against the unchanged wrapper at
`e2d8f57`. `python scripts/check --full` passed in Windows Sandbox on the
`e2d8f57` source tree; later commits change only documentation and remove the
stale `scripts/cargo-sandboxed` entry from `.gitattributes`. The real runs
exposed no wrapper defect. Standards and spec reviews found no blocking issue;
their evidence-wording findings are addressed. No blockers remain.

Follow-up outside this delivery: the guest probe cannot distinguish a
disabled network from a probe that failed to run.

Next action: open the PR that closes #48, review it and merge.

## Product baseline

[PR #49](https://github.com/spitefulFr0g/PstOff/pull/49) merged at `3d39f7d`;
issue #32 is closed. [Its evidence](issue-32-progress-cancel.md) establishes the
generated-corpus memory gate. Broader memory contracts, public-fixture and
Faithful acceptance remain open. The tooling PR #51 merged at `e2d8f57`.
