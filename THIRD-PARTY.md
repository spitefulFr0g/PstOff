# Dependency provenance and notices

All source imports below were inspected as data before any sandboxed build.
No private mail or spike implementation is included.

| Component | Source pin | License | Notice |
| --- | --- | --- | --- |
| pstfree library base, owned as pstoff-format | sp00nznet/pstfree b6e2ff31592f74458f0481d4671251c92044d8f0 | MIT | third-party/pstfree/LICENSE |
| outlook-pst 1.2.0, unmodified Cargo git dependency | microsoft/outlook-pst-rs cfb721daee538acc50d2bad6faac5ce724f6037d | MIT | third-party/outlook-pst-LICENSE |
| serde 1.0.228 | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files retained in Cargo cache/distribution sources |
| sha2 0.10.9 | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files; source identity hashing |
| serde_json 1.0.145 | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files; Conversion report serialization |
| miniz_oxide 0.9.1 | crates.io, Cargo.lock checksum | MIT OR Zlib OR Apache-2.0 | upstream crate license files |
| windows-sys 0.61.2 (fork's Windows code-page support) | crates.io, Cargo.lock checksum | MIT OR Apache-2.0 | upstream crate license files |

Cargo.lock is the authoritative transitive version/checksum list. Packaging
must retain dependency license texts alongside executable distributions;
this foundation does not produce a release bundle. All resolved package
license expressions must remain MIT-compatible. No GPL/LGPL dependency is
approved. The `gpui-pre` 0.3.9 snapshot and Windows dependency graph are recorded in `docs/gui-dependency-audit.md`. Cargo builds and checks run in Windows Sandbox through the repository wrappers;
WSL is reserved for the isolated libpff reader.

The resolved graph was audited using sandboxed `cargo metadata --locked`.
`scripts/check` enforces the reviewed license expressions and Microsoft pin.
The graph also includes adler2, byteorder, once_cell, pin-project-lite,
proc-macro2, quote, serde_core/derive, syn, thiserror/impl, tracing/attributes/core,
unicode-ident and windows-link. All offer MIT, with unicode-ident additionally
requiring Unicode-3.0, a permissive notice license compatible with MIT.
Keep that additional notice in distributions. The vendored `pstoff-format`
package is based on upstream pstfree commit
`b6e2ff31592f74458f0481d4671251c92044d8f0`, with local adaptations to strict
reader validation and v36 heap decoding. The upstream MIT notice is retained;
the local changes are tracked in this repository.

The tracer writer reuses the repository's `ticket/20-writer` implementation
at `49aa687`, including store policy and rebuilt PC/TC contexts. It does not
inherit that branch's validation claims. The engine enables the existing
windows-sys dependency's file-system API for exclusive Windows publication.
New transitive hashing/JSON crates are locked; `scripts/check` audits their
license expressions with the rest of the graph.

The new graph includes ryu 1.0.20 (`Apache-2.0 OR BSL-1.0`) and version_check
0.9.5 (`MIT/Apache-2.0`, the upstream legacy dual-license expression). Their
cached `LICENSE-APACHE`/`LICENSE-BOOST` and `LICENSE-APACHE`/`LICENSE-MIT`
files respectively were inspected as data. These exact metadata expressions
are now accepted by the license audit; upstream notices remain required in
distributions.
