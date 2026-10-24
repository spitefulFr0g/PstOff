# Writer integration checkpoint

This writer is rebuilt in the engine, without calling the fork's conversion
routine or importing spike patches. `convert` currently supports `Split::Off`
and no exclusions. Planner options explicitly return NotImplemented until the
conversion task integrates them. Conversion completion does not assert Faithful.

The writer emits v23 Unicode pages with permutative encoding and no password,
fresh BBT/NBT and recursively rebuilt subnodes, property heaps and tables.
Microsoft's unmodified pinned library regenerates allocation maps. Non-final
heap pages are padded to 8176 bytes before XBLOCK emission, so allocation
addressing, physical lengths and cbTotal agree. Raw property bytes are retained;
large allocations become payload subnodes. Contents rows use exact Item property
tags; other cached cells remain. Q17 omits only 66280014 without a matching PC
property, and absent computed defaults are never introduced. Required contents
column descriptors have absent cells. Differences identify changed store and
contents properties with their source and reason. The temporary is owned only
after create_new, removed on writer failure, and published without overwriting.

The essential store policy resolves the public fixture's system folder hierarchy.
Missing or ambiguous system folders and record keys fail explicitly. Localized
Deleted Items resolution beyond this demonstrated fixture is not validated.
Full Q12/Q15 cache validation belongs to the policy task and is not implemented
here. Source identity is hashed through a clone of the retained read-only handle.

Run `cargo fetch` with the documented research User-Agent, then `scripts/check`.
The regular checks use generated fixtures. The explicit public check is:

```bash
python3 scripts/fetch-public-writer-fixture.py
scripts/cargo-sandboxed test -p pstoff-engine --test writer public_fixture -- --ignored --nocapture
python3 scripts/scanpst-check.py target/writer-fixtures/output/public.pst
python3 scripts/libpff-check.py compare target/writer-fixtures/example-2013.ost target/writer-fixtures/output/public.pst
```

The public check refuses an existing output. Preserve earlier evidence before
removing that generated candidate for a rerun. Fixtures and candidates are local
ignored artifacts; no mail content belongs in committed logs.

Independent checks currently demonstrate 45 folders, 17 Item row identities,
three normal message PCs and three recipient rows on the public fixture. The
Microsoft message API rejects the 14 associated Items without subnode trees;
libpff separately checks their reachable property graph. The public fixture has
no attachment or embedded-message payloads. A generated test independently
verifies a 40,000-byte attachment after rebuilding and externalizing its heap
allocation. Embedded-message positive readback is still required, as are the
broader synthetic Faithful matrix and backend/frontend integration reviews.

The public strict SCANPST gate remains FAIL (51 diagnostic lines), including
17 missing modification times and row/Item disagreement. Q12/Q15 must integrate
before repeating that milestone; no first-scanpst or writer acceptance PASS is
claimed. No subsequent engine expansion should treat this checkpoint as passing
the first milestone. Libpff's reachable-graph PASS is insufficient for Faithful;
manual classic Outlook checks also remain required.
