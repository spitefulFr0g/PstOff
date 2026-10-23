#!/usr/bin/env python3
"""Fetch pinned public inputs only; never execute downloaded content."""
from pathlib import Path
import tempfile,subprocess,json
BASE=Path('/home/watermelon/.local/share/pstoff-spike21');BASE.mkdir(exist_ok=True)
config=BASE/'sources.json'
if config.exists():
 print('Reusing issue-21 source configuration.');raise SystemExit(0)
prior=Path('/home/watermelon/.local/share/pstoff-spike20/sources.json')
if prior.exists():
 existing=json.loads(prior.read_text())
 if all(Path(p).is_dir() for p in existing.values()):
  config.write_text(json.dumps(existing,indent=2)+'\n');print('Reusing existing pinned public sources.');raise SystemExit(0)
roots={}
for name,url,rev in [('pstfree','https://github.com/sp00nznet/pstfree','b6e2ff31592f74458f0481d4671251c92044d8f0'),('microsoft','https://github.com/microsoft/outlook-pst-rs','cfb721daee538acc50d2bad6faac5ce724f6037d'),('java','https://github.com/rjohnsondev/java-libpst','f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26')]:
 repo=Path(tempfile.mkdtemp(prefix='pstoff21-'+name+'-'))/'repo';roots[name]=str(repo)
 git=['git','-c','http.userAgent=PstOff-research (https://github.com/spitefulFr0g/PstOff)']
 subprocess.run(git+['clone','--quiet','--depth','1',url,str(repo)],check=True)
 subprocess.run(git+['-C',str(repo),'fetch','--quiet','--depth','1','origin',rev],check=True)
 subprocess.run(['git','-C',str(repo),'checkout','--quiet',rev],check=True)
config.write_text(json.dumps(roots,indent=2)+'\n')
