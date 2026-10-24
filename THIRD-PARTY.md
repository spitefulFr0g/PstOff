# Dependency provenance and notices

All source imports below were inspected as data before any sandboxed build.
No private mail or spike implementation is included.

| Component | Source pin | License | Notice |
| --- | --- | --- | --- |
| pstfree library base, owned as pstoff-format | sp00nznet/pstfree b6e2ff31592f74458f0481d4671251c92044d8f0 | MIT | third-party/pstfree/LICENSE |
| outlook-pst 1.2.0, unmodified Cargo git dependency | microsoft/outlook-pst-rs cfb721daee538acc50d2bad6faac5ce724f6037d | MIT | third-party/outlook-pst-LICENSE |
| serde 1.0.228 | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files retained in Cargo cache/distribution sources |
| miniz_oxide 0.9.1 | crates.io, Cargo.lock checksum | MIT OR Zlib OR Apache-2.0 | upstream crate license files |
| windows-sys 0.61.2 (fork's Windows code-page support) | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files |

Cargo.lock is the authoritative transitive version/checksum list. Packaging
must retain dependency license texts alongside executable distributions;
this foundation does not produce a release bundle. All resolved package
license expressions must remain MIT-compatible. No GPL/LGPL dependency is
approved. GPUI is intentionally deferred to the Windows feasibility/license
audit, and must not enter Linux builds. CI checkout action is pinned to
11bd71901bbe5b1630ceea73d27597364c9af683 (v4.2.2).

The resolved graph was audited using sandboxed `cargo metadata --locked`.
`scripts/check` enforces the reviewed license expressions and Microsoft pin.
The graph also includes adler2, byteorder, once_cell, pin-project-lite,
proc-macro2, quote, serde_core/derive, syn, thiserror/impl, tracing/attributes/core,
unicode-ident and windows-link. All offer MIT, with unicode-ident additionally
requiring Unicode-3.0, a permissive notice license compatible with MIT.
Keep that additional notice in distributions. The imported pstfree library
sources are byte-for-byte upstream; only the manifest/package name changed.
