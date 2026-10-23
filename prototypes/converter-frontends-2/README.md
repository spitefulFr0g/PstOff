# PROTOTYPE: converter frontends, round 2 (throwaway)

Question (#13, "What should PstOff look like?"): which overall shape should the
converter's main screens take?

Open `prototype.html` in any browser by double-clicking it. Nothing needs
installing, and all data is fake and kept in memory. Switch designs with the
yellow bar at the bottom, the ← → keys, or `?variant=A|B|C`. The **state**
button shows the current state as JSON.

Each variant covers the same flow: add OSTs, untick folders, choose output and
Split, convert with Cancel, then read the conversion report. Try "Add all
sample files" first. It loads one ready OST (58 GB, over the 50 GB limit
unless Split is on), one locked by Outlook (select Retry), and one in an
unsupported legacy format.

| Variant | Shape | What it bets on |
| --- | --- | --- |
| **A · Guided** | Step rail on the left with one large question per screen and Back / Next. | Most users convert once and want to be walked through it. |
| **B · Workbench** | Dark, dense master-detail: toolbar, queue table, inspector tabs (Folders / Output / Report), activity log and status bar. | The people converting are IT staff working through several OSTs, and want everything visible at once. |
| **C · Flow** | One screen: From → What → To columns. The Convert button turns into the progress bar, and the report slides in as a receipt. | The whole job fits on one screen, so the screen itself can show the conversion. |

What all three share, from decisions already made on the map:

- v1 runs in Copy mode only, so there is no In-place control.
- One PST is made per OST.
- Split is optional and size-based, never cuts an item in half, and names its
  files `name (part i of n).pst`.
- A warning appears when a PST would pass Outlook's 50 GB limit.
- A locked OST asks the user to close Outlook and offers Retry.
- Skipped items are listed with plain-language reasons.
- Every report says, in plain language, that Mailbox completeness can't be
  known. Headers-only items are reported separately.

Assumption to check: Cancel behaviour isn't specified yet. Here, Cancel asks
for confirmation, removes the unfinished PST, keeps finished ones, and marks
queued OSTs "not started".
