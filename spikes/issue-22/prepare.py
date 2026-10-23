#!/usr/bin/env python3
"""Pinned source preparation. Replays refuse to overwrite differing local edits."""
from pathlib import Path
import shutil,subprocess,tempfile,json
from wire import apply
BASE=Path('/home/watermelon/.local/share/pstoff-spike22');HERE=Path(__file__).resolve().parent
roots=json.loads((BASE/'sources.json').read_text())
SRC=Path(roots['pstfree']);MS=Path(roots['microsoft'])
for repo,rev in [(SRC,'b6e2ff31592f74458f0481d4671251c92044d8f0'),(MS,'cfb721daee538acc50d2bad6faac5ce724f6037d')]:
 assert subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==rev
 assert not subprocess.check_output(['git','-C',str(repo),'diff','HEAD','--','src','crates','Cargo.toml','.cargo'],text=True).strip()
(BASE/'source').mkdir(parents=True,exist_ok=True);(BASE/'output').mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='prepare-',dir=BASE/'source') as tmp:
 tmp=Path(tmp);shutil.copytree(SRC/'src',tmp/'src')
 for name in ['Cargo.toml','Cargo.lock','LICENSE']:shutil.copy(SRC/name,tmp/name)
 subprocess.run(['patch','-p1','-i',str(HERE/'patches/pstfree-writer.patch')],cwd=tmp,check=True)
 apply(tmp/'src')
 dest=BASE/'source/pstfree-final'
 if dest.exists():
  assert sorted(str(f.relative_to(tmp)) for f in tmp.rglob('*') if f.is_file())==sorted(str(f.relative_to(dest)) for f in dest.rglob('*') if f.is_file()),'Prepared file set differs'
  for f in tmp.rglob('*'):
   if f.is_file():assert (dest/f.relative_to(tmp)).read_bytes()==f.read_bytes(),f'Local edits differ: {f.relative_to(tmp)}'
 else:shutil.copytree(tmp,dest)
p=HERE/'probe/Cargo.toml';s=p.read_text();import re
s=re.sub(r'outlook-pst = \{ path = "[^"]+" \}', 'outlook-pst = { path = "'+str(MS/'crates/pst')+'" }', s);p.write_text(s)
print('Pinned clean source + Q12 + accepted Q15 + Q16 row rebuild ready.')
