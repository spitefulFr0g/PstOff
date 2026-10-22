#!/usr/bin/env python3
"""Public conversion matrix and red-capable SCANPST/output regressions."""
import json,subprocess,sys,hashlib,re
from pathlib import Path
from run import run,BASE,PUBLIC,EMPTY,sha
HERE=Path(__file__).resolve().parent
OUT=BASE/'output'
assert sha(EMPTY)=='c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'
report={'public_source_sha256':sha(PUBLIC),'runs':{}}
variants={'baseline':(), 'schema':('PSTOFF_TC',), 'header':('PSTOFF_HEADER',), 'schema-header':('PSTOFF_TC','PSTOFF_HEADER'), 'alias':('PSTOFF_ALIAS',), 'schema-header-exclusive':('PSTOFF_TC','PSTOFF_HEADER','PSTOFF_ALIAS')}
expected={'baseline':326,'schema':93,'header':325,'schema-header':92,'alias':302,'schema-header-exclusive':34}
def scan(path):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop.py'),str(path)],capture_output=True,text=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 return json.loads(r.stdout)
for label,flags in variants.items():
 r=run('convert',PUBLIC,f'/output/{label}-raw.pst',flags=flags);assert r.returncode==0,(label,r.stdout,r.stderr)
 assert 'nodes=308 dropped=0 problems=[]' in r.stdout
 r=run('map-spike',OUT/f'{label}-raw.pst',f'/output/{label}.pst');assert r.returncode==0,(r.stdout,r.stderr)
 p=OUT/f'{label}.pst';s=scan(p)
 assert s['DiagnosticLineCount']==expected[label] and s['gate']=='FAIL' and s['original_output_unchanged'],s
 report['runs'][label]={'sha256':sha(p),'size':p.stat().st_size,'scan':s}
 print(label,s['DiagnosticLineCount'],flush=True)
(OUT/'invalid-zeros.pst').write_bytes(bytes(4096))
report['controls']={'valid_empty':scan(EMPTY),'invalid_zeros':scan(OUT/'invalid-zeros.pst')}
assert report['controls']['valid_empty']['gate']=='PASS'
assert report['controls']['invalid_zeros']['gate']=='FAIL'
for label in ['baseline','schema-header-exclusive']:
 r=run('audit',PUBLIC,f'/output/{label}.pst');report['runs'][label]['structural_audit']={'exit':r.returncode,'summary':r.stdout.strip()}
 assert (r.returncode==0)==(label!='baseline')
 assert 'identical_table_rows=67' in r.stdout and 'byte_identical_subnodes=136' in r.stdout
for label in ['schema-header-exclusive']:
 r=run('ms-probe',OUT/f'{label}.pst');assert r.returncode==0,r.stderr
 report['runs'][label]['microsoft_reader']=r.stdout.strip()
# Keep raw dumps local; publish only aggregate/key-level diagnostics.
source=run('pc-probe',PUBLIC);result=run('pc-probe',OUT/'schema-header-exclusive.pst')
assert source.returncode==result.returncode==0
key=lambda x:' '.join(x.split(' ')[:2])
a={key(x):x for x in source.stdout.splitlines()};b={key(x):x for x in result.stdout.splitlines()}
report['preservation']={'identical_property_values':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('VALUE ')), 'identical_table_rows':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('TCROW ')), 'changed_keys':sorted(k for k in a.keys()&b.keys() if a[k]!=b[k]),'missing_keys':sorted(a.keys()-b.keys()),'added_keys':sorted(b.keys()-a.keys())}
assert report['preservation']['identical_property_values']==1493
assert report['preservation']['identical_table_rows']==67
assert report['preservation']['missing_keys']==[]
assert report['preservation']['changed_keys']==['PC 00000021','VALUE 00000021:3001','VALUE 00000021:67ff']
# Classify local Windows log; never publish paths, names or raw values.
h=sha(OUT/'schema-header-exclusive.pst')[:12]
ps=f'$p=Join-Path $env:TEMP "PstOffPublicSpike/gate-{h}-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
r=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],capture_output=True,text=True,check=True)
log=json.loads(r.stdout)
report['remaining']={name:len(re.findall(pattern,log,re.I)) for name,pattern in {'missing_modification_time':'Missing PR_LAST_MODIFICATION_TIME','table_object_mismatch':"row doesn.t match sub-object",'missing_column':'missing required column','failed_addref':'failed addref','rgbFP':'Problems with rgbFP','invalid_content_count':'Invalid PR_CONTENT_COUNT'}.items()}
assert report['remaining']=={'missing_modification_time':17,'table_object_mismatch':17,'missing_column':0,'failed_addref':0,'rgbFP':0,'invalid_content_count':0}
# Source-backed timestamp option: count only, without publishing actual timestamps.
message_ids=[line.split()[1] for line in source.stdout.splitlines() if line.startswith('PC ') and ('type=4 ' in line or 'type=8 ' in line)]
source_rows=[line for line in source.stdout.splitlines() if line.startswith('TCROW ')]
coverage=0;conflicts=0
for nid in message_ids:
 assert 'VALUE '+nid+':3008' not in a
 times={match.group(1) for line in source_rows if line.split()[1].split(':')[1]==nid for match in re.finditer(r'12296: Time\((\d+)\)',line)}
 coverage+=len(times)==1;conflicts+=len(times)>1
report['source_timestamp_option']={'message_pcs':len(message_ids),'pcs_missing_modification_time':len(message_ids),'messages_with_unambiguous_cached_modification_time':coverage,'conflicting_cached_times':conflicts,'promotion_applied':False}
assert len(message_ids)==coverage==17 and conflicts==0
report['verdict']='Structural regressions pass; clean SCANPST gate remains FAIL. Product decision on promoting cached timestamps is open. Outlook validation belongs to parent.'
(OUT/'results.json').write_text(json.dumps(report,indent=2)+'\n')
print(report['verdict'])
