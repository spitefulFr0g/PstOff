# Public Rust conversion spike for #16

This unmerged bounded experiment informs #10. The pinned pstfree baseline converts a public version-36 OST into a version-23 Unicode PST that the pinned Microsoft reader opens. It does **not** establish Faithful Conversion: allocation maps are explicitly invalid, Windows acceptance is unknown, and semantic coverage is limited. No writer fixes or allocation-map repairs were made.

## Provenance

- pstfree 0.5.1, MIT: `https://github.com/sp00nznet/pstfree`, commit `b6e2ff31592f74458f0481d4671251c92044d8f0`, checkout `/tmp/pstres-sp00nznet_pstfree/repo`.
- outlook-pst 1.2.0, MIT: `https://github.com/microsoft/outlook-pst-rs`, commit `cfb721daee538acc50d2bad6faac5ce724f6037d`, checkout `/tmp/pstres-src/microsoft_outlook-pst-rs/repo`.
- Public OST: `https://github.com/rjohnsondev/java-libpst`, commit `f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26`, `src/test/resources/example-2013.ost`, logical size 16,818,176 bytes (sparse checkout file).
- Public PST control: Microsoft's pinned `crates/pst/examples/Empty.pst`.
- OST SHA256: `b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d`.
- Control SHA256: `c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5`.
- Official rustup minimal stable, Linux x86_64, installed without modifying PATH: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; `cargo 1.99.1 (5f94df478 2026-08-27)`.
- Rustup bootstrap `https://sh.rustup.rs`, script SHA256 `9502b514362a21096210d498f143b8cdacda365d3dfcf45e8f5541e089ccafc0`.
- Existing subscription-authenticated Codex session only; no other model calls or nested delegation.

Only these public fixtures were opened. No private OST or private directory was accessed. Originals and disposable mode-0444 copies matched the hashes before and after runtime checks. Bubblewrap mounts each input read-only, with no network connectivity or home-directory mount. Only system libraries, one executable, one public input and the dedicated output directory are mounted. Output is writable only for conversion. Every command and exit code is in `evidence/results.json`. Raw property dumps remain outside Git; committed evidence contains aggregate counts, structural IDs, hashes and diagnostics, without mailbox contents.

## Pre-execution inspection

Read both workspace/package manifests, pstfree's lockfile and `.cargo/config.toml`, relevant CLI/rebuild/convert/input-open paths, and Microsoft's header/store/input-open paths. Neither candidate has a build script. pstfree's config adds Windows-only linker manifest arguments; Linux CLI uses miniz_oxide/adler2. Its input uses `File::open`; conversion creates the explicit output path. No network client or process execution was found on those runtime paths. GUI/Windows FFI paths were not built.

Microsoft's `PstFile::open` attempts a write handle, but records failure and continues. Read-only mounting denies that handle. The probe never calls a transaction/write/rebuild-AMap API. The reader control succeeds under the same restriction.

The minimal probe uses local pinned libraries, avoiding Microsoft's example/UI dev dependencies. Committed Cargo.lock pins registry versions/checksums. Dependency build-script inspection identified proc-macro2/thiserror compiler feature probes (rustc invocation, OUT_DIR writes) and quote's rustc version check, with no network path. Fetching used crates.io; compilation was offline. This targeted review is not a full supply-chain audit. Builds were not sandboxed and never received fixture arguments.

## Reproduction

Use the pinned clones at the paths above. Inspect sources before building. From this branch root:

```bash
/home/watermelon/.cargo/bin/cargo fetch --locked --manifest-path /tmp/pstres-sp00nznet_pstfree/repo/Cargo.toml
CARGO_TARGET_DIR=/home/watermelon/.local/share/pstoff-spike16/target \
  /home/watermelon/.cargo/bin/cargo build --offline --locked --release --bin pstfree \
  --manifest-path /tmp/pstres-sp00nznet_pstfree/repo/Cargo.toml
/home/watermelon/.cargo/bin/cargo fetch --locked --manifest-path spikes/issue-16/probe/Cargo.toml
CARGO_TARGET_DIR=/home/watermelon/.local/share/pstoff-spike16/target \
  /home/watermelon/.cargo/bin/cargo build --offline --locked --release \
  --manifest-path spikes/issue-16/probe/Cargo.toml
python3 spikes/issue-16/run-public.py
```

The runner accepts no arbitrary input, checks candidate revisions/source diffs and fixture hashes, creates disposable copies, uses 60-second per-process timeouts, and rechecks hashes. Reruns overwrite dedicated generated PST/logs. Rust targets live on user-local disk outside `/tmp`.

## Performed evidence

- Source: version 36, 308 indexed nodes, 310 blocks, 40 ordinary folders, 6 search folders, 3 ordinary messages, 14 associated messages. All 310 indexed blocks readable, zero checksum failures. 80 folder-table/parent checks agree.
- Conversion: exit 0, 308 nodes and 518 blocks, no dropped-node warning. Output: version 23, PST magic, plain block encoding, 226,816 bytes. All 518 indexed blocks read back with zero checksum failures; 80 folder-parent checks agree.
- Allocation maps: pstfree reports invalid maps; Microsoft independently reports `amap_valid=Invalid`. `convert.rs:616` explicitly sets `header[248] = 0`; CLI says run SCANPST first. This is an acceptance gap. Actual scan-only SCANPST behavior remains for parent validation.
- Microsoft result read: exit 0, 25 store properties, 2 root hierarchy rows, 16 decoded root values, matching store/IPM record keys; IPM folder opens with hierarchy table. Partial structural interoperability only.
- Microsoft public control: exit 0, valid maps, 13 store properties, 4 root rows, 24 decoded values, IPM folder opens.
- Microsoft source read: exit 1, `InvalidNdbHeaderMagicClientValue(20307)` (OST client magic). Unchanged public open API cannot provide source semantic comparison.
- Same-candidate comparison: 64 top-level property contexts (store, ordinary/search folders, ordinary/associated messages) and 134 top-level table contexts read in each file, zero read errors. All 67 decoded table rows match. Of 1,495 source property values, 1,493 match, two store values differ, none disappear, four are added (1,499 result values). No MissingSubnode marker; three Raw-value representations on each side.
- Store changes: `0x3001` display name and `0x67ff` PST password differ; `0x35df` valid-folder mask and `0x35e0`, `0x35e3`, `0x35e7` folder entry IDs are added. Consistent with documented `pst_store` behavior, not an independent identity audit.
- No oversized-heap warning on this fixture. The known source gap remains in `convert.rs:172–204`: blocks above 8,176 bytes are split; heaps are warned about without reconstruction of allocation IDs/references. This fixture neither reproduces nor clears that failure.
- Source sweep finds six message nodes outside the live index. This run uses the live index without salvage; these are not evidence of six live items dropped. Recovery/deleted-item policy remains undecided.
- Repeated runs produced the same output hash.

## Unperformed / unknown

Scan-only SCANPST, Outlook open, Windows count/property checks and full independent item validation were not run here; parent owns Windows validation. libpff/pypff was not installed. No complete attachment/embedded-message byte comparison, recursive subnode inventory, named-property identity comparison, all-type raw-property audit, large-heap stress fixture or broad corpus was attempted. Same-candidate comparisons share parser blind spots and compare decoded debug representations, not all raw serialized tags/types. Unknown stays unknown; this result is not Faithful.

## Parent handoff

Generated PST absolute path:

`/home/watermelon/.local/share/pstoff-spike16/output/example-2013-pstfree.pst`

SHA256: `6c2a333a47cc772ed7b35bbf4148919ef7841fb938cf1185161aacf3fe670f95`.

Binary SHA256 values:

- pstfree: `8933a8e22cbb713d72fb2368463a0716b8c5ade202f0120343f49b190c465522`
- ms-probe: `b72a4e8b9c00c4bb057e44ae019c65bd9d9ba36a5ff28f46a00a2f9119b31261`
- pc-probe: `bb6cd50262d61ba7ef86471b52d3893e5d7a390db9a3c5c7c08cd9cd188e63aa`

For #10, this supports investigating hybrid Rust: pstfree demonstrates modern OST reading and a limited conventional-PST baseline; Microsoft supplies an independent result check. It does not justify adopting the writer unchanged. Valid allocation maps, heap/reference reconstruction and explicit store-identity/preservation rules remain writer work. Stop here rather than implementing those subsystems. Parent retains issue updates/closure; no PR.
