# PstOff

PstOff converts Outlook OST files into PST files, so a mailbox's cached contents can be kept, moved or archived without the account that created them.

## Language

### Files and stores

**Mailbox**:
The account's store on the mail server (Exchange, Microsoft 365 or IMAP), which an OST caches locally.
_Avoid_: Account, server

**OST**:
The local cache file Outlook keeps for a mailbox. It holds only what Outlook has synced, and Outlook opens it only through the profile that created it.
_Avoid_: Offline folder file, offline data file, Outlook data file

**PST**:
A standalone Outlook data file, not tied to any mailbox or profile, which any Outlook can open.
_Avoid_: Archive, personal folders file, Outlook data file

**Orphaned OST**:
An OST whose mailbox or Outlook profile is no longer reachable, so Outlook cannot open it or export from it.
_Avoid_: Dead OST, broken OST

**Partial cache**:
An OST that holds only part of its mailbox, because Outlook synced only recent items or only message headers.
_Avoid_: Incomplete OST, partial sync

**Item**:
One entry in a folder: a message, appointment, contact, task, note or journal entry.
_Avoid_: Email, message (when every item type is meant)

### Conversion

**Conversion**:
Producing a PST that holds the same folders and items as a source OST.
_Avoid_: Export, migration, repair

**Copy mode**:
The default way to convert: the OST is only read, never written, and the PST is a new file.
_Avoid_: Safe mode, read-only mode

**In-place mode**:
The second way to convert, where the OST file itself becomes the PST and no second copy is written. It puts the only copy at risk, so it's offered only behind a warning and a backup prompt.
_Avoid_: Overwrite mode, direct mode

**Split**:
A conversion whose output is spread across several PSTs, each kept under a size limit, because one PST would be too large.
_Avoid_: Chunk, volume, part file

**Faithful**:
Describes a conversion whose PST opens in classic Outlook, passes Outlook's Inbox Repair Tool with no errors, and differs from the OST in no item property without a recorded reason.
_Avoid_: Lossless, perfect

**Skipped item**:
An item or folder that couldn't be read from the OST, so it's left out of the PST and listed in the conversion report.
_Avoid_: Lost item, failed item

**Conversion report**:
The record a conversion produces of what was converted, what was skipped and why, and anything the user should know before relying on the PST.
_Avoid_: Log, summary
