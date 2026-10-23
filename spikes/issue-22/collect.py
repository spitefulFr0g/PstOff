#!/usr/bin/env python3
"""Verify fixed run artifacts and publish only structural data, tags and hashes."""
import json,re,subprocess,sys
from pathlib import Path
from run import BASE,PUBLIC,EMPTY,run,sha
HERE=Path(__file__).resolve().parent;OUT=BASE/'output'
BASELINE=Path('/home/watermelon/.local/share/pstoff-spike21/output/q16-final-public.pst')
EXPECTED='32257b4b4c759d085bb2eb7aafaaf33847d464d2c8b22b33e3df29cb3eccd765'
assert sha(BASELINE)==EXPECTED

def checked(binary,source,*args):
 r=run(binary,source,*args);assert r.returncode==0,(binary,r.stderr[:1200]);return r.stdout.strip()
def rows_in_scan(d):
 alias=d['Alias'];assert re.fullmatch('gate-[a-f0-9]{12}',alias)
 ps='$p=Join-Path $env:TEMP "PstOffPublicSpike/'+alias+'-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
 log=json.loads(subprocess.check_output(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],text=True))
 return sorted(re.findall(r'RowID = ([0-9A-F]+)',log))
def saved(p):
 d=json.loads(p.with_suffix('.json').read_text());assert d['sha256']==sha(p)
 assert all(d[k] for k in ['ScanFinished','LogReportsFinished','ScanInputUnchanged','original_output_unchanged'])
 count=d['DiagnosticLineCount'];assert count in [0,3]
 ids=rows_in_scan(d);assert ids==([] if count==0 else ['200084','2000E4','200184'])
 d['mismatch_row_ids']=ids;d['strict_exit']=int(count!=0);return d

def gate(p):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(p)],capture_output=True,text=True,timeout=50)
 assert r.returncode in [0,1];d=json.loads(r.stdout);d['strict_exit']=r.returncode;d['sha256']=sha(p)
 assert d['ScanInputUnchanged'] and d['original_output_unchanged'];return d

trials=[('trial01-clear-all','t1',0),('trial02-clear-derived','t1:0e03001f,30130102,65c60003',3),('trial03-clear-retained','t1:0e05001f,0e170003,0e380003,66120003,66280014,67f10003',3),('trial04-clear-without-derived','t1:0e05001f,0e170003,0e380003,66120003,66280014,67f10003,30130102,0e03001f',3),('trial05-clear-common','t1:0e03001f,30130102,65c60003,0e170003,66120003,66280014',0),('trial06-clear-without-status','t1:0e03001f,30130102,65c60003,66120003,66280014',0),('trial07-clear-without-6612','t1:0e03001f,30130102,65c60003,66280014',0),('trial08-clear-pair','t1:65c60003,66280014',0),('trial09-clear-secure','t1:65c60003',3),('trial10-clear-6628','t1:66280014',3),('trial11-add-pair','t2:65c60003,66280014',0)]
report={'source_sha256':sha(PUBLIC),'baseline_sha256':sha(BASELINE),'accepted_candidate':saved(OUT/'accepted-replay.pst'),'trials':{}}
assert report['accepted_candidate']['sha256']==EXPECTED
for label,mode,count in trials:
 p=OUT/(label+'.pst');d=saved(p);assert d['DiagnosticLineCount']==count
 entry={'mode':mode,'diagnostic_only':True,'scan':d}
 if mode.startswith('t1'):
  entry['preservation']=json.loads(checked('cell-diff',BASELINE,'/output/'+p.name))
  entry['audit']=checked('audit',BASELINE,'/output/'+p.name)
 if mode.startswith('t2'):
  entry['audit']=checked('audit',BASELINE,'/output/'+p.name,'diagnostic-pair')
  entry['pc_additions']={'65c60003':3,'66280014':3}
 report['trials'][label]=entry
report['accepted_preservation']=json.loads(checked('cell-diff',BASELINE,'/output/accepted-replay.pst'))
report['accepted_audit']=checked('audit',BASELINE,'/output/accepted-replay.pst')
report['minimal_pair_preservation']=report['trials']['trial08-clear-pair']['preservation']
assert report['minimal_pair_preservation']['removed_cells_by_id']=={'65c6':3,'6628':3}
report['subobjects']=[json.loads(l) for l in checked('subobjects',BASELINE).splitlines()]
report['subobjects_source']=[json.loads(l) for l in checked('subobjects',PUBLIC).splitlines()]
report['policy_guards']={b:checked(b,PUBLIC if b in ['q12-guards','q15-guards'] else BASELINE) for b in ['q12-guards','q15-guards','q16-guards']}
report['microsoft_reader']=checked('ms-probe',OUT/'trial08-clear-pair.pst')
invalid=OUT/'control-invalid-zeros.pst'
if not invalid.exists():
 with invalid.open('xb') as f:f.write(bytes(4096))
assert invalid.read_bytes()==bytes(4096)
control_file=OUT/'controls.json'
if not control_file.exists():control_file.write_text(json.dumps({'valid_empty':gate(EMPTY),'invalid_zeros':gate(invalid)},indent=2)+'\n')
report['controls']=json.loads(control_file.read_text())
assert report['controls']['valid_empty']['sha256']==sha(EMPTY) and report['controls']['invalid_zeros']['sha256']==sha(invalid)
assert report['controls']['valid_empty']['gate']=='PASS' and report['controls']['invalid_zeros']['gate']=='FAIL'
report['independent_libpff']={'run':False,'reason':'PASS is diagnostic only. No accepted-behaviour or value-preserving writer fix passes.'}
report['verdict']='Accepted FAIL/3, byte-identical replay. Diagnostic PASS/0 requires clearing both 65c60003 and 66280014 table presences, or diagnostic PC additions. No accepted change applied.'
(HERE/'evidence').mkdir(exist_ok=True)
(HERE/'evidence/results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'verdict':report['verdict'],'minimal_pair_preservation':report['minimal_pair_preservation'],'accepted_audit':report['accepted_audit']}))
