#!/usr/bin/env python3
"""Verify prior diagnosis candidates, controls and sanitized evidence; not acceptance."""
import json,re,subprocess,sys
from pathlib import Path
from run import run,BASE,PUBLIC,EMPTY,sha
HERE=Path(__file__).resolve().parent
PREFIX=(sys.argv[1]+'-') if len(sys.argv)>1 else ''
SOURCE=Path('/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst')
def scan(p):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(p)],capture_output=True,text=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 d=json.loads(r.stdout);assert (r.returncode==0)==(d['gate']=='PASS');return d
def checked(binary,path,*args):
 r=run(binary,path,*args);assert r.returncode==0,(r.stdout,r.stderr);return r.stdout.strip()
def log_info(alias):
 assert re.fullmatch('gate-[a-f0-9]{12}',alias)
 ps='$p=Join-Path $env:TEMP "PstOffPublicSpike/'+alias+'-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
 r=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],capture_output=True,text=True,check=True)
 log=json.loads(r.stdout)
 return {'mismatching_row_ids':sorted(re.findall(r'RowID = ([0-9A-F]+)',log)), 'missing_modification_time':len(re.findall('Missing PR_LAST_MODIFICATION_TIME',log,re.I)), 'table_object_mismatch':len(re.findall("row doesn.t match sub-object",log,re.I))}
assert sha(SOURCE)=='dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177'
report={'scope':'PUBLIC only, subscription Codex, diagnostic only; Q12 remains sole accepted item reconstruction','baseline':scan(SOURCE),'trials':{}}
report['baseline'].update(log_info(report['baseline']['Alias']))
labels={'minimal-promote-pair':16,'minimal-promote-key':17,'minimal-promote-pcl':17,'minimal-pair':16,'minimal-key':17,'minimal-pcl':17,'promote-sync':3,'clear-pair':3,'pair-align-both':3,'pair-all-defaults':3,'pair-promote-cn':3,'pair-align-defaults-flags':3}
for label,count in labels.items():
 out=BASE/'output'/(PREFIX+label+'.pst');d=json.loads(out.with_suffix('.json').read_text());assert d['output_sha256']==sha(out)
 gate=scan(out);assert gate['DiagnosticLineCount']==count and gate['ScanFinished'] and gate['ScanInputUnchanged'] and gate['original_output_unchanged'];gate.update(log_info(gate['Alias']))
 report['trials'][label]={'sha256':sha(out),'size':out.stat().st_size,'strict_scan':gate,'experimental_only':True}
base=set(report['baseline']['mismatching_row_ids']);mini=set(report['trials']['minimal-pair']['strict_scan']['mismatching_row_ids']);assert base-mini=={'100028'} and not mini-base
for label in ['minimal-key','minimal-pcl','minimal-promote-key','minimal-promote-pcl']:assert set(report['trials'][label]['strict_scan']['mismatching_row_ids'])==base
assert base-set(report['trials']['minimal-promote-pair']['strict_scan']['mismatching_row_ids'])=={'100028'}
normal={'200084','2000E4','200184'}
for label in ['promote-sync','clear-pair']:assert set(report['trials'][label]['strict_scan']['mismatching_row_ids'])==normal
candidate=BASE/'output'/(PREFIX+'promote-sync.pst')
report['diagnostic_promotion_preservation']=checked('audit',SOURCE,'/output/'+candidate.name)
report['microsoft_reader']=checked('ms-probe',candidate)
report['sync_shapes']=checked('sync-shape',SOURCE)
def decoded(p):return {' '.join(l.split(' ')[:2]):l for l in checked('pc-probe',p).splitlines()}
a=decoded(SOURCE);b=decoded(candidate)
report['promotion_decoded_delta']={'unchanged_existing_values':sum(a[k]==b[k] for k in a if k.startswith('VALUE ')), 'changed_keys':sorted(k for k in a.keys()&b.keys() if a[k]!=b[k]),'removed_keys':sorted(a.keys()-b.keys()),'added_keys':sorted(b.keys()-a.keys())}
d=report['promotion_decoded_delta'];assert not d['removed_keys'];assert not [k for k in d['changed_keys'] if k.startswith(('VALUE ','TCROW '))];assert len(d['added_keys'])==34 and all(k.startswith('VALUE ') and k[-4:] in ('65e2','65e3') for k in d['added_keys'])
invalid=BASE/'output/diagnosis-invalid-zeros.pst'
if not invalid.exists():invalid.write_bytes(bytes(4096))
assert invalid.read_bytes()==bytes(4096)
report['controls']={'valid_empty':scan(EMPTY),'invalid_zeros':scan(invalid)}
assert report['controls']['valid_empty']['gate']=='PASS' and report['controls']['invalid_zeros']['gate']=='FAIL'
report['verdict']='POLICY BLOCK: 14 associated-item failures causally isolated to missing PC 65e2/65e3 versus cached values. Three normal-item failures remain technically unresolved. No accepted fix or zero-gate output.'
(HERE/'evidence/results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'baseline_diagnostics':report['baseline']['DiagnosticLineCount'],'diagnostic_promoted_diagnostics':report['trials']['promote-sync']['strict_scan']['DiagnosticLineCount'],'controls':{k:v['gate'] for k,v in report['controls'].items()},'verdict':report['verdict']},indent=2))
