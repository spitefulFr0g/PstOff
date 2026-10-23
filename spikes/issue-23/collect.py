#!/usr/bin/env python3
"""Fresh controls, preservation and selection guards; publish aggregate evidence only."""
import json,subprocess,sys,collections
from pathlib import Path
from run import BASE,PUBLIC,EMPTY,run,sha
HERE=Path(__file__).resolve().parent; OUT=BASE/'output'
BASELINE=Path('/home/watermelon/.local/share/pstoff-spike22/output/accepted-replay.pst')
assert sha(BASELINE)=='32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765'
def checked(binary,source,*args):
 r=run(binary,source,*args); assert r.returncode==0,(binary,r.stdout,r.stderr);return r.stdout.strip()
def gate(p):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(p)],capture_output=True,text=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 d=json.loads(r.stdout);d.update(sha256=sha(p),strict_exit=r.returncode)
 assert d['ScanInputUnchanged'] and d['original_output_unchanged'] and d['ScanFinished']
 return d
out=OUT/'q17-public.pst'; candidate=json.loads(out.with_suffix('.json').read_text())
assert candidate['sha256']==sha(out) and candidate['gate']=='PASS' and candidate['DiagnosticLineCount']==0
confirmation=json.loads((OUT/'q17-confirm.json').read_text()); assert confirmation['sha256']==sha(out) and confirmation['gate']=='PASS'
assert sha(EMPTY)=='c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'
raw=checked('cell-diff',BASELINE,'/output/q17-public.pst');(OUT/'exact-cell-preservation.jsonl').write_text(raw+'\n')
preservation=json.loads(raw.splitlines()[-1]);assert preservation['removed_cells_by_id']=={'0e03':4,'65c6':3,'6628':3}
assert preservation['changed_cells_by_id']==preservation['added_cells_by_id']=={}
provenance=json.loads((OUT/'q17-public-provenance.json').read_text());q17=[l for l in provenance if l.startswith('Q17 ')]
counts=collections.Counter((l.split('tag=')[1].split()[0],l.split('action=')[1]) for l in q17)
assert counts=={('0e03001f','keep_source_absent_default'):4,('65c60003','keep_source_absent_default'):3,('66280014','omit_missing_pc_cache'):3},counts
invalid=OUT/'control-invalid-zeros.pst'
if not invalid.exists():invalid.write_bytes(bytes(4096))
assert invalid.read_bytes()==bytes(4096)
controls={'valid_empty':gate(EMPTY),'invalid_zeros':gate(invalid)}
assert controls['valid_empty']['gate']=='PASS' and controls['valid_empty']['DiagnosticLineCount']==0
assert controls['invalid_zeros']['gate']=='FAIL' and controls['invalid_zeros']['DiagnosticLineCount']>0
report={'candidate':candidate,'confirmation':confirmation,'source_sha256':sha(PUBLIC),'baseline_sha256':sha(BASELINE),'preservation':preservation,'audit':checked('audit',BASELINE,'/output/q17-public.pst'),'q17_counts':[{'tag':t,'action':a,'count':c} for (t,a),c in sorted(counts.items())],'policy_guards':{b:checked(b,PUBLIC if b in ['q12-guards','q15-guards'] else out) for b in ['q12-guards','q15-guards','q16-guards','q17-guards']},'microsoft_reader':checked('ms-probe',out),'controls':controls,'libpff':json.loads((HERE/'evidence/libpff-comparison.json').read_text())}
assert report['source_sha256']=='b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d'
(HERE/'evidence/results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'preservation':preservation,'audit':report['audit'],'controls':controls,'guards':report['policy_guards']}))
