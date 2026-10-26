# Faithful checks

A Conversion is **Faithful** (see `GLOSSARY.md`) when all three checks below pass. The rules for the explained differences come from [issue #11](https://github.com/spitefulFr0g/PstOff/issues/11). Run the checks only on public or synthetic files; see `docs/agents/safety.md`.

## 1. SCANPST, scan-only

```bash
python scripts/scanpst-check.py out.pst
```

The script copies the PST to Windows temp and runs `SCANPST.EXE -silent -no repair` on the copy. It prints one JSON line. It exits 0 for **PASS**: the scan finished, the log reports zero diagnostic lines, and neither file changed. It exits 1 for FAIL and 2 when the check couldn't run.

Run the controls whenever the script or SCANPST changes:
- Microsoft's `Empty.pst` must PASS.
- A zero-filled file must FAIL.

## 2. libpff comparison

Run it in WSL against the Windows checkout (`docs/agents/environment.md`):

```bash
libpff="wsl.exe -d Debian -- python3 /mnt/c/Users/spitefulfr0g/dev/PstOff/scripts/libpff-check.py"
MSYS_NO_PATHCONV=1 $libpff compare /mnt/c/<source.ost> /mnt/c/<out.pst>   # exit 0: only explained differences
MSYS_NO_PATHCONV=1 $libpff tree /mnt/c/<out.pst>                         # folder tree with message counts
```

`compare` reads every reachable property from both files with libpff, an independent reader. It fails on any difference outside the allowed list, on a changed object graph, and on any reader error. The allowed list lives in the script, which keeps it in one place; change it there only after a decision on #11.

Coverage is the reachable property graph only. It doesn't cover table contents, attachment payloads, recipients or embedded messages.

The reader is Debian's libpff packages, extracted and never installed, and it always runs under `bwrap`. Its root is `~/.local/share/pstoff-independent-reader/root`; override it with `PSTOFF_LIBPFF_ROOT`. Package hashes are in `provenance.json` beside it.

## 3. Opens in classic Outlook

```bash
python scripts/outlook-check.py out.pst --user-notified [--expect-sender "Microsoft Outlook"]
```

The script drives this machine's classic Outlook through Windows UI Automation, using only the UIAutomationClient assemblies built into Windows. It never uses the Outlook object model, because `AddStoreEx` hangs on every PST ([issue #18](https://github.com/spitefulFr0g/PstOff/issues/18)). It prints one JSON line. It exits 0 for **PASS**, 1 for FAIL and 2 when the check couldn't run.

What it does:
1. Refuses files under `testdata/private/`. Refuses to start if Outlook is already open or the `PstOffValidation` profile is missing.
2. Copies the PST into the check folder `C:\Users\spitefulfr0g\PstOff-outlook-check\` under a unique name. Outlook writes to every file it opens, so it only ever sees the copy, which is deleted afterwards.
3. Starts `outlook.exe /profile PstOffValidation`, the account-free profile from #7, and nothing else. The default `Outlook` profile holds the user's mailbox.
4. Opens the copy via **File → Open & Export → Open Outlook Data File**. Then it reads the folder list and the sender column of every folder's message list, and opens one message.
5. Closes the data file and Outlook.

**PASS** needs the data file to open, the standard folders (`Deleted Items` by default; change with `--expect-folder`), a readable sender in every listed row, and one message that opens with a body. A sender that is empty, holds a replacement or control character, or shows a message class such as `IPM.Note` is a FAIL. That is what the pre-Q16 output showed (#18). `--expect-sender` also requires every row to show that sender.

A hung Outlook is killed after `--timeout` seconds (default 300), and so is one that won't close. Both exit 2, never PASS. The JSON line's `ReducedFunctionality` says whether Outlook showed its unlicensed or reduced-functionality state. This machine's Outlook is unlicensed and had to be activated before 2026-10-26. If it has gone view-only, tell the user the check needs another Windows setup.

The check uses the user's real desktop, mouse and keyboard, so the guardrails in `docs/agents/computer-use.md` apply. Tell the user before a run, then pass `--user-notified`. Without it the script refuses.

Run the controls whenever the script or Outlook changes:
- Microsoft's `Empty.pst` must PASS with the standard folder set.
- The passing #17 output (SHA256 `1a0919f5…bb7b`) must PASS with `--expect-sender "Microsoft Outlook"`.
- The pre-Q16 spike output, with aliased contents-table cells, must FAIL on its garbled sender column.
