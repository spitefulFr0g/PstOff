#!/usr/bin/env python3
"""Replay the completed adaptive T1 bisection and corroborating diagnostic T2 pair.
This fixed sequence records the actual final round, not a generic unattended search.
"""
import argparse,json,subprocess,sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--prefix',required=True);a=p.parse_args()
assert __import__('re').fullmatch('[a-z0-9-]+',a.prefix)
trials=[('clear-all','t1'),('clear-derived','t1:0e03001f,30130102,65c60003'),('clear-retained','t1:0e05001f,0e170003,0e380003,66120003,66280014,67f10003'),('clear-without-derived','t1:0e05001f,0e170003,0e380003,66120003,66280014,67f10003,30130102,0e03001f'),('clear-common','t1:0e03001f,30130102,65c60003,0e170003,66120003,66280014'),('clear-without-status','t1:0e03001f,30130102,65c60003,66120003,66280014'),('clear-without-6612','t1:0e03001f,30130102,65c60003,66280014'),('clear-pair','t1:65c60003,66280014'),('clear-secure','t1:65c60003'),('clear-6628','t1:66280014'),('add-pair','t2:65c60003,66280014')]
for i,(label,mode) in enumerate(trials,1):
 r=subprocess.run([sys.executable,str(Path(__file__).with_name('reproduce.py')),f'{a.prefix}-trial{i:02d}-{label}',mode],capture_output=True,text=True)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 print(json.dumps({'trial':i,'mode':mode,'scan':json.loads(r.stdout)}),flush=True)
