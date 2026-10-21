# Which libraries can read OSTs and write PSTs, and do their licenses fit MIT?

Research for the ticket of the same title, done on 2026-10-21. All claims come from each project's own source code, read at the commits linked below, or from the vendor's own pages. Repositories were cloned into `/tmp`. Nothing was built or run. "Unverified" means I could not confirm the claim from a primary source.

Recovered from the previously written note in the original research worktree; its original research and attribution are preserved. The decision-critical source checks below were repeated against the cited project sources. Volatile release and activity metrics are historical snapshots from 2026-10-21, not current measurements.

## Answer

No library reviewed here appears to provide the whole Conversion as a ready-to-use, validated dependency.

**Closest match: pstfree.** [pstfree](https://github.com/sp00nznet/pstfree) (MIT, Rust) documents and implements support for the Outlook 2013+ 4 KB-page compressed format. It writes a Unicode PST by copying node payloads and retaining node IDs while rebuilding PST structures; it is not a block-for-block copy. Its limits:

- It is a ten-week-old application with one author.
- Its PSTs are written with the allocation maps marked invalid, and the project documentation says SCANPST is required before use. A user report on the project issue says one of three files passed SCANPST and opened in Outlook; two failed SCANPST. This is limited user-reported evidence, not independent Outlook validation.

**Readers.** [libpff](https://github.com/libyal/libpff) is a strong independent reader candidate: its own source claims support for PST/OST content and 4 KB compressed OSTs. It is alpha and licensed LGPL-3.0-or-later. That license does not categorically prohibit static linking; distribution method, relinking/replacement rights, notices, and corresponding source obligations need case-specific review. This note is not legal advice. Readers that handle 4 KB OSTs under permissive licenses:

- pstfree (MIT)
- java-libpst (Apache-2.0)
- pst-extractor (MIT)
- XstReader (MS-PL)

Microsoft's own Rust crate, outlook-pst (MIT), is the most mature Rust PST code, but it rejects every OST.

**Writers.** No general-purpose PST writer exists:

- outlook-pst-rw (MIT) writes only email-shaped messages from a fixed set of fields.
- mbox2pst (BSD-2-Clause) writes structurally careful PSTs but has never been opened in Outlook.
- pstsdk's write API sits on an unreleased 2010-era branch.
- Aspose.Email (commercial) states that its one-call OST-to-PST conversion does not handle 2013+ OSTs.

**What this means for PstOff.** PstOff will need its own 4 KB-OST reader and PST writer. The best foundations:

- outlook-pst's NDB and LTP code, as a crate dependency or a fork.
- pstfree and mbox2pst as MIT- and BSD-licensed code to port from.
- java-libpst and libpff's format document as references.

The open problems are splitting OST heap blocks larger than 8,176 bytes and writing valid allocation maps.

## Comparison

"Pre-2013 OST" means `wVer` 23 with the `SO` magic. "4 KB OST" means `wVer` 36 with 4 KB pages and DEFLATE-compressed blocks (Outlook 2013 and later).

| Library | Language | Pre-2013 OST | 4 KB OST | Writes PSTs | Outlook accepts output | Activity (as of 2026-10-21) | License | How MIT PstOff can use it | Test files shipped |
|---|---|---|---|---|---|---|---|---|---|
| [outlook-pst](https://github.com/microsoft/outlook-pst-rs) (Microsoft) | Rust | No, rejects `SO` | No | No item-level writer. Has NDB/LTP write primitives and AMap rebuild | n/a | crates.io 1.2.0 (2026-03-17), last commit 2026-09-14, CI on 3 OSes, 15 tests | MIT | Crate dependency or fork | `Empty.pst` |
| [outlook-pst-rw](https://github.com/dongs0104/outlook-pst-rs) (fork) | Rust | No | No | Yes, but only email-like messages with fixed fields | Changelog cites SCANPST and Outlook fixes. Not independently verified | crates.io 1.2.14 (2026-10-14), first published 2026-08-09, one author, 37 tests | MIT | Crate dependency, or port its writer | `Empty.pst` |
| [pstfree](https://github.com/sp00nznet/pstfree) | Rust | Yes (header rewrite only, never tested on a real file) | Yes | Yes. Copies node payloads and rebuilds PST structures | User report: 1 of 3 opened after SCANPST; 2 failed. Not independent validation | v0.5.1 (2026-09-30), last commit 2026-10-14, one author, 68 tests | MIT | Port code (an app, not on crates.io) | None committed. A script fetches java-libpst's |
| [libpff](https://github.com/libyal/libpff) | C, with Python bindings | Yes | Yes | No. Open-for-write fails | n/a | Alpha. Last commit 2026-10-10, release alpha-20260926, many unit tests and fuzzers | LGPL-3.0-or-later (format doc GFDL-1.3) | Distribution and modification obligations require review; no blanket static-link prohibition | `outlook.pst` (CC-BY-4.0), downloaded |
| [pff-tools](https://github.com/avranju/pff-tools) | Rust FFI to libpff | Via libpff | Via libpff | No | n/a | Last push 2025-08-02, not on crates.io | MIT (libpff itself stays LGPL) | Shows the FFI route works | None |
| [libpst / readpst](https://github.com/pst-format/libpst) | C | Probably. The version check accepts it (code reading, untested) | Yes, since 0.6.71 (2017) | No. Writes mbox, MH, .eml, .msg | n/a | 0.6.76 (2021-12), last commit 2025-01-22 | GPL-2.0 | No | None in repo |
| [java-libpst](https://github.com/rjohnsondev/java-libpst) | Java | Yes | Yes. Rejects cyclic-encoded files | No (README says it will not) | n/a | Last commit 2022-07-12, last tag 0.9.4 (2017-01), 45 open issues | Apache-2.0 or LGPL (dual) | Port under Apache-2.0 | `example-2013.ost`, 2 PSTs |
| [pst-extractor](https://github.com/epfromer/pst-extractor) | TypeScript | Yes | Yes | No | n/a | 1.12.0 (2026-01-07) | MIT per `package.json`. No LICENSE file | Port (a java-libpst port) | 3 × 4 KB OSTs, `enron.pst` |
| [XstReader](https://github.com/Dijji/XstReader) | C# | Yes | Yes | No. Exports files and CSV | n/a | v1.14 (2021-01-02) | MS-PL | Port with care: ported files stay MS-PL | None |
| [pstsdk](https://github.com/enrondata/microsoft-pst-sdk) (Microsoft, mirror) | C++ header-only | Probably. Magic is not checked (code reading, untested) | No | Trunk: no. Unreleased branch has a write API | Unverified | v0.2.0 (2010), mirror 2018 | Apache-2.0 | Port, as a reference | 5 PSTs |
| [go-pst](https://github.com/mooijtech/go-pst) | Go | Unverified | Page layout yes, block decompression no | `writer` branch is unfinished | n/a | v6.0.2 (2023-06-13), main last changed 2023-09-06 | Apache-2.0 | Port, as a reference | 3 PSTs |
| [mbox2pst](https://github.com/roldud/mbox2pst) | Rust | n/a | n/a | Yes, from mbox. Valid AMaps, multi-page heaps | "has not been opened in Microsoft Outlook" | Created 2026-10-13, one author | BSD-2-Clause | Port, keeping the notice | Generates its own |
| [outlook-mapi](https://github.com/microsoft/mapi-rs) (Microsoft) | Rust bindings to Outlook's MAPI | Only through an Outlook profile (unverified for an Orphaned OST) | Same | Yes, through Outlook's own PST provider | Outlook writes the PST itself | 0.15.10 (2026-03-25) | MIT | Crate dependency. Needs Outlook installed | None |
| [Aspose.Email](https://docs.aspose.com/email/net/reading-and-converting-outlook-files/) | .NET, Java, C++, Python | Yes | Reads yes. One-call convert no | Yes | Unverified | Commercial | Proprietary | Paid license, from US$2,997 to redistribute. No Rust API | n/a |

## Details

### Microsoft outlook-pst (`outlook-pst` crate)

**What it is.** A Rust implementation of [MS-PST] that Microsoft describes as clean-room and compatible with `mspst32.dll`. It is the most polished Rust code in this space.

**Activity.** crates.io lists `outlook-pst` 1.2.0, published 2026-03-17, MIT, about 207,000 downloads. The repository's last commit is [`cfb721d`](https://github.com/microsoft/outlook-pst-rs/commit/cfb721daee538acc50d2bad6faac5ce724f6037d) (2026-09-14). It has 38 stars and 9 open issues. CI runs fmt, build, test and clippy on Ubuntu, macOS and Windows ([`.github/workflows/rust.yml`](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/.github/workflows/rust.yml)). The source has 15 `#[test]` functions.

**OST support: none.** The header parser in [`crates/pst/src/ndb/header.rs`](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs) rejects every OST in three places:

- It requires the PST client magic (`HEADER_MAGIC_CLIENT = b"MS"` big-endian, i.e. `SM` on disk, [line 15](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L15), checked at [line 182](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L182)).
- It accepts only `wVer` 14–15 and 23 ([lines 34–36](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L34-L36)).
- It enforces client version 19 ([line 41](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L41), [line 206](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L206)).

There is no zlib or deflate code anywhere in the crate.

**Writing: primitives only.** The crate README is headed ["Unimplemented: PST file modification"](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/README.md#L5-L9) and says it is "suitable for read-only access". It does implement crash recovery and AMap rebuilding (a `rebuild_amap` example). Its NDB and LTP types can be written as well as read, and the header writer is at [line 305](https://github.com/microsoft/outlook-pst-rs/blob/cfb721daee538acc50d2bad6faac5ce724f6037d/crates/pst/src/ndb/header.rs#L305). What it lacks is anything that builds folders and Items.

**License.** MIT, Copyright Microsoft. Contributing upstream needs the Microsoft CLA.

**Test files.** `crates/pst/examples/Empty.pst` (271,360 bytes).

**Fit for PstOff.** A good base for PST-side data structures. Teaching it to read OSTs means relaxing the header checks, adding 4 KB page and block layouts, and adding inflate. That would be a fork or an upstream contribution.

### outlook-pst-rw (dongs0104's fork)

**Activity.** crates.io `outlook-pst-rw` 1.2.14 (2026-10-14), MIT. It was first published 2026-08-09, has had 15 versions and about 4,400 downloads. The repository ([`4799f5d`](https://github.com/dongs0104/outlook-pst-rs/commit/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05)) has one author and no stars. Its library is still named `outlook_pst`, and it has 37 tests.

**What the writer can and can't do.** The writer lives in `crates/pst/src/lib.rs`:

- It creates a new PST by copying the bundled `Empty.pst` template ([line 661](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/src/lib.rs#L661)), then appends messages ([`create` line 294](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/src/lib.rs#L294), [`append` line 340](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/src/lib.rs#L340)).
- A message is the fixed struct [`UnicodePstMessage`](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/src/lib.rs#L249). Its fields are subject, sender name and email, recipients, plain and HTML body, message ID and delivery time. Attachments ([line 263](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/src/lib.rs#L263)) are by value.
- There is no way to write arbitrary MAPI properties, named properties, or non-mail Items. A Conversion through it could not be Faithful.
- Its README says ANSI append and encoded PSTs are unsupported, and that superseded blocks are not reclaimed ([README line 26](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/crates/pst/README.md#L26)).

**Outlook acceptance: claimed, not verified.** The changelog records fixes found by running real tools:

- Heap allocations over 3,580 bytes made "SCANPST stop with Fatal Error 80040813" ([CHANGELOG lines 32, 39](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/CHANGELOG.md#L32)).
- One table layout "made Outlook fail to load the folder's contents table" ([line 30](https://github.com/dongs0104/outlook-pst-rs/blob/4799f5d0aa5dfea1419e2eb2bca0be93065fcd05/CHANGELOG.md#L30)).

That implies testing against Outlook and SCANPST, which I have not verified independently. It does not read OSTs: the header code is upstream's.

### pstfree

**Activity.** [sp00nznet/pstfree](https://github.com/sp00nznet/pstfree) at [`b6e2ff3`](https://github.com/sp00nznet/pstfree/commit/b6e2ff31592f74458f0481d4671251c92044d8f0). MIT ([`Cargo.toml` line 7](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/Cargo.toml#L7)). It was created 2026-08-11; the latest release is v0.5.1 (2026-09-30). It has one contributor, about 9,700 lines of Rust and 6 stars. Its only dependency is `miniz_oxide`, plus `windows-sys` on Windows. It is an application with a library target and is not on crates.io.

**Reading.** It reads ANSI and Unicode PSTs, and OSTs including the 2013+ 4 KB compressed format.

**Writing.** `--rebuild` (alias `--convert`) writes a Unicode PST from an OST. Reading [`src/convert.rs`](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs):

- [`convert()`](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L507) re-emits each node's data as PST blocks of at most 8,176 bytes ([line 51](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L51)), with XBLOCK/XXBLOCK trees and rebuilt subnode trees. Node IDs are kept.
- It rebuilds the message store node in [`pst_store()`](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L342). An OST store has no IPM subtree, Deleted Items or Finder entry IDs, because Exchange supplies them, and its `PidTagPstPassword` (0x67FF) is -1. pstfree adds `0x35E0` and the others and sets the password to 0 ([lines 315–318](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L315-L318)).
- It writes the header with `fAMapValid = 0`, meaning invalid ([line 616](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/src/convert.rs#L616)).

**Its own documented limits** ([`docs/converting.md`](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/docs/converting.md)):

- Block boundaries must be preserved. Heap-on-node allocations are addressed by block index (`hidBlockIndex`, MS-PST 2.3.1.1), so re-splitting a stream breaks them. When a heap block is too big for a PST it is "reported rather than done quietly" ([lines 75–87](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/docs/converting.md#L75-L87)).
- "Outlook still wants SCANPST first, because the allocation maps go out marked invalid" ([line 126](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/docs/converting.md#L126)).
- OST sync-state nodes (NID types 0x14–0x19) are copied across unchanged.
- A pre-2013 OST is "already laid out exactly as a PST is", so only two header fields are rewritten. However, "No sample of one has turned up to test it against" ([lines 132–135](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/docs/converting.md#L132-L135)).

**Outlook acceptance: partial.** A user reported in [pstfree issue #1](https://github.com/sp00nznet/pstfree/issues/1#issuecomment-5964129628) (2026-10-03):

- One real OST (`testost3`) converted with v0.5.1, then passed SCANPST, then opened in Outlook with no password prompt and all folders present.
- Two other OSTs from a working profile converted, but SCANPST then failed with `80040900` at nodes `0x216E` and `0x840E`. It reported heap errors (bad `ibHnpm`, "cFree greater than cAlloc") in the split-heap path.

The issue is still open, and no fix had been released as of 2026-10-17.

**Tests.** The README reports 68 tests ([line 254](https://github.com/sp00nznet/pstfree/blob/b6e2ff31592f74458f0481d4671251c92044d8f0/README.md#L254)). `tests/bakeoff.py` compares pstfree's output against libpff, reading both the source OST and the result. Fixtures are fetched by `tests/fetch-fixtures.ps1` from java-libpst's files (through the freepst R package) rather than committed.

**Fit for PstOff.** The best available map of what an OST-to-PST copy must do. The code can be ported under MIT, but it is not a dependable dependency.

### libpff (libyal)

**Activity.** [`f92780a`](https://github.com/libyal/libpff/commit/f92780abd5e1225fd5b028b4f25316643f154daa), last commit 2026-10-10. The latest release is `libpff-alpha-20260926`. It has 372 stars and 17 open issues, many C unit tests under `tests/`, and OSS-Fuzz targets. The [README](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/README#L7-L8) says "Status: alpha" and "Licence: LGPL-3.0-or-later".

**Reading.** It reads PAB, PST and OST content types ([`libpff_file_header.c` line 247](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/libpff/libpff_file_header.c#L247)) and 32-bit, 64-bit and "64-bit with 4k pages with DEFLATE compression (Compressed OST files)" ([README line 20](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/README#L20); [`LIBPFF_FILE_TYPE_64BIT_4K_PAGE`, line 271](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/libpff/libpff_file_header.c#L271)). It can also recover deleted Items.

**Writing: none.** Opening with the write flag fails with "write access currently not supported" ([`libpff_file.c` line 540](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/libpff/libpff_file.c#L534-L540)).

**Format document.** The repo includes a format document, [`documentation/Personal Folder File (PFF) format.asciidoc`](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc), licensed GFDL-1.3. It is the most complete public description of the 4 KB OST layout. Block data is deflate-compressed, and the offset index records the compressed size ([lines 1169–1174](https://github.com/libyal/libpff/blob/f92780abd5e1225fd5b028b4f25316643f154daa/documentation/Personal%20Folder%20File%20(PFF)%20format.asciidoc#L1169-L1174)). Microsoft does not document this format: [MS-PST] covers PSTs only, as the XstReader source also notes.

**License fit (not legal advice).** The LGPL-3.0-or-later license does not categorically bar static linking. The obligations depend on how the combined work is distributed and may include notices, source availability, and allowing users to modify or replace the LGPL component; porting/adapting library code also raises license questions. A dynamically replaceable library is one possible architecture, not the only legally possible one. Have the actual distribution model reviewed before selecting this dependency. Building libpff on Windows also means its libyal sub-libraries (`synclibs.sh`) and the msvscpp project files.

**Rust bindings.** [pff-tools](https://github.com/avranju/pff-tools) (MIT, last push 2025-08-02) shows the Rust route works: its `pff-sys` crate uses bindgen against a system libpff found through pkg-config ([`pff-sys/build.rs` lines 6–7](https://github.com/avranju/pff-tools/blob/f19f7344e78e5b6286722d1c2ce8a6042ae88313/pff-sys/build.rs#L6-L7)). It is not on crates.io.

**Test files.** `synctestdata.sh` downloads only `outlook.pst` from [libyal/testdata](https://github.com/libyal/testdata) (CC-BY-4.0). It ships no OST.

**Fit for PstOff.** The strongest independent reader for checking that a Conversion is Faithful, whether through Python `pypff` in a test harness or a DLL. It is not a writer.

### libpst / readpst

**Activity.** [`cc600ee`](https://github.com/pst-format/libpst/commit/cc600ee98c4ed23b8ab0bc2cf6b6c6e9cb587e89), last commit 2025-01-22. The version is 0.6.76 ([`configure.ac` line 2](https://github.com/pst-format/libpst/blob/cc600ee98c4ed23b8ab0bc2cf6b6c6e9cb587e89/configure.ac#L2)) and the last tag is from 2021-12. GPL-2.0 ([`COPYING`](https://github.com/pst-format/libpst/blob/cc600ee98c4ed23b8ab0bc2cf6b6c6e9cb587e89/COPYING)).

**Reading.** It recognizes `wVer` 0x0E, 0x0F, 0x15, 0x17 and 0x24 ([`src/libpst.c` lines 23–27, 355–371](https://github.com/pst-format/libpst/blob/cc600ee98c4ed23b8ab0bc2cf6b6c6e9cb587e89/src/libpst.c#L355-L371)) and links zlib. "Add support for the OST 2013 format" arrived in 0.6.71, 2017 ([NEWS line 6](https://github.com/pst-format/libpst/blob/cc600ee98c4ed23b8ab0bc2cf6b6c6e9cb587e89/NEWS#L6)). Only the signature and `wVer` are checked, not the client magic, so a pre-2013 OST should open. That is from reading the code, not from testing.

**Writing.** `readpst` converts to mbox, MH, separate `.eml` files and `.msg` (`src/msg.cpp`). It never writes PSTs.

**Fit for PstOff.** GPL-2.0 rules out linking it or porting it.

### java-libpst

**Activity.** [`f158a64`](https://github.com/rjohnsondev/java-libpst/commit/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26) on `develop`, last commit 2022-07-12. The last tag is 0.9.4 (2017-01-19); there are no GitHub releases. It has 274 stars and 45 open issues.

**License.** "licensed under both the LGPL and Apache License v2.0" ([README.txt line 5](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/README.txt#L5)), with both license files present. PstOff can take the Apache-2.0 option.

**Reading.** It accepts `wVer` 14/15, 23 and 36 ([`PSTFile.java` lines 55–58, 171–182](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/main/java/com/pff/PSTFile.java#L171-L182)). It inflates 4 KB OST blocks ([`PSTNodeInputStream.java` lines 136, 163](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/main/java/com/pff/PSTNodeInputStream.java#L136)). It throws on cyclic-encoded files: "Only unencrypted and compressable PST files are supported" ([line 193](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/main/java/com/pff/PSTFile.java#L192-L194)).

**Writing.** Its README lists "Provide write access to PST files" among things it "will most likely not do" ([README.md lines 12–16](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/README.md#L12-L16)). The code has no writer.

**Test files** (`src/test/resources`):

| File | Size | Header |
|---|---|---|
| `example-2013.ost` | 16,818,176 bytes | `SO`, `wVer` 36, client 12, unencoded |
| `dist-list.pst` | 271,360 bytes | `wVer` 23, permute-encoded |
| `passworded.pst` | 271,360 bytes | `wVer` 23, permute-encoded |

The OST was added 2014-06-08 in "Addition of base 2013 ost test" and is used by [`Version36Test`](https://github.com/rjohnsondev/java-libpst/blob/f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26/src/test/java/com/pff/Version36Test.java). Its origin and license beyond the repository's own are unverified.

### pst-extractor

**What it is.** [epfromer/pst-extractor](https://github.com/epfromer/pst-extractor) at [`7d08ef9`](https://github.com/epfromer/pst-extractor/commit/7d08ef9bc7112c23275fa948306c76bba6e24fba): version 1.12.0, 2026-01-07, 88 stars. It is a TypeScript port of java-libpst.

**License.** `package.json` declares MIT ([line 19](https://github.com/epfromer/pst-extractor/blob/7d08ef9bc7112c23275fa948306c76bba6e24fba/package.json#L19)) and the README says "MIT © epfromer". The repository has no LICENSE file, and the README credits java-libpst as its basis. Whether porting from an Apache/LGPL project carries any notice obligations was not checked.

**Reading.** It handles `wVer` 36 ([`PSTFile.class.ts` line 24](https://github.com/epfromer/pst-extractor/blob/7d08ef9bc7112c23275fa948306c76bba6e24fba/src/PSTFile.class.ts#L24)) and inflates blocks with Node's zlib ([`PSTNodeInputStream.class.ts` line 121](https://github.com/epfromer/pst-extractor/blob/7d08ef9bc7112c23275fa948306c76bba6e24fba/src/PSTNodeInputStream.class.ts#L121)). No writer.

**Test files.** Three more 4 KB OST fixtures. All three are 16,818,176 bytes, `wVer` 36, unencoded, and distinct from java-libpst's. Filenames that appeared to contain email addresses were omitted. There is also `enron.pst`. Their origin is unverified.

### XstReader

**What it is.** [Dijji/XstReader](https://github.com/Dijji/XstReader) at [`3b7856c`](https://github.com/Dijji/XstReader/commit/3b7856c8b3d01bdf8aa13744e476d1ef7761b832). The last release is v1.14 (2021-01-02). It has 692 stars and 21 open issues. Licensed MS-PL ([`license.md`](https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/license.md)).

**Reading.** It reads `wVer` 0x0E/0x0F, 0x15/0x17 and 0x24 ([`XstReader.Base/NDB.cs` lines 180–209](https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/NDB.cs#L180-L209)). A comment there notes that the 4 KB format "is not documented in [MS-PST], being .ost only". It inflates compressed blocks with `DeflateStream` after skipping the 2-byte zlib header ([lines 519–535](https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/NDB.cs#L519-L535)). Its 4 KB structures, including the HID bit split (14-bit index, 13-bit block index), are in [`LayoutsU4K.cs`](https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/LayoutsU4K.cs) and [`Layouts.cs` lines 114–125](https://github.com/Dijji/XstReader/blob/3b7856c8b3d01bdf8aa13744e476d1ef7761b832/XstReader.Base/Layouts.cs#L114-L125). That bit split is a detail worth noting for any writer.

**Writing.** It exports email bodies, attachments and properties to files and CSV. It never writes PSTs.

**License fit.** MS-PL is a weak, file-level copyleft. Using it unchanged is fine, but code ported from it must stay under MS-PL in those files. Reading it as a reference is the safer use.

### pstsdk (Microsoft's PST SDK)

**What it is.** Microsoft released pstsdk in 2010 alongside the [MS-PST] documentation ([Microsoft Interoperability blog](https://learn.microsoft.com/en-us/archive/blogs/interoperability/two-open-source-projects-to-facilitate-interoperability-with-outlook-pst-data-files)). CodePlex is gone. The copy read here is the [enrondata/microsoft-pst-sdk](https://github.com/enrondata/microsoft-pst-sdk) mirror (last change 2018), which expands the CodePlex archive. Its last CodePlex release was v0.2.0 "Beta" (2010-07-04, `releases/releaseList.json`). It is Apache-2.0: the mirror's `LICENSE`, plus `sourceCode/fairport/trunk/pstsdk/pst.h` line 14. It needs Boost 1.42.

**Reading.** It knows `wVer` 14/15 and 20–23 and defines the OST magic and client version 12 (`trunk/pstsdk/disk/disk.h` lines 64–90). However, `validate_header` checks only `wVer` (`trunk/pstsdk/ndb/database.h` lines 180–200), so a pre-2013 OST would probably open (untested). It has no zlib or deflate code and no 4 KB layout, so 4 KB OSTs are unsupported.

**Writing.** Trunk is read-only; the README says "a library for reading PST-format email archives". The unreleased CodePlex branch `branch/ankopp` adds a write API: `create_empty_message_store()`, `save_pst()` (`branch/ankopp/pstsdk/pst/pst.h` lines 167–196) and an `allocation_map` with transactions (`branch/ankopp/pstsdk/ndb/allocation_map.h`). Whether Outlook accepts its output is unverified.

**Test files.** `trunk/test/` ships `sample1.pst`, `sample2.pst`, `submessage.pst`, `test_ansi.pst` and `test_unicode.pst`, all under Apache-2.0.

(The paths above are inside `sourceCode/` at mirror commit [`70701d7`](https://github.com/enrondata/microsoft-pst-sdk/tree/70701d755f52412f0f21ed216968e314433a324e/sourceCode).)

### go-pst

**Activity.** [mooijtech/go-pst](https://github.com/mooijtech/go-pst), Apache-2.0. `main` was last changed 2023-09-06 at [`74d91fa`](https://github.com/mooijtech/go-pst/commit/74d91fa0faec3830b4e2a6caa70df767d9656aed); later pushes are dependency-bot branches. The last release is v6.0.2 (2023-06-13).

**Reading.** On `main`, `GetFormatType` maps 14/15 to ANSI, 21/23 to Unicode and 36 to "Unicode4k" ([`pkg/file.go` lines 203–215](https://github.com/mooijtech/go-pst/blob/74d91fa0faec3830b4e2a6caa70df767d9656aed/pkg/file.go#L203-L215)), and the B-tree code has 4 KB page cases. There is no block decompression on `main`: no zlib import, and `go.mod` has no compression library. So 4 KB OSTs with compressed blocks are not fully readable there.

**Writing.** The separate `writer` branch ([`743c4be`](https://github.com/mooijtech/go-pst/commit/743c4be7838b981354adac59170fdb24d449339d), 2023-09-17, "start Unicode 4k (OST) support") adds about 2,200 lines of `*_writer.go` and a `zlib.go`. It is full of `TODO`s in the B-tree, block and heap writers (for example, [`pkg/btree_writer.go` lines 84–86](https://github.com/mooijtech/go-pst/blob/743c4be7838b981354adac59170fdb24d449339d/pkg/btree_writer.go#L84-L86)), and the README's Writer section is an empty code block. It is unfinished and unreleased.

**Test files.** `data/support.pst`, `data/enron.pst` and `data/32-bit.pst`.

### mbox2pst

**What it is.** [roldud/mbox2pst](https://github.com/roldud/mbox2pst) at [`9e3c4e7`](https://github.com/roldud/mbox2pst/commit/9e3c4e792a45da80fdc8ce2bda45c5ea18f93d79), BSD-2-Clause, created 2026-10-13. It is a from-scratch Rust PST writer of about 1,700 lines (`ndb.rs`, `ltp.rs`, `pst.rs`) that turns Thunderbird mbox into a Unicode PST. It does not read OSTs.

**How careful the writer is:**

- It builds multi-page heaps with page maps and fill levels ([`src/ltp.rs`](https://github.com/roldud/mbox2pst/blob/9e3c4e792a45da80fdc8ce2bda45c5ea18f93d79/src/ltp.rs#L94-L200)).
- It writes real allocation maps and sets `fAMapValid` to `VALID_AMAP2` ([`src/ndb.rs` line 522](https://github.com/roldud/mbox2pst/blob/9e3c4e792a45da80fdc8ce2bda45c5ea18f93d79/src/ndb.rs#L522)). That is exactly what pstfree lacks.
- It ships `tools/validate_pst.py`, which checks the spec plus rules "Outlook and its Inbox Repair Tool (`scanpst.exe`) enforce beyond the published specification" ([README lines 72–90](https://github.com/roldud/mbox2pst/blob/9e3c4e792a45da80fdc8ce2bda45c5ea18f93d79/README.md#L72-L90)). It is not reviewed further here.

**Outlook acceptance: untested.** Its README says the output "has not been opened in Microsoft Outlook during development" ([line 63](https://github.com/roldud/mbox2pst/blob/9e3c4e792a45da80fdc8ce2bda45c5ea18f93d79/README.md#L63)).

**Fit for PstOff.** A clean, permissive reference for the write side.

### outlook-mapi (Microsoft mapi-rs): writing through Outlook itself

**What it is.** [microsoft/mapi-rs](https://github.com/microsoft/mapi-rs) at [`a098370`](https://github.com/microsoft/mapi-rs/commit/a0983702baed876130993ab265db05032dfe0cbf), MIT. It publishes `outlook-mapi` 0.15.10 and `outlook-mapi-sys` 0.7.5 (2026-03-25), Windows bindings to Outlook's MAPI that delay-load `olmapi32.dll`.

**What it could do.** The bindings include `MAPIAdminProfiles` ([`bindings.rs` line 968](https://github.com/microsoft/mapi-rs/blob/a0983702baed876130993ab265db05032dfe0cbf/crates/mapi-sys/src/bindings.rs#L968)), `IMsgServiceAdmin::CreateMsgService` ([line 29073](https://github.com/microsoft/mapi-rs/blob/a0983702baed876130993ab265db05032dfe0cbf/crates/mapi-sys/src/bindings.rs#L29073)) and `IMAPIFolder::CopyFolder`. In principle, PstOff could create a PST through Outlook's own PST provider and copy into it, so Outlook would write the file itself.

**Unverified:**

- whether this works without a licensed or configured Outlook
- whether Outlook's MAPI can open an Orphaned OST at all, given that the GLOSSARY says Outlook opens an OST only through the profile that created it

It requires Outlook to be installed. Not investigated further.

### Aspose.Email (commercial reference)

**Reading.** "The process for reading an OST file with Aspose.Email is exactly the same as for reading a PST file" ([Aspose docs](https://docs.aspose.com/email/net/reading-and-converting-outlook-files/)).

**Converting.** The same page says the API "supports converting OST formats to PST **except 2013/2016/2019/2021 and future versions**". The [`PersonalStorage.SaveAs` reference](https://reference.aspose.com/email/net/aspose.email.storage.pst/personalstorage/saveas/) says the method "doesn't support conversion of OST created by MS Office 2013 and later versions" and suggests creating a new PST and using `MergeWith`. Whether that route gives a Faithful result is unverified.

**License.** Closed-source and commercial. The [pricing page](https://purchase.aspose.com/pricing/email/net/) lists Developer Small Business at US$999, which does not allow distribution to third parties. The cheapest tier that allows it is Developer OEM at US$2,997. Higher tiers and metered plans exist.

**Rust.** No Rust API; PstOff would need .NET or C++ interop.

### Other crates.io results

**Unusable or out of scope.**

- `pst-rs` 0.0.1 (AGPL-3.0-or-later): a name reservation whose repository returns 404.
- `msg_parser`: handles `.msg` files.
- `compressed-rtf` (MIT, part of outlook-pst-rs): useful later for RTF bodies.

**No relevant crates.** Searches for `pst`, `ost`, `pff`, `libpff`, `outlook`, `mapi` and `msg` turned up nothing else that reads OSTs or writes PSTs.

### Microsoft's specification

[MS-PST](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/141923d5-15ab-4ef1-a524-6dce75aae546) is listed under Microsoft's [Open Specification Promise](https://learn.microsoft.com/en-us/openspecs/dev_center/ms-devcentlp/1c24c7c8-28b0-4ce1-a47d-95fe1ff504bc), which covers conforming implementations against Microsoft's patent claims. This search did not check whether the OST-only 4 KB format, which MS-PST does not document, falls under it. This is a licensing note, not legal advice.

## Public test files found

**4 KB OSTs (`wVer` 36).** Four distinct files, each 16,818,176 bytes:

- `example-2013.ost` in java-libpst (also redistributed by the freepst R package)
- three in pst-extractor's `src/__tests__/testdata` and `example/testdata`

**PSTs.**

- java-libpst: two (`wVer` 23)
- pstsdk: five, including an ANSI one
- go-pst: three, including a 32-bit one
- libyal/testdata: `outlook.pst` (CC-BY-4.0)
- the Enron PSTs in pst-extractor and go-pst

**Pre-2013 OSTs.** None found. pstfree's author reports the same.

## Not covered

- Commercial tools other than Aspose.
- Whether `pstsdk`'s `ankopp` branch or `outlook-pst-rw` output survives SCANPST. Both would need testing on Windows.
- The Outlook MAPI route in practice.
