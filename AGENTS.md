# PstOff

## Agent skills

### Issue tracker

Issues live in GitHub Issues on `spitefulFr0g/PstOff`, managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default triage labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Delivery

Before implementation, delegation, review or handoff, read `docs/agents/delivery.md` and `docs/current-work.md` for scope, acceptance evidence and the next action.

## Machine

Windows host: develop only in this checkout. Before a build, a check, a WSL command or a Windows Sandbox run, read `docs/agents/environment.md`.

## Computer use

To drive a Windows app (Outlook, the PstOff GUI), read `docs/agents/computer-use.md`.

## Safety

**Hard rules** for private mail data, third-party code, SCANPST and pushing are in `docs/agents/safety.md`. Read it before building, running SCANPST or opening any OST or PST, and pass it on to every delegated agent.

## Faithful checks

To check whether a converted PST is **Faithful** (SCANPST, the libpff comparison and the manual Outlook check), see `docs/agents/faithful-checks.md`.
