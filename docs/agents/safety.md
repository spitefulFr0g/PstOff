# Safety rules

Hard rules for every agent working in this repo, including delegated ones. They protect the user's private mail and this machine. When a rule blocks the task, stop and ask the user rather than working around it.

## Private mail data

- Real OSTs and PSTs live in the gitignored `testdata/private/`. Work only with public fixtures and generated synthetic files; leave everything under `testdata/private/`, and any file converted from it, to the user. Never read, list, copy or open those files, and never point a tool at them.
- Committed evidence (logs, JSON, READMEs) holds counts, hashes, node IDs and property tags. Names, subjects, addresses and bodies from mail stay out of it, even from public fixtures.

## Third-party code

- Downloaded files are untrusted. Give each download its own new directory under the Windows temp folder (`$TEMP`), keep your own scripts elsewhere, and never execute what you downloaded.
- Build, test and run PstOff only inside **Windows Sandbox**, from the very first build. Crate build scripts and proc macros are third-party code, and so is every binary they produce. The guest has networking and the clipboard off. Source, crate cache, Rust toolchain, MSVC and the Windows SDK are mapped read-only. Only the build output folder is writable. Run `cargo fetch` on the host first: it downloads crates and writes `Cargo.lock` without running crate code. `docs/agents/environment.md` says where the route's parts live.
- Windows runs one Sandbox at a time. Before launching one, check that none is running. A Sandbox you didn't start belongs to the user or another agent, so leave it and ask.
- libpff is the one Linux-only tool. Run it in WSL through `scripts/libpff-check.py`, which wraps it in `bwrap`.

## SCANPST

Scan-only, on a copy: `-silent -no repair`. Run it through `scripts/scanpst-check.py`, which fixes those flags. Repaired output is never used or kept as evidence.

## Network identity

Any HTTP User-Agent is `PstOff-research (https://github.com/spitefulFr0g/PstOff)`. The user's email address never goes into a request.

## Git

- Leave the global git config alone: no `gh auth setup-git`, and the remote stays HTTPS.
- Push with `git -c credential.helper= -c credential.helper='!gh auth git-credential' push`.
- Push your branch after every commit. Only the pull request waits for acceptance. `scripts/unpushed` lists branches with commits no remote has, and a Claude Code Stop hook runs it at the end of each turn.
