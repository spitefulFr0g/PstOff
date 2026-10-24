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

## Reconstructed MIT notices for the Windows GPUI probe

The maintainer authorized the standard MIT text in
`third-party/seahash-simd_helpers-MIT` for these exact releases (#45). Both
archives declare `MIT` but contain no license/notice file, and the upstream
trees at their packaged VCS revisions contain no notice path. This is a
PstOff-reconstructed notice, **not an upstream-provided notice**. Attribution
uses the package authors from each exact archive's `Cargo.toml.orig`
`[package].authors`; email addresses are omitted. The years are crates.io
publication years supplied in the maintainer's decision, **not verified
copyright dates**. No dependency or license expression was changed.

| Package | License expression | Attribution source | Metadata-derived year |
| --- | --- | --- | --- |
| seahash 4.1.0 | MIT | `seahash-4.1.0/Cargo.toml.orig`: ticki and Tom Almeida | 2021 |
| simd_helpers 0.1.0 | MIT | `simd_helpers-0.1.0/Cargo.toml.orig`: Luca Barbato | 2019 |

Exact archive provenance (SHA-256 equals `spikes/windows-gpui/Cargo.lock`):

- `https://static.crates.io/crates/seahash/seahash-4.1.0.crate`: `1c107b6f4780854c8b126e228ea8869f4d7b71260f962fefb57b996b8959ba6b`; packaged VCS revision `94b632aeac099031c373599313d5b5f0acbbaec0` in `redox-os/seahash` on gitlab.redox-os.org.
- `https://static.crates.io/crates/simd_helpers/simd_helpers-0.1.0.crate`: `95890f873bec569a0362c235787f3aca6e1e887302ba4840839bcc6459c42da6`; packaged VCS revision `ca1a2f84aa386d758e98f8a609d990263932fb85` in `lu-zero/simd_helpers` on GitHub.

Bundle this full notice alongside the collected upstream notices for the
other 332 packages in the Windows-filtered 334-package probe graph. The
notice reconstruction resolves the two missing files under the maintainer's
approved choice; it does not establish an upstream copyright history.
