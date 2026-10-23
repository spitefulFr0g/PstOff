#!/usr/bin/env python3
"""Representation-only bounded trials. Each output is fresh, maps regenerated."""
import argparse,json,subprocess,sys
from pathlib import Path
from run import run,BASE,sha
HERE=Path(__file__).resolve().parent
parser=argparse.ArgumentParser();parser.add_argument('--prefix',default='clean');parser.add_argument('--source',type=Path,default=BASE/'output/q15-validated-public.pst');parser.add_argument('trials',nargs='*');args=parser.parse_args()
assert args.prefix and all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in args.prefix)
source=args.source
for trial in args.trials or ['repack','clone-all','canonical-bits','canonical-layout','canonical-all','zero-absent']:
 raw=BASE/'output'/f'{args.prefix}-{trial}-raw.pst';out=BASE/'output'/f'{args.prefix}-{trial}.pst'
 assert not raw.exists() and not out.exists()
 r=run('convert',source,'/output/'+raw.name,trial);assert r.returncode==0,(r.stdout,r.stderr)
 r=run('map-spike',raw,'/output/'+out.name);assert r.returncode==0,(r.stdout,r.stderr)
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(out)],text=True,capture_output=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 gate=json.loads(r.stdout);gate.update(trial=trial,sha256=sha(out))
 (out.with_suffix('.json')).write_text(json.dumps(gate,indent=2)+'\n')
 print(json.dumps(gate),flush=True)
