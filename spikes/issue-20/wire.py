"""Apply the scoped Q15 policy and representation-only trial hook to a fresh copy."""
from pathlib import Path
import shutil
HERE=Path(__file__).resolve().parent
def apply(root):
 for name in ['q15.rs','q20.rs']:shutil.copy(HERE/name,root/name)
 p=root/'convert.rs';s=p.read_text()
 s=s.replace('pub mod q12;', 'pub mod q12;\n#[path = "q15.rs"]\npub mod q15;\n#[path = "q20.rs"]\npub mod q20;')
 s=s.replace('    q12: std::collections','    q15: q15::Plan,\n    q12: std::collections')
 s=s.replace('            q12: Default','            q15: Default::default(),\n            q12: Default')
 s=s.replace('let mut blocks = pst.node_blocks(src)?;', 'let mut blocks = q20::blocks(src,pst.node_blocks(src)?);')
 anchor='        for b in blocks {'
 s=s.replace(anchor,'''        if let Some(props) = what.strip_prefix("node 0x").and_then(|s| u32::from_str_radix(s,16).ok()).and_then(|nid| self.q15.get(&nid)) {
            blocks = q15::promote(blocks, props)?;
        }
'''+anchor,1)
 s=s.replace('    if std::env::var_os("PSTOFF_Q12")','    o.q15 = q15::plan(pst, nodes)?;\n    if std::env::var_os("PSTOFF_Q12")')
 p.write_text(s)
