# Synthetic v36 OST fixtures (#23)

`crates/engine/tests/support/synthetic.rs` is a test-only description-to-bytes
builder. Import it from an integration test using `#[path = "support/synthetic.rs"]`.
It never opens an input file. Output depends only on the description: no clock,
randomness, machine identity or real mail data.

Describe a `Store` with a root `Folder`, nested folders and `Item::new(class, size)`.
Size is the requested `PidTagMessageSize`, not physical allocation size. It lets
later Split tests describe large Items without allocating gigabytes. The builder
accepts all six Item classes, typed property bytes, recipients, by-value and
embedded attachments, external references, a named `HeaderItem` marker, and
arbitrary cached contents-table cells (including Q12/Q15). HeaderItem uses
`PSETID_Common`, numeric LID `0x8578`, and `PT_LONG`, as in
[Microsoft's headers-only example](https://learn.microsoft.com/en-us/office/client-developer/outlook/mapi/how-to-determine-if-outlook-downloaded-only-the-header-of-a-message).

The format has v36 headers, 4096-byte NBT/BBT pages, 512-byte aligned blocks,
independent bitwise CRCs, signatures, sorted indexes and scoped subnode trees.
Large properties use subnode streams and XBLOCKs. Heap allocations span multiple
HN pages using v36's 14-bit allocation index and block index at bit 19.
`unreadable = true` on an Item or folder destroys only its heap signature before
checksums are sealed. Indexes, tables and unrelated nodes remain readable.

The builder deliberately bounds fixtures: one NBT leaf (126 nodes), a BBT of up to two levels
(169 blocks per leaf, up to 169 leaves), one XBLOCK per stream (999 data blocks), at most 3500 bytes per
heap allocation. Larger property values become subnode streams. A description
that exceeds a structural bound panics rather than silently truncating data.
No allocation maps or free-space history are generated. These are reader-test
inputs, not evidence of Outlook/SCANPST acceptance or Faithful Conversion.

## Repeat the checks

Read the safety and environment docs first. Every Cargo invocation uses Windows
Sandbox. Stop if the wrapper reports another Sandbox is running.

```powershell
python scripts/sandbox-cargo.py test --locked -p pstoff-engine --test synthetic
python scripts/sandbox-cargo.py test --locked -p pstoff-engine --test synthetic export_for_libpff -- --ignored
```

The opt-in export writes four generated samples to
`%TEMP%\PstOffSandboxCargo\<worktree-digest>\output-v2\synthetic-23` through the
wrapper's writable output mapping. The digest is the first 16 hex characters of
SHA-256 of the case-folded absolute worktree path. Generated OSTs stay out of Git.
Use that directory's `/mnt/c/...` path for the independent check:

```powershell
wsl.exe -d Debian -- python3 /mnt/c/<checkout>/scripts/libpff-check.py synthetic-check /mnt/c/<output>/synthetic-23
```

That command uses `scripts/libpff-check.py` and its existing `bwrap` isolation.
It compares libpff's tree, Item classes and sizes, body/attachment hashes,
multi-page heap properties, recipients, embedded Item, external reference,
HeaderItem marker, and cached Q12/Q15 cells with literal synthetic expectations.
Both defect kinds must cause a read failure at their target; both healthy
controls must read without errors. Only counts, identifiers, tags, lengths,
hashes and error categories leave the probe. The 20180714 Python binding omits
recipient and embedded-Item methods, so the probe uses that same sandboxed
library's documented C API for those checks.

Two reader corrections were exposed by the independent check and are included:
`inspect` accepts the root folder's specified self-parent, while rejecting other
cycles; the format reader uses version-aware HIDs for multi-page heaps. See
[libpff's root handling](https://github.com/libyal/libpff/blob/main/libpff/libpff_item_tree.c)
and [v36 heap-reference decoding](https://github.com/libyal/libpff/blob/main/libpff/libpff_table.c).

The public OST fixture is absent from the integration branch and the copied
spec notes. This work uses generated synthetic data only. It supplies no new
public-fixture Conversion, Outlook or SCANPST evidence. Microsoft's pinned
outlook-pst reader accepts versions 14/15/23, so libpff is the independent v36
reader used here.

## Validation on 2026-10-26

- `python scripts/check`: dependency audit (25 packages), formatting, Clippy
  with warnings denied, and all 27 engine/CLI test cases passed.
- A fresh test profile (`profile.test.debug=1`) also passed those 27 cases and
  all 60 format-crate test functions. Upstream fixture-dependent tests can return
  early when their files are absent; their success is not public-fixture evidence.
- The opt-in export ran successfully in Sandbox. The independent check passed
  with libpff 20180714 on both healthy samples and both deliberate read failures.
  [Captured result and generated-file hashes](evidence/issue-23-synthetic.json).
