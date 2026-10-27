//! Writes the generated OSTs the Windows Sandbox smoke run opens
//! (`scripts/gui-smoke.py`). Ignored by default; run it explicitly.
mod support;

use std::path::Path;
use support::*;

#[test]
#[ignore = "writes smoke-run fixtures into the target folder"]
fn write_gui_smoke_fixtures() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-smoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in [
        ("1-mailbox.ost", mailbox()),
        ("2-archive.ost", mailbox()),
        ("3-older-ansi.ost", unsupported(14, 0)),
        ("4-damaged.ost", vec![0; 512]),
    ] {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}
