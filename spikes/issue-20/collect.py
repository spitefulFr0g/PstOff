#!/usr/bin/env python3
"""Sanitized aggregates/identities only; raw values and scan logs never published."""
import json,re,subprocess,sys
from pathlib import Path
from run import run,BASE,PUBLIC,EMPTY,sha
HERE=Path(__file__).resolve().parent
OUT=BASE/'output';CANDIDATE=OUT/'q15-validated-public.pst'
Q12=Path('/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst')
def scan(p):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(p)],text=True,capture_output=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 d=json.loads(r.stdout);assert (r.returncode==0)==(d['gate']=='PASS')
 assert d['original_output_unchanged'] and d['ScanInputUnchanged']
 d['strict_exit']=r.returncode;d['sha256']=sha(p)
 alias=d['Alias'];assert re.fullmatch('gate-[a-f0-9]{12}',alias)
 ps='$p=Join-Path $env:TEMP "PstOffPublicSpike/'+alias+'-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
 log=json.loads(subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],capture_output=True,text=True,check=True).stdout)
 d['mismatching_row_ids']=sorted(re.findall(r'RowID = ([0-9A-F]+)',log))
 d['xblock_invalid_total']=len(re.findall('XBLOCK has invalid cbTotal',log))
 return d
def checked(binary,path,*args):
 r=run(binary,path,*args);assert r.returncode==0,(binary,r.stdout[:1000],r.stderr[:1000]);return r.stdout.strip()
def decoded(p):
 lines=checked('pc-probe',p).splitlines();assert not any(l.startswith(('PC_ERROR','TC_ERROR')) for l in lines)
 return {' '.join(l.split(' ')[:2]):l for l in lines}
def delta(a,b):
 return {'unchanged_existing_property_values':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('VALUE ')), 'unchanged_decoded_rows':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('TCROW ')), 'changed_keys':sorted(k for k in a.keys()&b.keys() if a[k]!=b[k]),'removed_keys':sorted(a.keys()-b.keys()),'added_keys':sorted(b.keys()-a.keys())}
assert sha(Q12)=='dc91ddc7b91affe0bd71afaa87bb97a1c5afe8ad61a016b5d12b161cae286177'
assert sha(EMPTY)=='c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'
report={'scope':'PUBLIC only; Q12 and Q15 accepted; representation trials preserve item/table values','source_sha256':sha(PUBLIC),'baseline_q12':scan(Q12),'candidate':scan(CANDIDATE),'provenance':json.loads((OUT/'q15-validated-public-provenance.json').read_text()),'policy_guards':checked('q15-guards',CANDIDATE),'output_path':str(CANDIDATE)}
assert report['baseline_q12']['DiagnosticLineCount']==17
normal=['200084','2000E4','200184'];assert report['candidate']['DiagnosticLineCount']==3 and report['candidate']['mismatching_row_ids']==normal
report['candidate_audit']=checked('audit',Q12,'/output/'+CANDIDATE.name)
report['source_stream_shapes']=checked('stream-shapes',PUBLIC).splitlines()
report['candidate_layout']=checked('layout',CANDIDATE).splitlines()
report['source_reference_observations']=[l for l in checked('layout',PUBLIC).splitlines() if l.startswith(('TABLE ','COL 30130102','COL 001a001f'))]
a=decoded(Q12);b=decoded(CANDIDATE);report['q15_delta']=delta(a,b)
d=report['q15_delta'];assert d['unchanged_existing_property_values']==1516 and d['unchanged_decoded_rows']==67 and not d['removed_keys'];assert len(d['added_keys'])==34 and all(k.startswith('VALUE ') and k[-4:] in ('65e2','65e3') for k in d['added_keys']);assert not any(k.startswith(('VALUE ','TCROW ')) for k in d['changed_keys'])
report['microsoft_reader']=checked('ms-probe',CANDIDATE)
report['trials']={}
for trial in ['repack','clone-all','canonical-bits','canonical-layout','canonical-all','zero-absent','move-row-matrix','move-row-index','cells-next-page','cells-next-page-padded']:
 p=OUT/f'clean-{trial}.pst';gate=scan(p);d=delta(b,decoded(p));assert not any(d[k] for k in ['changed_keys','removed_keys','added_keys']),trial
 audit=checked('audit',CANDIDATE,'/output/'+p.name)
 if trial=='cells-next-page':assert gate['DiagnosticLineCount']==21 and gate['xblock_invalid_total']==3
 else:assert gate['DiagnosticLineCount']==3 and gate['mismatching_row_ids']==normal
 report['trials'][trial]={'strict_scan':gate,'preservation':d,'audit':audit,'interpretation':'introduced invalid XBLOCK totals; excluded as evidence about original mismatch' if trial=='cells-next-page' else 'no original mismatch removed'}
invalid=OUT/'strict-control-invalid-zeros.pst'
if not invalid.exists():invalid.write_bytes(bytes(4096))
assert invalid.read_bytes()==bytes(4096)
report['controls']={'valid_empty':scan(EMPTY),'invalid_zeros':scan(invalid)}
assert report['controls']['valid_empty']['gate']=='PASS' and report['controls']['invalid_zeros']['gate']=='FAIL'
report['verdict']='FAIL/3. Q15 accepted and implemented. No cause isolated for normal rows after bounded representation trials. Separate synthetic writer defect: non-final short HN blocks yield invalid XBLOCK cbTotal; padding preserves allocations but leaves original FAIL/3. No new meaningful property/cache policy applied.'
(HERE/'evidence/results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'candidate':report['candidate'],'controls':{k:v['gate'] for k,v in report['controls'].items()},'trial_count':len(report['trials']),'verdict':report['verdict']},indent=2))
