# Source system folder identity

The writer resolves source roles once, before recovery removes unreadable nodes.
Recovery carries the resulting IDs into selection and store emission. Neither
selection nor store emission searches display names again. An EntryID for a
supported localized/custom hierarchy therefore continues to point to its source
folder, including when Split retains only system shells and selected ancestors.

## Authoritative local EntryIDs

Read the message-store PC at NID `0x21` and these exact binary properties:

| Role | Property |
| --- | --- |
| IPM subtree | `0x35e00102` |
| Wastebasket | `0x35e30102` |
| Finder/search root | `0x35e70102` |

The supported local encoding is 24 bytes: four zero flag bytes, the source
store's 16-byte `0x0ff90102` record key, and a little-endian four-byte NID.
This follows [MS-PST's EntryID/NID mapping](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/9378e8b9-7b6a-45bf-a51a-f21daf24d9ce).
This implementation does not guess a local NID from a remote Exchange EntryID
or an arbitrary last-four-byte suffix.

For every present role property, validate type, exact length, flags, provider
key, indexed normal-folder NID and a parent chain reaching root `0x122` without
a cycle. The root cannot itself be a role target. A wastebasket must descend
from the identified IPM subtree; Finder must be outside that subtree. Invalid,
foreign-store, dangling, wrong-type and conflicting references fail with
`DamagedSource` before output creation. They never fall back to an English
display name that happens to match.

The IPM subtree's actual parent supplies the output container. It can be the
root directly; an extra folder named `Root - Mailbox` is not required. A deeper
source parent chain is retained as selected ancestors. Valid source folders keep
their original NIDs and properties, including display names. Identical or
misleading English names elsewhere in the tree do not override explicit roles.

## Missing metadata and compatibility limits

Display names never establish system folder identity. There is no canonical
English-name fallback, translated-name dictionary, lowest-NID, first-folder or
"most-populated-folder" heuristic. This intentionally narrows conversion of
older inputs that lack usable identity metadata: even a canonical English
hierarchy fails safely when its source roles cannot be established. The format
can still be supported while its folder identities are unresolvable.

Missing metadata is not proof that the corresponding folder is absent. After
recovery, a missing IPM role with any retained non-root folder or Item causes
`DamagedSource`. A missing wastebasket role with readable IPM child folders,
or a missing Finder role with readable sibling candidates, also causes failure
before output creation. No readable candidate is renamed or replaced by an
empty role shell. A truly empty/recovered-away boundary can receive explicit
scaffolding under the existing recovery contract.

Generated conversion fixtures now declare their three system roles separately
from display names. The builder serializes those declarations into store
EntryIDs using its deterministic global NID sequence. Explicit-metadata tests
can replace that property set, including omitting it entirely. CLI/GUI fixture
helpers declare the same roles; they do not infer roles from English names.

## Recovery and generated scaffolding

A referenced folder with unreadable data/table remains a known source identity.
Recovery skips it and its descendants under the existing damage rules. Only
roles that were absent or whose nodes recovery actually removed receive empty
generated scaffolding. New folder NIDs are above the source high-water mark;
source skip records and scaffold differences remain explicit. This distinguishes
an unreadable identified subtree from a readable renamed subtree.

The generated names are output defaults, not identity criteria for a readable
source. `SystemFolders` is the shared internal contract for recovery, Split,
exclusions and store EntryID generation. The public API and report schema are
unchanged.

Synthetic public-API tests cover localized names with independent Microsoft
reader payload readback, English decoys, Split/exclusions, a direct-root
subtree, invalid references, unresolved healthy renamed sources and recovery
of an identified unreadable localized subtree. The tests compare source bytes
before and after conversion. No private/unknown mail inputs or Faithful checks
are part of this change.
