#!/usr/bin/env python3
"""Single labelled diagnostic only; does not implement conversion policy."""
import json,sys,subprocess
from run import run,BASE,sha
from pathlib import Path
label,clears,tweaks=sys.argv[1:]
assert all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in label)
source=Path('/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst')
assert sha(source)=='dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177'
raw=BASE/'output'/(label+'-raw.pst');out=BASE/'output'/(label+'.pst');assert not raw.exists() and not out.exists()
r=run('convert',source,'/output/'+raw.name,'clear:'+clears,tweaks);assert r.returncode==0,(r.stdout,r.stderr)
conversion=r.stdout
r=run('map-spike',raw,'/output/'+out.name);assert r.returncode==0,(r.stdout,r.stderr)
r=subprocess.run([sys.executable,str(Path(__file__).with_name('scan-loop-strict.py')),str(out)],capture_output=True,text=True,timeout=50);assert r.returncode in (0,1),(r.stdout,r.stderr)
data=json.loads(r.stdout);data.update(label=label,clear_tags=clears,tweaks=tweaks,conversion=conversion,output_sha256=sha(out),experimental_only=True)
out.with_suffix('.json').write_text(json.dumps(data,indent=2)+'\n');print(json.dumps(data))
