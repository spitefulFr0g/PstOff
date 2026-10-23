# Safety rules

Hard rules for every agent working in this repo, including delegated ones. They protect the user's private mail and this machine. When a rule blocks the task, stop and ask the user rather than working around it.

## Private mail data

- Real OSTs and PSTs live in the gitignored `testdata/private/`. Work only with public fixtures and generated synthetic files; leave everything under `testdata/private/`, and any file converted from it, to the user. Never read, list, copy or open those files, and never point a tool at them.
- Committed evidence (logs, JSON, READMEs) holds counts, hashes, node IDs and property tags. Names, subjects, addresses and bodies from mail stay out of it, even from public fixtures.

## Third-party code

- Downloaded files are untrusted. Give each download its own new directory under `/tmp`, keep your own scripts elsewhere, and never execute what you downloaded.
- Build and test with `scripts/cargo-sandboxed` (no network, no home), from the very first build. Crate build scripts and proc macros are third-party code. The repo is mounted read-only, so run `cargo fetch` first, outside the sandbox: it downloads crates and writes `Cargo.lock` without running crate code.
- Run any other third-party program (libpff, spike binaries) under `bwrap` the same way; `scripts/libpff-check.py` shows the pattern.
- Windows-hosted builds have no sandbox yet. Ask the user before running one.

## SCANPST

Scan-only, on a copy: `-silent -no repair`. Run it through `scripts/scanpst-check.py`, which fixes those flags. Repaired output is never used or kept as evidence.

## Network identity

Any HTTP User-Agent is `PstOff-research (https://github.com/spitefulFr0g/PstOff)`. The user's email address never goes into a request.

## Git

- Leave the global git config alone: no `gh auth setup-git`, and the remote stays HTTPS.
- Push with `git -c credential.helper= -c credential.helper='!gh auth git-credential' push`.
