#!/usr/bin/env python3
"""Twelve bounded hypotheses, explicitly diagnostic; requires fresh prefix."""
import argparse,json,subprocess,sys,shutil
from pathlib import Path
from run import BASE,ROOTS,run
p=argparse.ArgumentParser();p.add_argument('--prefix',required=True);a=p.parse_args()
assert __import__('re').fullmatch('[a-z0-9-]+',a.prefix)
out=BASE/'output';donor=out/'control-dist-list.pst'
if not donor.exists():
 with donor.open('xb') as f:f.write((Path(ROOTS['java'])/'src/test/resources/dist-list.pst').read_bytes())
if not (out/'donor-columns.bin').exists():
 r=run('donor-schema',donor,'/output/donor-columns.bin');assert r.returncode==0,(r.stdout,r.stderr)
trials=[('subject-only','only:0037001f'),('sender-only','only:0042001f'),('conversation-only','only:30130102'),('sender-conversation','only:0042001f,30130102'),('display-only','only:0e03001f'),('secure-submit-only','only:65c60003'),('hasattach-only','only:0e1b000b'),('flags-only','only:0e070003'),('size-only','only:0e080003'),('donor-shape','shape'),('clear-local-cache','diagnostic-clear:66120003,66280014'),('clear-local-status','diagnostic-clear:0e170003,66120003,66280014')]
for i,(label,mode) in enumerate(trials,1):
 label=f'{a.prefix}-trial{i:02d}-{label}'
 r=subprocess.run([sys.executable,str(Path(__file__).with_name('reproduce.py')),label,mode],text=True,capture_output=True)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 d=json.loads(r.stdout);print(json.dumps({'trial':i,'mode':mode,'result':d}),flush=True)
 if d['gate']=='PASS':raise SystemExit('Clean diagnostic candidate: stop and assess policy/preservation before adopting.')
print('Bound reached; all trials failed. No minimal sufficient fix demonstrated.')
