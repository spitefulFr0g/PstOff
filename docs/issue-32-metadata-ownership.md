# Issue #32: inspection and recovery ownership slice

Base: `73359c5 -> 6691945 -> 55a34d4`. Generated fixtures only.

## Changed owners

- Conversion inspection borrows the already classified, deny-writer source and
  its node inventory. It no longer opens another Pst or builds another node list.
- Recovery takes ownership of that node vector and filters it in place before
  adding required system folders. It no longer clones every retained source node.
- Property traversal spools source heap pages, retaining one allocation per BTH
  depth (maximum 33 levels), rather than calling `node_blocks` for the entire PC.
  A strictly increasing leaf-key cursor detects duplicate/out-of-order properties
  without a property-count map. Each unselected subnode value is CRC/length
  validated through `node_chunks`, then discarded. Local subnode identity stays
  in the original context; relocation still emits the original local reference.
- Inspection selects size/name operands; recovery validation discards PC cells;
  evidence selects its exact operands. Scalar subnode operands retain at most
  eight bytes while validating the rest. Attachment-data presence requires no
  copy of the attachment payload. Folder report mapping reads only message size.
- Inspection no longer builds item/table membership vectors per folder. Cycle
  detection uses Floyd's parent walk instead of two folder membership sets.
- Selection uses its sorted Item slice for membership and scans retained nodes
  instead of constructing two further Item/node membership sets. Candidate parts
  collect only retained nodes, and do not copy report evidence/skipped vectors.
  Transitioning incomplete evidence to checked moves the reference vector.

These changes trade additional source scans/spool I/O for reduced ownership.
Floyd's walk and retained-node membership scans can be quadratic in hostile or
large hierarchies. A disk-backed source/parent index remains preferable.

## Remaining owners: no ceiling claim

The 16 MiB shared-reservation and 256 MiB private-commit gates remain OPEN.
There is no shared reservation accounting or separate-process end-to-end peak
measurement in this slice. Allocation-counter tests are local regressions only.

- `Pst::nodes` still returns the full source `Vec<Node>`; recovery owns that
  same vector, Selection retains a separate sorted Item vector, and each candidate
  part owns a filtered node vector. These are count-dependent owners.
- Recovery `omitted`, `bad_folders`, and `lost`, Selection exclusions/folders,
  `system::resolve`'s folder BTreeMap, and writer copies of omitted/created
  metadata still grow with node/folder count. Recovery's named-header-ID vector
  and its evidence/unchecked vectors also remain materialized.
- Inspection's public folder tree, `flat`, parent/child indexes, assembly stack,
  and built-folder map grow with folder count. The public API needs a deliberate
  streaming/paged alternative before claiming bounded whole-process metadata.
- `Pst::subnodes` still materializes each namespace's HashMap and visited set;
  reader Item traversal retains a visited set/pending vector, and recovery's
  external-reference traversal retains a pending vector. Wide namespaces remain
  unbounded even though payload bytes are discarded.
- Selected folder display names and external attachment target strings still
  materialize. Exact existing code-page decoding and target-policy behavior is
  preserved; a bounded streaming decoder/predicate is still needed. Malformed
  selected scalar heap operands can retain one heap allocation (at most one
  decoded source block), not a whole subnode stream.
- Relocation and selected writer reads still return complete property descriptor
  maps; inline heap values remain in those maps. `context::properties` and system/
  store metadata paths can still request full selected payloads. Q16 selected
  operands and row spool materialization also require a further descriptor pass.
- Named-property maps, public report evidence/differences/skips/folder-part vectors,
  JSON serialization and text reports remain count-dependent. Earlier writer,
  table, planner and finalizer improvements do not remove these owners.

No private mail, SCANPST, Outlook acceptance or Faithful gate is exercised here.

## Validation

All Rust commands ran through `python scripts/sandbox-cargo.py` on Windows.

- `fmt -p pstoff-engine -- --emit stdout`: all changed Rust files were updated
  from, and compared byte-for-byte with, the Sandbox formatter output.
- `clippy --locked -p pstoff-engine --all-targets -- -D warnings`: passed.
- `test --locked -p pstoff-engine`: 74 passed, one deliberately ignored
  generated libpff-export test; no failures.
- New regressions use 64,000/700,000-byte unselected properties and 1/8/16 Item
  folders, check original node identity, read back converted property bytes,
  compare the complete generated source bytes and exercise cancelled reads.
  The local allocation delta for the unselected property read is below 256 KiB;
  fixture construction and independent readback are outside that local counter.

No whole-workspace, GUI or external-reader acceptance is claimed by these checks.
