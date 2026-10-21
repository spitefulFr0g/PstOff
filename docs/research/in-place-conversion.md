# Can an OST be safely converted into a PST in place?

Research for the ticket "Can an OST be safely converted into a PST in place?" on the
"PstOff: OST-to-PST feasibility and v1 decisions" map. Researched 2026-10-21. Terms follow
[GLOSSARY.md](../../GLOSSARY.md).

## Answer

- **Don't put true In-place mode in v1.** Rewriting an OST into a PST inside the same file is
  possible in principle. In practice it means building a complete new PST over the only copy of
  the data, and it saves no disk space in the case that matters.
- **Modern OSTs need every byte rewritten.** Outlook 2016 and later create a compressed OST by
  default. That format uses 4 KB pages, blocks of up to 64 KB, deflate compression and a
  different heap-reference layout, and none of it is allowed by Microsoft's PST specification
  (MS-PST). Every page, block, allocation map and heap reference has to be rewritten.
- **A header patch isn't enough, even for older 512-byte OSTs.** The OST store also uses a
  different store GUID and EntryID scheme, and it holds OST-only nodes.
- **Risk is high.** MS-PST has no journal and no second header copy. Its crash protection is
  copy-on-write with the header written last, plus an "AMap invalid" flag that Outlook 2013 and
  later ignore. A crash part-way through an in-place rewrite leaves a file that is neither a
  valid OST nor a valid PST.
- **Making it crash-safe removes the reason to do it.** To stay recoverable, the conversion has
  to keep every live OST structure until the new PST is complete, or it has to keep an undo
  journal elsewhere. Either way, peak disk use is about OST size plus PST size, the same as
  Copy mode.
- **No tool found converts in place.** Every tool inspected writes a new PST. Microsoft
  documents no "rename .ost to .pst" method, and a renamed OST fails the spec's header check.
- **Decision input for "Is in-place mode in v1, and with what safeguards?":**
  - Ship Copy mode only, or
  - offer a "replace the OST afterwards" step: run Copy mode, verify the PST, then replace or
    delete the OST with a backup. That step writes a second copy, so it doesn't fit the
    glossary's current definition of In-place mode. If it's chosen, the glossary entry needs
    rewording.
  - Preconditions for either: Outlook is closed, free space is checked first, and the PST is
    verified before the OST is touched.

Confidence: **high** on what MS-PST and Microsoft say. **Medium** on the details of the 4K OST
format and OST identity, which come from libpff's reverse-engineered notes. **Unverified**:
whether Outlook or scanpst would accept a header-patched OST.

## Sources and how they were used

- **[MS-PST]**, Microsoft's Outlook Personal Folders (.pst) File Format specification, as
  published on Microsoft Learn (last change-tracking entry 2025-02-18). Section URLs have the
  form `https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/<id>`, listed in
  [References](#references). MS-PST describes PSTs only. A search of every section fetched for
  "OST", "offline" and "cached" found nothing, so all OST facts below come from other sources.
  Section 1.6, Versioning and Localization, reads in full: "None."
- **libpff** format notes, the main public reverse-engineering of the OST format:
  `documentation/Personal Folder File (PFF) format.asciidoc` at commit `f92780a`
  (<https://github.com/libyal/libpff/blob/main/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc>).
  Line numbers below refer to that commit. These notes are primary for what has been observed,
  but they are not authoritative, and they mark many fields "Unknown" or "TODO".
- **Microsoft first-party documentation** on Outlook, Inbox Repair (scanpst) and Windows file
  I/O.
- **Source code and READMEs of existing converters**, cloned to `/tmp` and read but not built
  or run.

## 1. What has to change

### 1.1 The header

MS-PST 2.2.2.6 HEADER sets these rules for a Unicode PST:

| Field | MS-PST requirement | In an OST |
| --- | --- | --- |
| `dwMagic` | MUST be `!BDN` | Same (libpff, format notes) |
| `dwCRCPartial` | CRC of the 471 bytes starting at `wMagicClient` (offset 0x0008) | Must be recomputed after any change in that range |
| `wMagicClient` | **MUST be `{0x53, 0x4D}` ("SM")** | "SO" (0x53 0x4F); "AB" is a PAB (libpff lines 194–202) |
| `wVer` | MUST be 14 or 15 for ANSI, and MUST be ≥ 23 for Unicode. 37 means a WIP-capable Outlook wrote it | 23 for 512-byte Unicode; **36 for the "64-bit Unicode format with 4k"** (libpff lines 204–214) |
| `wVerClient` | 19 for the format the document describes | Not researched |
| `dwUnique` | Changed on every header modification, "to ensure that the HEADER CRCs are different after each header modification" | Must be bumped by a converter |
| `bCryptMethod` | 0x00 none, 0x01 permute, 0x02 cyclic, 0x10 WIP | libpff: "it seems that compressed files are not encrypted" (TODO, line 1177) |
| `dwCRCFull` | CRC of the 516 bytes from `wMagicClient` to `bidNextB`, inclusive | Must be recomputed |
| `dwReserved1`, `dwReserved2`, `rgbReserved2`, `bReserved`, `rgbReserved3` | "SHOULD ignore this value and SHOULD NOT modify it". Footnotes <9>–<13> say Outlook stores implementation-specific data there, and "Modification of this value can result in failure to read the PST file by Outlook." | What Outlook stores there in an OST is unknown |

Three things follow:

- **A literal rename fails the spec.** The bytes still say "SO", and MS-PST says
  `wMagicClient` MUST be "SM". A conforming reader may reject the file on that field alone.
- **The header patch itself is small:** two magic bytes, `dwUnique` and two CRCs, plus `wVer`
  if the layout changes. The reserved fields hold Outlook-private OST data whose meaning in a
  PST is undocumented (footnotes <9>–<13>). Copying them across is a guess, and so is zeroing
  them.
- **Note on `wVer`.** "≥ 23 for Unicode" does not by itself rule out 36, because 36 ≥ 23. The 4K
  layout is ruled out by the page and block sections in [1.3](#13-the-4k-page-compressed-format-outlook-2013-and-later),
  not by the version field. libpff decides the layout from `wVer` alone: ≤ 0x0F is 32-bit,
  ≥ 0x24 (36) is 4K, and ≥ 0x15 is 64-bit (`libpff/libpff_file_header.c`, lines 265–275).
  So libpff would read a WIP file (`wVer` 37) as 4K. This is passed to the sibling ticket "How
  do modern OSTs differ from PSTs on disk?"

### 1.2 OST-only structures and store identity

These are reverse-engineered findings. MS-PST is silent on OSTs.

- **Store GUID and EntryIDs differ.** libpff lines 3145–3175:
  - In a PST, "The record key in the message store item (33) is used as GUID in the entry
    identifiers". That record key is `0x0FF9` PidTagRecordKey.
  - In an OST, "The 0x6615 value in the message store item (33) is used as GUID in the entry
    identifiers… Note that the entryid object identifier data is different than those used in
    a PST file." The example shown is an Exchange-style `PidTagIpmAppointmentEntryId`.
- **What MS-PST requires instead.**
  - The message store's property context MUST include PidTagRecordKey ("the Provider UID of
    this PST"), PidTagDisplayName, PidTagIpmSubTreeEntryId, PidTagIpmWastebasketEntryId and
    PidTagFinderEntryId (2.4.3.1).
  - A PST EntryID is 4 zero flag bytes, then the store's PidTagRecordKey, then the NID
    (2.4.3.2).
  - If the record key is missing, "the PST client MAY generate a new unique ID, or reject the
    PST as invalid" (2.4.3.2).
- **What this means for a conversion.**
  - The store's property context must be rewritten, and so must every EntryID it holds (the
    special-folder pointers above).
  - Any EntryIDs stored elsewhere must be rewritten too, for example folder or reminder
    references. Their full list is unverified, so the scope of this rewrite is unknown.
  - Each rewrite is an LTP-level change, so under MS-PST it is copy-on-write all the way up the
    B-trees ([2.1](#21-what-ms-pst-provides)).
- **OST-only nodes.** NIDs 0x801, 0x821 and 0x841 appear in OSTs and are marked "Unknown (found
  in OST)" (libpff lines 2168–2173).
  - If they're left in place, a PST reader meets nodes MS-PST doesn't define.
  - Removing them is another B-tree modification.
  - What Outlook does with them in a PST is unverified.
- **Folder data is duplicated.** Folder property and table contexts copy properties of their
  children, and they "MUST" be kept coherent (2.4.4.3). Any per-item rewrite also touches the
  parent folder's tables.

The sibling ticket "How do modern OSTs differ from PSTs on disk?" owns the full list of
differences. This section covers only what bears on whether an in-place rewrite is possible.

### 1.3 The 4K-page compressed format (Outlook 2013 and later)

**What Microsoft says about the format.** Plan an Outlook deployment, "Upgrading to Office with
Cached Exchange Mode enabled":

> "By default, when Outlook 2016 or later versions are installed, a new compressed version of
> the Outlook data file (.ost) is created. This compressed version of the .ost is up to 40%
> smaller than the size of the .ost files that were created in earlier versions of Outlook."

A Group Policy, "Do not create new OST file on upgrade", keeps the old file. Microsoft does not
describe the layout.

**What MS-PST allows:**

- "A page is a fixed-size structure of 512 bytes" (2.2.2.7 Pages).
- "Blocks are assigned in sizes that are multiples of 64 bytes and are aligned on 64-byte
  boundaries. The maximum size of any block is 8 kilobytes (8192 bytes)" (2.2.2.8 Blocks).
- The Unicode BLOCKTRAILER holds `cb`, `wSig`, `dwCRC` and `bid` (2.2.2.8.1). There is no field
  for an uncompressed size, and MS-PST defines no compression anywhere.
- HID (heap reference): 5 bits of type, 11 bits of index and 16 bits of block index (2.3.1.1).
- Growing the file must happen in whole AMap intervals of "approximately 250 kilobytes", with
  every new AMap, PMap, FMap and FPMap initialized (2.6.1.1.2).

**What libpff observed in the 4K variant**, first found "in an Outlook 2013 OST file"
(lines 145–147):

| Structure | 512-byte PST (MS-PST) | 4K OST (libpff) |
| --- | --- | --- |
| Page | 512 bytes | 4096 bytes, with a 24-byte trailer at offset 4072 (lines 432–447) |
| First AMap | 0x4400, each map covering 253,952 bytes | 0x22000, each map covering 16,678,912 bytes (lines 513–546) |
| Density list | (MS-PST DList) | At 0x21000 (line 1051) |
| Block size | 64-byte multiples, at most 8192 bytes | "variable of size in 512 byte increments"; "likely maximum size of 8 x 8192 = 65536 bytes" (lines 1102–1104, 1150) |
| Block trailer | 16 bytes | 24 bytes, including an "Uncompressed block data size" (lines 1150–1166) |
| Compression | None | "the block data can be compressed. The compression method used is deflate/RFC1951." In the block index, a compressed block's size is the compressed size (lines 1169–1174) |
| HID split | 5 / 11 / 16 bits | 5 / 14 / 13 bits (lines 1549–1560) |

**Can a PST use this format?** Not under MS-PST. Its page size, block size and alignment, block
trailer and HID layout are all fixed, and the spec has no versioning mechanism (1.6 "None").

Whether Outlook's PST provider would still open a file marked "SM" in the 4K layout is
**unverified**: no primary source says either way. Outlook's storage code clearly reads the 4K
layout for OSTs, but Microsoft has never documented a 4K PST.

The one open-source converter that accepts 4K input writes 512-byte output on purpose. Its
DEVELOPMENT.md says "Do not infer output requirements from OST behavior" (Hyper-Beast/OST2PST,
DEVELOPMENT.md §5). Even if Outlook accepted a 4K PST, PstOff's Faithful bar also requires the
Inbox Repair Tool to pass with no errors. Leaning on undocumented Outlook behavior could break
in any Outlook update.

**What converting a 4K OST in place would involve:**

- Rewrite every page from 4096 to 512 bytes. That rebuilds every B-tree node and every map,
  each at a new offset.
- Inflate every compressed block and split blocks larger than 8192 bytes into PST data trees.
- Re-encode every HID inside every heap, because the bit layout differs.
- Rebuild the AMaps at different positions and granularity.

Nothing in the file keeps its offset. **This is writing a whole new PST, only in the same file
as the source.**

**Older 512-byte OSTs.** These come from Outlook before the compressed format, or were kept by
the upgrade policy. The work is smaller: the header, store identity, EntryIDs and OST-only
nodes. It is still copy-on-write through the B-trees, and it still depends on undocumented OST
semantics. How common such OSTs still are is not known.

## 2. Crash and power-cut safety

### 2.1 What MS-PST provides

- **Immutability** (2.6.1.3.1): "with the exception of the header and allocation metadata
  pages, the data in the NDB MUST NOT be modified in-place. Instead, a new copy of the data
  needs to be written at a new location, and then, when all references of the pre-existing data
  have been removed, the old data can be purged."
- **Modifying a page or block** (2.6.1.1.8, 2.6.1.1.9): write a new page or block, replace the
  references to it up through the parent pages, then free the old one. Creating one node
  touches the HEADER, the AMap, the BBT, the NBT and data blocks (2.6.1.3.6 Caching).
- **Transactional semantics** (2.6.1.3.3): "By architecting the sequence of modifications to
  ensure that BIDs are only updated after all the underlying data is successfully written, an
  implementation can design a system that leaves little or no chance for a Message object to
  end up in an inconsistent state."
- **The fAMapValid flag** (ROOT, 2.2.2.5; Crash Recovery and AMap Rebuilding, 2.6.1.3.7):
  - The flag is set to `INVALID_AMAP` at the start of any allocation or free and restored
    afterwards.
  - After an abnormal close, "the PST file MUST go through a very expensive recovery operation"
    (the AMap rebuild), which walks the NBT and BBT.
  - "implementations that do not implement AMap Rebuild algorithms MUST NOT modify the PST file
    in any way" while the flag is invalid.
  - Footnote <23> (Appendix B): "Outlook 2013 uses an internal algorithm instead of the AMap
    rebuild and does not set the fAMapValid flag. Outlook 2013 ignores the fAMapValid flag."

Some things are absent:

- There is no write-ahead or undo journal.
- There is no second copy of the header.
- There is no checksum over the whole file.

The header's ROOT holds the B-tree roots (`BREFNBT`, `BREFBBT`) and the file size. Writing the
header is therefore the single commit point. That commit is not one sector: `dwCRCFull` covers
516 bytes starting at offset 8, so the CRC-checked region ends at byte 524. That crosses the
first 512-byte boundary (2.2.2.6), so a power cut on a 512-byte-sector device can tear it.

How large a write the device does atomically is reported per device in
`FILE_STORAGE_INFO.PhysicalBytesPerSectorForAtomicity` ("Bytes per sector for atomic writes").
A torn header fails its CRC, and there is no fallback copy. What Outlook or scanpst does with
such a file is unverified.

### 2.2 What Windows provides

- **File Caching:** writes go to the cache by default, and the lazy writer flushes them later.
  "File system metadata is always cached. Therefore, to store any metadata changes to disk, the
  file must either be flushed or be opened with FILE_FLAG_WRITE_THROUGH."
- **FlushFileBuffers:** for data that must be on persistent media, "use the CreateFile function
  with the FILE_FLAG_NO_BUFFERING and FILE_FLAG_WRITE_THROUGH flags."
- **Deprecation of TxF** (Transactional NTFS "may not be available in future versions"): for
  document-like data, "A common approach is to write the document to a new file, then replace
  the original file with the new one. One method to do this is with the ReplaceFile API."
- **ReplaceFile** swaps a replacement file in for an existing one and can keep a backup copy of
  the original.

Microsoft's own recommendation for an atomic whole-file change is therefore write a new file,
then swap it in. That is Copy mode followed by a replace.

### 2.3 Why copy-on-write doesn't rescue an in-place conversion

Copy-on-write protects an already-valid PST: the old structures stay reachable until the header
is committed. An in-place conversion starts from a file whose live structures are an OST's.
There are three possible designs:

1. **Never overwrite live OST data until the PST is complete.** New PST pages and blocks then
   have to go into free space or past the end of the file. The file grows by about the PST's
   size before any OST space can be freed. The OST stays readable until the final header write,
   so this design is crash-safe at the cost of OST + PST disk space. It is Copy mode with both
   files inside one, and it is still harder: for a 4K OST every offset changes
   ([1.3](#13-the-4k-page-compressed-format-outlook-2013-and-later)), and the file can only grow
   in whole AMap intervals (2.6.1.1.2).
2. **Overwrite as you go, with no journal.** This is the only design that saves space. A crash
   or power cut part-way through leaves a file that is neither a valid OST nor a valid PST. The
   Mailbox may still hold the data, but for an Orphaned OST the only copy is gone.
3. **Overwrite as you go, with an undo journal in another file.** Before a region is
   overwritten, its old bytes are copied to the journal and flushed. The journal grows by the
   amount of overwritten OST data, so peak space again approaches OST + PST. It also needs
   careful flush ordering (2.2) and a recovery tool that understands both formats.

Two more constraints apply to every design:

- **Outlook must not have the file open.** The OST is Outlook's live cache. Hyper-Beast's README
  says "Close Outlook before copying the OST, or use an immutable copy" (README line 25).
- **Microsoft avoids in-place rewrites of these files.**
  - When Outlook moves to the compressed format, it creates a new OST rather than converting
    the old one (Plan an Outlook deployment).
  - The Inbox Repair Tool "creates a backup file during the repair process", a `.bak` beside
    the original, and Microsoft recommends backing up before running it (Repair Outlook data
    files).
  - For a damaged Exchange OST, Microsoft's advice is to delete it and let Outlook recreate it
    (same page).

## 3. How existing tools do it

No inspected tool converts in place. All of them write a new PST.

| Tool | How it converts | In place? | Evidence |
| --- | --- | --- | --- |
| Aspose.Email (commercial) | `PersonalStorage.FromFile(ost).SaveAs(path, FileFormat.Pst)` writes to a new path | No | "At present, the API supports converting OST formats to PST except OST 2013/2016." (docs.aspose.com, "Converting OST to PST") |
| Aspose forum, threads 224031 and 220508 | Users of Outlook 2019 OSTs get "The method or operation is not implemented." One user wrote that the output "was my ost with extension of pst" | — | User reports only, with no explanation from staff. They hint that Aspose's path for older OSTs may copy and patch bytes, but that is **unverified** |
| mkorthof/ost2pst (Java) | Wrapper around Aspose; usage `<input.ost> <output.pst>` | No | README |
| ternera/ost-to-pst | Wrapper around Aspose | No | Source |
| Arurikku/ost-to-pst | Wrapper around Aspose that uses a modified DLL to get around Aspose licensing | No | Source. Not a usable reference |
| Niv2023/ost2pst (C#, GPL-3.0) | Its own writer creates a new PST with `MagicClient = 0x4d53` ("SM") | No | `MS-PST/Constants.cs` line 9; `DATA-MGR/PstFile.cs` line 338 |
| Ahright11/ost2pst (Python) | Reads with libpff, writes a new PST | No | Source |
| Hyper-Beast/OST2PST (C++20, MIT, commit `53cf640`, 2026-10-16) | "reads the source directly and builds a new PST". Accepts "unencoded Unicode 4 KiB-page input" and "writes a 512-byte-page Unicode PST". "ANSI, WIP version 37 and nonzero `bCryptMethod` are refused." "output store identities/internal structures are regenerated." "A transactional staging journal supports recovery. The writer assembles a fresh PST, regenerates store-local references and allocation maps, rereads the candidate and publishes outcomes." Interrupted runs keep `<output>.work` and resume with `--resume`. | No | README lines 8–9, 33–39, 116, 142–144 |

**The "rename .ost to .pst" method.** Some vendor how-to pages describe renaming the file, and
some of the same pages say Outlook then rejects it as invalid (for example
<https://www.wps.com/office-quick-solution/document-file-issues/fix-outlook-ost-to-pst-conversion-and-renaming-errors/>).
That is vendor material, useful only as evidence of what is claimed. Microsoft documents no such
method. Under MS-PST a renamed OST is invalid, because `wMagicClient` is still "SO".

**Reliability.** No primary source shows any in-place or rename method producing a file that
passes Inbox Repair. The most complete open-source implementation for 4K input regenerates
identities and writes a fresh 512-byte PST. It also needed its own staging journal to make even
Copy mode resumable.

## 4. How much free disk space it still needs

**PST size compared with OST size.**

- **Upper bound from Microsoft's figure.** A compressed OST is "up to 40% smaller" than an
  uncompressed one. Its decompressed contents could therefore be up to about 1 / 0.6 ≈ 1.67×
  its size. This is my arithmetic on Microsoft's figure, not a measured bound.
- **Measured outcomes.** Hyper-Beast measured two real conversions (README lines 147–182). The
  PST came out at 77.5% of the OST (2.64 GB in) and 87.5% of the OST (26.25 GB in). Probable
  reasons: attachments are often already compressed, and OSTs carry free space (Outlook compacts
  only when there is "more than 20% of 'white space'", per "Reduce the size of your mailbox and
  Outlook Data Files"). Those reasons are untested here.
- **Conclusion.** The PST/OST ratio depends on the data. It can be below 1 and could be above 1.

**Peak free space by approach:**

| Approach | Peak extra space needed | Leaves a usable file after a crash? |
| --- | --- | --- |
| Copy mode | ≈ PST size, with the OST untouched | Yes. The OST is unchanged. Write the PST under a temporary name and rename it at the end |
| Copy mode, then replace the OST (ReplaceFile with a backup, or delete) | ≈ PST size; both files exist until the swap | Yes. The OST is kept until the PST is verified |
| In place, never overwriting live data (2.3, design 1) | ≈ PST size, as growth of the same file | Yes, but with far more complexity than Copy mode |
| In place with an undo journal (2.3, design 3) | Approaches PST size (journal plus growth) | Yes, if journaling and recovery are correct |
| In place, overwriting with no journal (2.3, design 2) | Roughly max(0, PST − OST) plus working room | **No.** The only copy is destroyed |

**A design idea, not researched further.** On NTFS, `FSCTL_SET_ZERO_DATA` on a sparse file
"may deallocate disk space". In principle, Copy mode could free OST regions it has finished
reading. That would cut peak space to a little over the larger of the two files. It is still
destructive to the source, and blocks shared between items, together with the OST B-trees, are
needed until the very end. This is not a v1 candidate.

## 5. Implications and recommendations

For the grilling ticket "Is in-place mode in v1, and with what safeguards?":

1. **Recommended: v1 ships Copy mode only.** True In-place mode gains no disk space unless it
   gives up crash safety. It is a second, harder conversion engine.
2. **If a space-saving option is wanted: "replace OST after a Faithful conversion".**
   - Run Copy mode to a temporary name in the destination folder.
   - Flush the data, then the header, then flush again.
   - Rename the PST to its final name, and verify it (reread, and optionally run Inbox Repair).
   - Only then replace or delete the OST, with ReplaceFile and a backup, or through the Recycle
     Bin.
   - It needs about the PST's size free, and a precheck must say so.
   - It writes a second copy, so the glossary's "In-place mode … no second copy is written"
     would need rewording, or this feature needs its own name.
3. **Preconditions for any mode that writes near or over the OST:**
   - Outlook is closed and the file is opened exclusively.
   - The free-space precheck uses a conservative PST/OST ratio until our own measurements exist.
   - `wVer`, `wMagicClient` and `bCryptMethod` are recognized.
   - The user has accepted the warning and the backup prompt.
4. **Copy mode should be crash-safe too.**
   - Write to a temporary file.
   - Order flushes the way MS-PST's copy-on-write expects: data first, header last.
   - Rename on success. A half-written PST should never sit under the final name.

## 6. Open questions for the map

- **Does a header-patched OST open?** Does classic Outlook, or the Inbox Repair Tool, open a
  *copy* of an OST with `wMagicClient` changed to "SM" and both CRCs and `dwUnique` updated?
  Test both a 512-byte OST and a 4K one. This is a cheap experiment for the task "What can this
  machine's unlicensed Outlook and scanpst do with a PST?". Run it on a copy only. It would show
  whether Outlook's PST provider accepts the 4K layout, which is useful context even though it
  doesn't change the recommendation.
- **`wVer` 37 (WIP) and the 4K layout.** libpff treats every `wVer` ≥ 36 as 4K. Can a WIP file
  have 512-byte pages? This is for the sibling ticket "How do modern OSTs differ from PSTs on
  disk?"
- **What the private test OST should record.** For the task "Put the private test OST in place
  and record its format", record:
  - `wMagicClient`, `wVer` and `bCryptMethod`
  - page size, and whether blocks are compressed
  - whether the store has 0x6615, PidTagRecordKey or both
  - whether NIDs 0x801, 0x821 and 0x841 are present
  - the free-space ratio
- **Measure the PST/OST size ratio.** Do this on the test OST once a converter exists. It feeds
  the free-space precheck and the size estimate for "How should a split divide items across
  PSTs?"
- **Where EntryIDs are stored.** Which stored EntryIDs besides the store's special-folder
  pointers need rewriting from OST form to PST form? This matters to any Conversion, not only
  In-place mode, and touches "Which OST-to-PST differences count as explained?"

## References

MS-PST sections. Prefix each ID with
`https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/`.

| Section | ID |
| --- | --- |
| 1.6 Versioning and Localization | `10e825c1-787e-4e40-9bd2-a8598ed3c75c` |
| 2.2.2.5 ROOT | `32ce8c94-4757-46c8-a169-3fd21abee584` |
| 2.2.2.6 HEADER | `c9876f5a-664b-46a3-9887-ba63f113abf5` |
| 2.2.2.7 Pages | `5774b4f2-cdc4-453e-996a-8c8230116930` |
| 2.2.2.8 Blocks | `a9c1981d-d1ea-457c-b39e-dc7fb0eb95d4` |
| 2.2.2.8.1 BLOCKTRAILER | `a14943ef-70c2-403f-898c-5bc3747117e1` |
| 2.3.1.1 HID | `85b9e985-ea53-447f-b70c-eb82bfbdcbc9` |
| 2.4.3.1 Minimum Set of Required Properties | `5493a0eb-0356-4e88-b4f5-0433ce0a93fa` |
| 2.4.3.2 Mapping between EntryID and NID | `9378e8b9-7b6a-45bf-a51a-f21daf24d9ce` |
| 2.4.4.3 Data Duplication and Coherency Maintenance | `0ca6bb65-92c3-4c94-9276-a7d5f8b15abe` |
| 2.6.1.1.1 Allocating Space from the PST | `d715d1ca-20e9-4966-b9b2-9cd36bd5f657` |
| 2.6.1.1.2 Growing the PST File | `cbea68b6-e93e-4477-a8ae-7e71c6b7908e` |
| 2.6.1.1.8 Modifying a Page | `e287bee3-dd1a-49aa-86cf-d0f5e4f49efe` |
| 2.6.1.1.9 Modifying a Block | `93faa1bc-e7be-4a15-8ff7-ed81ab7336a0` |
| 2.6.1.3.1 Immutability | `a6b9c705-4036-421b-8493-2fb2bbb9f3a1` |
| 2.6.1.3.3 Transactional Semantics | `bc5a92df-7fc1-4dc2-9c7c-5677237dd73a` |
| 2.6.1.3.6 Caching | `35847d36-42ec-422e-ac07-4c2c6cbb48ff` |
| 2.6.1.3.7 Crash Recovery and AMap Rebuilding | `d9bcc1fd-c66a-41b3-b6d7-ed09d2a25ced` |
| 6 Appendix B: Product Behavior (footnotes <9>–<13>, <23>) | `f040f8b2-f023-4ed9-94fd-de487da83ed5` |
| 7 Change Tracking | `5ef06572-0938-4652-8f56-753c0e74b9ba` |

Microsoft documentation:

- Plan an Outlook deployment: <https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/configuration/plan-outlook-deployment>
- Repair Outlook data files (.pst and .ost): <https://support.microsoft.com/en-us/office/repair-outlook-data-files-pst-and-ost-25663bc3-11ec-4412-86c4-60458afc5253>
- Reduce the size of your mailbox and Outlook Data Files: <https://support.microsoft.com/en-us/outlook/reduce-the-size-of-your-mailbox-and-outlook-data-files-pst-and-ost>
- File Caching: <https://learn.microsoft.com/en-us/windows/win32/fileio/file-caching>
- FlushFileBuffers: <https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers>
- FILE_STORAGE_INFO: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_storage_info>
- Deprecation of TxF: <https://learn.microsoft.com/en-us/windows/win32/fileio/deprecation-of-txf>
- ReplaceFileW: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew>
- FSCTL_SET_ZERO_DATA: <https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_zero_data>

libpff:

- Format notes, commit `f92780a`: <https://github.com/libyal/libpff/blob/main/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc>
- `libpff/libpff_file_header.c`, lines 265–275: <https://github.com/libyal/libpff/blob/main/libpff/libpff_file_header.c>

Tools:

- Aspose.Email, "Converting OST to PST": <https://docs.aspose.com/email/java/read-and-convert-outlook-ost-file/>
- Aspose forum: <https://forum.aspose.com/t/not-implemented-error-occurs-when-converting-ost-to-pst/224031> and <https://forum.aspose.com/t/bad-code-example-convertosttopst/220508>
- <https://github.com/mkorthof/ost2pst>
- <https://github.com/Niv2023/ost2pst>
- <https://github.com/Hyper-Beast/OST2PST>
- <https://github.com/Ahright11/ost2pst>
- <https://github.com/ternera/ost-to-pst>
