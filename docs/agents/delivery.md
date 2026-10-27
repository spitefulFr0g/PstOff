# Delivery

## Start

Read `docs/current-work.md` and the active GitHub issue. Verify the checkout,
branch and PR state with Git and `gh`; task activity comes from actual task
events. Update the current record before implementation with:

- The requested outcome and active issue (or direct user request).
- Checkable acceptance criteria and the planned proof for each.
- The stopping point and follow-ups outside this delivery.

For performance or memory work, reproduce and measure the baseline before
choosing a refactor. Proposed targets become requirements only when the user
or active issue adopts them. Record a justified threshold before the final run.

## Implement and verify

Prefer one issue per PR. Give a delegate the same scope, acceptance criteria,
base commit and safety instructions. Queue Sandbox verification through the
existing host lock; independent implementation can continue while it is busy.

Use `python scripts/check --fast` for development feedback. Before merge, use
the full check, unless its passing proof still covers the unchanged code and
the remaining change has narrower direct proof. Run the memory gate when
conversion memory behavior changes. A fast pass is explicitly partial proof.

New findings become follow-ups unless they prevent the agreed behavior from
working or expose a safety/correctness defect in the delivered path. Keep
release acceptance separate from the active PR's acceptance criteria.

## Review and handoff

Give the reviewer the fixed base/head, active acceptance criteria, observed
proof and deferred scope. Further rounds cover changed code and open findings.
Close only issues whose acceptance criteria are evidenced by this PR.

Update `docs/current-work.md` at milestones and before handoff. Keep current
scope, implementation state, branch/PR, validation revision/commands/results,
blockers and next action there. Link historical handoffs as evidence rather
than copying their status. Check GitHub for live delivery state and task events
for agent activity; a saved document is a dated checkpoint.
