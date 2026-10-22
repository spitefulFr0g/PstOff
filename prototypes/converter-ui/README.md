# Converter layout study (throwaway)

Question: which desktop layout makes adding OSTs, choosing folders and output,
following progress, and understanding the Conversion report easiest?

Three structurally different variants share one HTML file and synthetic data:

- `?variant=A`: **Workspace** — persistent queue, folder tree, output inspector.
- `?variant=B`: **Guided** — one decision at a time, with a compact queue.
- `?variant=C`: **Batch** — file table first, with expandable folder selection.

Run `python3 -m http.server 8080 --directory prototypes/converter-ui`, then open
`http://localhost:8080/prototype.html?variant=A`. No packages or remote assets.
The arrow switcher and keyboard left/right change variants. Setup, Progress,
and Report switch between simulated screens. No file is read or written.

## Design plan

Windows utility for IT staff and power users; keep source, selected scope,
destination, and any omitted content visible. Segoe UI for all app text,
12/14/20 px hierarchy. Use native-sized controls and folder rows rather than
a marketing-page treatment. Three layouts test different priorities:

```
A  Queue | Folder tree | Output     Persistent context, fewer navigation steps
B  Queue / Step navigation         Less density, one main choice at a time
          Current step
C  Queue table                     Multiple files and exceptions first
          Expanded file details
```

Base palette: paper `#ffffff`, canvas `#f3f6fa`, ink `#10233b`, muted
`#536579`, Windows blue `#0f6cbd`, warning `#865400`. Variant B uses more
space and a stronger step heading; C uses a compact navy command strip and
table rows. A keeps familiar Windows utility proportions. All align text
left. No decorative gradients or mail content previews.

Brief review: a generic dashboard would obscure the job behind metrics and
cards. These layouts instead devote their main area to the folder tree,
the current choice, or the queue table. Completeness language appears before
conversion and in the report. Copy mode is the sole v1 mode; no selector for
In-place mode is offered. Split starts at the accepted 20 GiB limit.

## Boundaries

This is a visual discussion artifact, not GPUI implementation or evidence of
Windows accessibility/build compatibility. Counts, timing, progress, output
paths, issue examples, and cancellation behavior are synthetic scenarios.
No output-publication or crash-safety policy is settled by this prototype.
No variant is accepted yet. No production application should ship this file.
