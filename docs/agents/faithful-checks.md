# Faithful checks

A Conversion is **Faithful** (see `GLOSSARY.md`) when all three checks below pass. The rules for the explained differences come from [issue #11](https://github.com/spitefulFr0g/PstOff/issues/11). Run the checks only on public or synthetic files; see `docs/agents/safety.md`.

## 1. SCANPST, scan-only

```bash
scripts/scanpst-check.py out.pst
```

The script copies the PST to Windows temp and runs `SCANPST.EXE -silent -no repair` on the copy. It prints one JSON line. It exits 0 for **PASS**: the scan finished, the log reports zero diagnostic lines, and neither file changed. It exits 1 for FAIL and 2 when the check couldn't run.

Run the controls whenever the script or SCANPST changes:
- Microsoft's `Empty.pst` must PASS.
- A zero-filled file must FAIL.

## 2. libpff comparison

```bash
scripts/libpff-check.py compare source.ost out.pst   # exit 0: only explained differences
scripts/libpff-check.py tree out.pst                 # folder tree with message counts
```

`compare` reads every reachable property from both files with libpff, an independent reader. It fails on any difference outside the allowed list, on a changed object graph, and on any reader error. The allowed list lives in the script, which keeps it in one place; change it there only after a decision on #11.

Coverage is the reachable property graph only. It doesn't cover table contents, attachment payloads, recipients or embedded messages.

The reader is Debian's libpff packages, extracted and never installed, and it always runs under `bwrap`. Its root is `~/.local/share/pstoff-independent-reader/root`; override it with `PSTOFF_LIBPFF_ROOT`. Package hashes are in `provenance.json` beside it.

## 3. Opens in classic Outlook (manual)

Outlook automation (`AddStoreEx`) hangs on every PST, so the user does this check by hand ([issue #18](https://github.com/spitefulFr0g/PstOff/issues/18)).

1. Clear the check folder `C:\Users\spitefulfr0g\PstOff-outlook-check\` down to `1-known-good-Empty.pst`. Then copy in the one candidate to check. Every PST opens under the same name in Outlook, so an older candidate left in the folder is easy to mistake for the new one.
2. Hand the user a copy, never the original output, because Outlook writes to every file it opens.
3. Ask the user to open it via **File → Open & Export → Open Outlook Data File**. They then check the folders and the message list, including the sender column, and open one message.
4. Ask them to close the data file afterwards.

This machine's Outlook is unlicensed and had to be activated before 2026-10-26. If it has gone view-only, tell the user the check needs another Windows setup.
