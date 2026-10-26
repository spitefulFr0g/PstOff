# Windows GUI dependency expressions

The Windows-filtered locked metadata adds the exact permissive expressions below.
The audit remains closed to unlisted expressions and checks the GPUI snapshot pin.
Package license declarations and license files are from the exact cached crate
archives downloaded by the Sandbox wrapper; no downloaded code was executed on
the host. Representative license notices were inspected: GPUI Apache 2.0,
arrayref BSD 2 clause, avif-serialize BSD 3 clause, ICU Unicode v3, foldhash Zlib,
fnv/atomic dual MIT/Apache, imgref CC0/Apache, and dunce CC0. Alternate expressions
with MIT use that option; Apache/BSD/Unicode/Zlib notice obligations remain.

These dependencies retain their own licenses. PstOff remains MIT; this audit
does not replace redistribution notices. Release packaging (#41) must include
the relevant license/notice texts. The reconstructed seahash/simd_helpers notice
is identified separately per #45.

| Package | Version | Declaration | License files in exact archive |
| --- | --- | --- | --- |
| arrayref | 0.3.9 | BSD-2-Clause | LICENSE |
| atomic | 0.5.3 | Apache-2.0/MIT | LICENSE-APACHE, LICENSE-MIT |
| av1-grain | 0.2.5 | BSD-2-Clause | LICENSE |
| avif-serialize | 0.8.9 | BSD-3-Clause | LICENSE |
| bit_field | 0.10.3 | Apache-2.0/MIT | LICENSE-APACHE, LICENSE-MIT |
| borsh-derive | 1.8.1 | Apache-2.0 | LICENSE-APACHE, LICENSE-MIT |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | LICENSE-APACHE, LICENSE-MIT, LICENSE-ZLIB |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | LICENSE-APACHE, LICENSE-MIT, LICENSE-ZLIB |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | LICENSE |
| exr | 1.74.2 | BSD-3-Clause | LICENSE.md |
| flume | 0.12.0 | Apache-2.0/MIT | LICENSE-APACHE, LICENSE-MIT |
| fnv | 1.0.7 | Apache-2.0 / MIT | LICENSE-APACHE, LICENSE-MIT |
| foldhash | 0.2.0 | Zlib | LICENSE |
| gpui-pre | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-collections | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-derive-refineable | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-http-client | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-macros | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-platform | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-refineable | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-scheduler | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-shared-string | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-sum-tree | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-util | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-windows | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-ztracing | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| gpui-pre-ztracing-macro | 0.3.9 | Apache-2.0 | LICENSE-APACHE |
| icu_collections | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_locale_core | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_normalizer | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_properties | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_properties_data | 2.4.0 | Unicode-3.0 | LICENSE |
| icu_provider | 2.4.0 | Unicode-3.0 | LICENSE |
| imgref | 1.12.3 | CC0-1.0 OR Apache-2.0 | LICENSE-APACHE, LICENSE-CC0 |
| lebe | 0.5.3 | BSD-3-Clause | LICENSE-BSD-3-Clause |
| litemap | 0.8.3 | Unicode-3.0 | LICENSE |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | LICENSE-APACHE.md, LICENSE.md |
| pollster | 0.2.5 | Apache-2.0/MIT | LICENSE-APACHE, LICENSE-MIT |
| pollster | 0.4.0 | Apache-2.0/MIT | LICENSE-APACHE, LICENSE-MIT |
| potential_utf | 0.1.6 | Unicode-3.0 | LICENSE |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | LICENSE-APACHE.md, LICENSE.md |
| rav1e | 0.8.1 | BSD-2-Clause | LICENSE |
| ravif | 0.13.0 | BSD-3-Clause | LICENSE |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | LICENSE-APACHE.md, LICENSE-MIT.md, LICENSE-ZLIB.md |
| sha1_smol | 1.0.1 | BSD-3-Clause | LICENSE |
| slotmap | 1.1.1 | Zlib | LICENSE |
| subtle | 2.6.1 | BSD-3-Clause | LICENSE |
| tiny-skia | 0.11.4 | BSD-3-Clause | LICENSE |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | LICENSE |
| tinystr | 0.8.5 | Unicode-3.0 | LICENSE |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | LICENSE-APACHE.md, LICENSE-MIT.md, LICENSE-ZLIB.md |
| v_frame | 0.3.9 | BSD-2-Clause | LICENSE |
| writeable | 0.6.4 | Unicode-3.0 | LICENSE |
| yoke | 0.8.4 | Unicode-3.0 | LICENSE |
| yoke-derive | 0.8.4 | Unicode-3.0 | LICENSE |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | LICENSE-APACHE, LICENSE-BSD, LICENSE-MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | LICENSE-APACHE, LICENSE-BSD, LICENSE-MIT |
| zerofrom | 0.1.8 | Unicode-3.0 | LICENSE |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | LICENSE |
| zerotrie | 0.2.5 | Unicode-3.0 | LICENSE |
| zerovec | 0.11.9 | Unicode-3.0 | LICENSE |
| zerovec-derive | 0.11.7 | Unicode-3.0 | LICENSE |
| zlib-rs | 0.6.8 | Zlib | LICENSE |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | LICENSE-APACHE, LICENSE-MIT, LICENSE-ZLIB |
| zune-inflate | 0.2.54 | MIT OR Apache-2.0 OR Zlib | See package declaration/upstream notice |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | LICENSE-APACHE, LICENSE-MIT, LICENSE-ZLIB |
