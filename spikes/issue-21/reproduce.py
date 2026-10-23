#!/usr/bin/env python3
"""Q16 candidate (diagnostic modes only when explicitly selected); exit zero only for the unchanged strict clean gate."""
import json,re,sys,subprocess
from pathlib import Path
from run import run,BASE,PUBLIC,sha
label=sys.argv[1] if len(sys.argv)>1 else 'q16-rebuilt-public'
assert re.fullmatch('[a-z0-9-]+',label)
raw=BASE/'output'/(label+'-raw.pst');out=BASE/'output'/(label+'.pst')
assert not raw.exists() and not out.exists()
r=run('convert',PUBLIC,'/output/'+raw.name,sys.argv[2] if len(sys.argv)>2 else 'all',flags=('PSTOFF_TC','PSTOFF_HEADER','PSTOFF_ALIAS','PSTOFF_Q12'));assert r.returncode==0,(r.stdout,r.stderr)
provenance=[l for l in r.stdout.splitlines() if l.startswith(('Q15 ','Q12 ','Q16'))]
assert len([l for l in provenance if l.startswith('Q15 ') and 'added=2' in l])==17
assert not any('unresolved' in l for l in provenance)
(BASE/'output'/(label+'-provenance.json')).write_text(json.dumps(provenance,indent=2)+'\n')
r=run('map-spike',raw,'/output/'+out.name);assert r.returncode==0,(r.stdout,r.stderr)
r=subprocess.run([sys.executable,str(Path(__file__).with_name('scan-loop-strict.py')),str(out)],capture_output=True,text=True,timeout=50)
assert r.returncode in (0,1),(r.stdout,r.stderr)
data=json.loads(r.stdout);data.update(path=str(out),sha256=sha(out))
out.with_suffix('.json').write_text(json.dumps(data,indent=2)+'\n');print(json.dumps(data));sys.exit(r.returncode)
