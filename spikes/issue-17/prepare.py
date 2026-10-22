#!/usr/bin/env python3
"""Prepare a dedicated editable checkout; never patch the pinned source."""
from pathlib import Path
import shutil,subprocess,tempfile
BASE=Path.home()/'.local/share/pstoff-spike17'
SRC=Path('/tmp/pstres-sp00nznet_pstfree/repo')
MS=Path('/tmp/pstres-src/microsoft_outlook-pst-rs/repo')
HERE=Path(__file__).resolve().parent
for repo,rev in [(SRC,'b6e2ff31592f74458f0481d4671251c92044d8f0'),(MS,'cfb721daee538acc50d2bad6faac5ce724f6037d')]:
 assert subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==rev
 assert not subprocess.check_output(['git','-C',str(repo),'diff','HEAD','--','src','crates','Cargo.toml','.cargo'],text=True).strip()
(BASE/'source').mkdir(parents=True,exist_ok=True);(BASE/'output').mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='prepare-',dir=BASE/'source') as tmp:
 tmp=Path(tmp);shutil.copytree(SRC/'src',tmp/'src')
 for name in ['Cargo.toml','Cargo.lock','LICENSE']:shutil.copy(SRC/name,tmp/name)
 subprocess.run(['patch','-p1','-i',str(HERE/'patches/pstfree-writer.patch')],cwd=tmp,check=True)
 dest=BASE/'source/pstfree'
 if dest.exists():
  for f in tmp.rglob('*'):
   if f.is_file():assert (dest/f.relative_to(tmp)).read_bytes()==f.read_bytes(),f'Local edits differ: {f.relative_to(tmp)}'
 else:shutil.copytree(tmp,dest)
print('Pinned sources clean; dedicated patched copy ready.')
