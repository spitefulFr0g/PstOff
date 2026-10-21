# OST versus PST on disk

Research for [#2](https://github.com/spitefulFr0g/PstOff/issues/2), part of [#1](https://github.com/spitefulFr0g/PstOff/issues/1). Researched 2026-10-21; planning only. Terms follow [GLOSSARY.md](../../GLOSSARY.md).

## Answer and evidence boundary

Reading all three established OST families is feasible. The 4 KB-page family needs substantial additions to an ordinary Unicode PST reader. Producing a **Faithful** PST is harder than reading an OST: it requires rebuilding storage, preserving item semantics, and reconciling store identity. Choose a conventional Unicode PST with 512-byte pages as the output target until Windows validation establishes otherwise. This is a recommendation, not an implemented decision.

**Verified from specifications:** Microsoft documents ANSI and Unicode PST layouts, required PST store properties and EntryIDs. **Verified from source inspection:** independent readers implement the 4K OST layout and decompression; pstfree has distinct conversion paths. **Reverse-engineered observations:** OST-specific identifiers and unexplained nodes. **Unknown:** classic Outlook acceptance of a genuine 4K-layout PST, and acceptance of an older OST converted by header changes alone. No downloaded code was executed, and no Outlook/SCANPST experiment was performed.

The source snapshots below were reused from existing local clones. Microsoft documentation is authoritative for PST requirements. Reader code establishes what an implementation does; libpff observations establish what its authors found, not a complete Outlook contract. Neither establishes that a generated file opens in Outlook.

## Header recognition, encoding and version traps

Both file types begin with bytes `21 42 44 4E` (`!BDN`). At offset 8, PST client magic is on-disk `53 4D` (`SM`, little-endian integer `0x4D53`); OST uses `53 4F` (`SO`, `0x4F53`). Do not confuse the byte string with the integer. `wVer` at offset 10 selects the storage family; `wVerClient` at 12 is a separate client-format version. PST creation uses client version 19. OST client version 12 is also recognized by Microsoft's older SDK. Sources: [MS-PST HEADER][header], [libpff header reader][pff-header], [SDK constants][sdk].

`bCryptMethod` is at offset 461 in ANSI and 513 in Unicode layouts. PST defines `0x00` none, `0x01` permutation, `0x02` cyclic, and `0x10` Windows Information Protection (WIP). Permutation's historical name, “compressible encryption,” does **not** mean DEFLATE compression. WIP is a separate protected-data case; do not assume the old reversible encodings handle it. Sources: [HEADER][header], [libpff encoding descriptions][pff-encoding].

The PST header permits Unicode versions >=23; version 37 signals a WIP-capable writer, with protection possible rather than mandatory. Thus “>=36 means 4K” is unsafe as a general PST rule: [libpff implements that threshold][pff-header], while the [Microsoft specification][header] assigns 37 another meaning. Gate unsupported versions/protection explicitly instead of silently treating them as tested version-36 OSTs.

Header changes require recomputing applicable CRCs. Changing the version number does not translate the underlying layout. [HEADER][header]

## Storage families and reading difficulty

### ANSI: versions 14/15 (`0x0E`/`0x0F`)

**Verified specification plus OST observations.** ANSI uses 32-bit BIDs and file offsets; Unicode uses 64-bit values. Width changes propagate through headers, roots, trees and trailers. ANSI is not simply Unicode with a different text encoding. [MS-PST ANSI versus Unicode][ansi]; [libpff version observations][pff-versions]

PST pages are 512 bytes. Blocks are aligned and allocated in 64-byte units, at most 8192 bytes including trailer. ANSI's 12-byte trailer leaves at most 8180 data bytes; Unicode's 16-byte trailer leaves 8176. These ordinary layouts have no version-36 block compression. [Pages][pages], [Blocks][blocks], [libpff block layouts][pff-blocks]

**Assessment:** medium reader effort if starting from scratch; lower incremental effort with a tested ANSI parser. Allow for legacy string/code-page handling and separate structure widths. Confidence is reduced by the lack of a validated older OST fixture in this investigation. An ANSI OST being physically readable does not prove it is a standalone PST after changing its header.

### Ordinary Unicode: version 23 (`0x17`), observed version 21 (`0x15`)

**Verified specification plus OST observations.** This retains 512-byte pages and 8192-byte maximum blocks with 64-bit BIDs/offsets. Version 21 is recognized in historical reader implementations and libpff notes; it is outside the current PST header's >=23 requirement. Treat it as a separately tested historical input case. [ANSI versus Unicode][ansi], [Blocks][blocks], [libpff versions][pff-versions]

**Assessment:** lowest incremental reader difficulty relative to an existing Unicode PST reader. Header acceptance must allow OST magic/client versions, but the folder/item layer still needs OST-aware interpretation. Do not infer all old or newly created files' layouts from their Outlook marketing version.

### 4K Unicode OST: version 36 (`0x24`), observed since Outlook 2013

**Verified reader behavior, based on reverse engineering.** [XstReader's 4K definitions][xst-layout] implement 4096-byte pages, a 24-byte page trailer, changed B-tree entry counts, and BBT entries with separate stored/inflated lengths. Its [NDB reader][xst-ndb] uses a 65536-byte block-size limit. Libpff describes 512-byte block increments and a 24-byte block trailer, but labels its proposed 64 KiB maximum as *likely*, not proven. Do not promote that qualification into a Microsoft-guaranteed format limit. [libpff blocks][pff-blocks]

[XstReader][xst-ndb] inflates when stored and inflated lengths differ, consuming a zlib header before DEFLATE. Therefore support both compressed and plain blocks, validate stored versus inflated sizes, and bound allocation/decompression. Merely increasing page size cannot read this family.

Heap identifiers change too: conventional HID fields are 5-bit type / 11-bit allocation index / 16-bit block index; 4K uses 5 / 14 / 13. [XstReader HID implementation][xst-hid] Consequently the high-level heap references must be translated when writing conventional PST heaps; byte-copying decompressed node streams is insufficient in the general case.

Microsoft documents creation of compressed OSTs by default in Outlook 2016; the 2013 association comes from reader authors' observations. These are different kinds of evidence. [Microsoft deployment guidance][cached], [XstReader recognition][xst-ndb]

**Assessment:** high reader effort relative to ordinary Unicode, but existing independent implementations make it tractable. Tests need multiple blocks, multi-page trees, multi-block heaps, subnodes, mixed compressed/plain data and damaged-size cases. The combination of compression and nonzero encoding remains uncertain: libpff explicitly leaves their ordering unresolved. Do not promise support for that combination without fixtures. [libpff compression caveat][pff-blocks]

### File-size limits are a separate dimension

A 64-bit address field is not an Outlook usability limit. Microsoft's settings document an ANSI cap of 2 GB and Unicode defaults of 50 GB for Outlook 2010/2013/2016, versus 20 GB in 2003/2007; policy can affect behavior. Compression also means source byte size does not predict output byte size. **Inference:** Split and free-space planning must estimate emitted PST data and respect the target Outlook configuration, rather than copy the OST's size or assume 64-bit means unlimited. [Microsoft size settings][sizes]

## OST-only state, folders and identity

Microsoft confirms OST folders contain local binary synchronization status, but does not map that state to physical nodes in its sync-monitor article. [Synchronization monitor][sync]

Libpff observes OST-only/unexplained NIDs `0x801`, `0x821`, `0x841`. Its entry-identifier notes distinguish an OST store GUID property `0x6615` from the PST store's `PidTagRecordKey` (`0x0FF9`) and say OST EntryID object data differs. These observations are not an exhaustive catalog or a deletion recipe. [libpff internal nodes][pff-nodes], [identity observations][pff-identity]

Pstfree describes NID types `0x14`–`0x19` as sync state and retains them. Libpff still labels several of these types unknown. **Unknown:** their complete semantics and whether each is exclusively synchronization data. Do not equate provider bookkeeping with disposable content based on an unverified type range. [pstfree limitations][pstfree-doc], [libpff node types][pff-types]

Outlook exposes Sync Issues, Conflicts, Local Failures and Server Failures as Exchange-only default folders. That is an API restriction, not proof their contents cannot be represented in a PST. **Recommendation:** preserve recoverable diagnostic items/local problem copies as items where possible; separately decide the fate of opaque provider state and report omissions. [Microsoft folder enumeration][folders]

A valid PST store requires RecordKey, DisplayName, IpmSubTreeEntryId, IpmWastebasketEntryId and FinderEntryId. PST EntryIDs contain zero flags, the store RecordKey/provider UID and the target NID. **Inference:** create coherent PST store identity and translate relevant store/folder references. Never blindly rewrite every binary EntryID: recipients or remote-object references may intentionally identify objects outside this PST. The full list of references requiring translation remains to be validated. [Required store properties][store], [PST ENTRYID][entryid]

A Partial cache can omit older items entirely. Conversion cannot reconstruct bytes absent from the OST. Also distinguish locally present items and provider metadata from the complete Mailbox. [Microsoft cache-window documentation][partial]

## Reconciling the two related notes

The library note's “header rewrite” describes **code behavior**, not proven compatibility. In [pstfree `repair::rebuild`][pstfree-repair], the ordinary Unicode small-page path copies surviving blocks, rebuilds indices and updates the output header. For an OST it changes client magic and client version; it does not invoke the modern path's store-identity reconstruction. This is not literally a two-byte in-place patch, nor evidence that every old OST needs only two field changes.

The [modern `convert` path][pstfree-convert] rebuilds the message-store node; its [author's limitations][pstfree-doc] describe missing special-folder EntryIDs and a password sentinel in observed OSTs, and explicitly say no pre-2013 sample was available. Those observations must not become universal claims about every OST. The older path's missing identity work is a concrete compatibility risk, not a completed proof of failure.

The in-place note's conclusion that identity work is required is a sound conservative **writer requirement for faithful general conversion**. Its stronger reading, that *every* pre-2013 OST has been experimentally shown to fail after header changes, is unsupported. Similarly, “every byte must be rewritten” should mean that a fresh conventional output needs rebuilt storage and translated references, not that every item property must change. The two notes can coexist once these evidence levels are explicit.

## Does Outlook accept a 4K PST?

**Unknown; not established here.** MS-PST defines 512-byte pages and blocks no larger than 8192 bytes; its permissive version field does not document the 4K layout. Independent readers accepting a renamed OST, and source names such as “Unicode4k,” do not establish classic Outlook PST-provider acceptance. Nor does the lack of a public specification prove Outlook rejects every such file. [Pages][pages], [Blocks][blocks], [XstReader][xst-ndb]

A decisive future experiment needs a deliberately constructed 4K-layout file with PST identity, exact Outlook build and SCANPST results, plus semantic comparison before/after. Renaming an OST alone confounds identity and layout. Until tested, output ordinary Unicode PST and treat 4K as an input family.

## Main writer risks and next decisions

1. **Heap rebuilding:** compressed blocks can inflate beyond ordinary PST capacity. Repartition allocations and remap HIDs, subnodes and references coherently; arbitrary stream chunking breaks addressing. Preserve named-property mappings, associated contents, recipients, attachments and folder tables rather than only visible messages. [HID implementation][xst-hid], [pstfree conversion limitations][pstfree-doc]
2. **Identity and store coherence:** generate required PST EntryIDs and reconcile special folders, without erasing meaningful remote references. Define and report deliberate changes. [Store requirements][store], [ENTRYID][entryid]
3. **Physical integrity:** emit correct trees, allocation maps, trailers, lengths and checksums. A reader opening the result is weaker evidence than the project's Faithful bar. Pstfree's output marks allocation maps invalid and its author requires repair first; do not adopt this as a finished writer contract. [pstfree converter][pstfree-convert], [limitations][pstfree-doc]
4. **Evidence gaps:** obtain anonymized/synthetic ANSI and ordinary Unicode OST fixtures; decide whether historical version 21 and WIP are supported or explicitly rejected. Prioritize version-36 compressed OSTs and conventional Unicode output. Keep 4K PST acceptance and header-only older-OST conversion as unresolved experiments, not v1 assumptions.

## Primary references

[header]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/c9876f5a-664b-46a3-9887-ba63f113abf5
[ansi]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/cdad232f-e7d9-41c1-9773-c7561b9cba04
[pages]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/5774b4f2-cdc4-453e-996a-8c8230116930
[blocks]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/a9c1981d-d1ea-457c-b39e-dc7fb0eb95d4
[store]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/5493a0eb-0356-4e88-b4f5-0433ce0a93fa
[entryid]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/9378e8b9-7b6a-45bf-a51a-f21daf24d9ce
[pff-header]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/libpff/libpff_file_header.c#L233-L275
[pff-versions]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L194-L214
[pff-encoding]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L383-L394
[pff-blocks]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L1095-L1178
[pff-types]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L850-L892
[pff-nodes]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L2168-L2174
[pff-identity]: https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L3145-L3175
[xst-layout]: https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/LayoutsU4K.cs
[xst-ndb]: https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/NDB.cs
[xst-hid]: https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/Layouts.cs#L110-L138
[sdk]: https://github.com/enrondata/microsoft-pst-sdk/blob/70701d755f52412f0f21ed216968e314433a324e/sourceCode/fairport/trunk/pstsdk/disk/disk.h#L64-L90
[pstfree-doc]: https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/docs/converting.md
[pstfree-repair]: https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/repair.rs#L230-L373
[pstfree-convert]: https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L507-L635
[cached]: https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/configuration/cached-exchange-mode
[sizes]: https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/data-files/configure-size-limit-outlook-data-files
[sync]: https://learn.microsoft.com/en-us/microsoft-365-apps/outlook/configuration/sync-monitor-improvements
[folders]: https://learn.microsoft.com/en-us/office/vba/api/outlook.oldefaultfolders
[partial]: https://learn.microsoft.com/en-us/troubleshoot/outlook/user-interface/only-subset-items-synchronized
