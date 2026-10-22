#!/usr/bin/env python3
"""DISPOSABLE diagnostic trials, never conversion policy. PUBLIC only."""
import json,subprocess,sys
from pathlib import Path
from run import run,BASE,sha
HERE=Path(__file__).resolve().parent
SOURCE=Path('/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst')
assert sha(SOURCE)=='dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177'
for tag in sys.argv[1:]:
 mode=tag;zero=tag.split("+zero:")[1] if "+zero:" in tag else None;tag=tag.split("+zero:")[0];tag=tag.removeprefix("clear:");flags=tag.endswith("+flags");tag=tag.removesuffix("+flags");align=tag.endswith("+align0042");tag=tag.removesuffix("+align0042")
 assert tag=='status' or all(len(t)==4 and all(c in '0123456789abcdef' for c in t) for t in tag.split(','))
 label=tag+('-aligned' if align else '')+('-flags' if flags else '')+('-zero-'+zero if zero else '')
 raw=BASE/'output'/(('ceb-' if mode.startswith('clear:') else 'mask-')+label+'-raw.pst');out=BASE/'output'/(('ceb-' if mode.startswith('clear:') else 'mask-')+label+'.pst')
 assert not raw.exists() and not out.exists()
 r=run('convert',SOURCE,'/output/'+raw.name,mode.split('+zero:')[0].removesuffix('+align0042').removesuffix('+flags'),*(['align0042'] if align else ['flags'] if flags else ['zero:'+zero] if zero else []));assert r.returncode==0,(r.stdout,r.stderr)
 r=run('map-spike',raw,'/output/'+out.name);assert r.returncode==0,(r.stdout,r.stderr)
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(out)],capture_output=True,text=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 data=json.loads(r.stdout);data.update(tag=tag,output_sha256=sha(out),experimental_only=True)
 (out.with_suffix('.json')).write_text(json.dumps(data,indent=2)+'\n')
 print(json.dumps(data),flush=True)
