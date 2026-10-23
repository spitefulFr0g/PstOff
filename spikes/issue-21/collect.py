#!/usr/bin/env python3
"""Verify bounded results; publish aggregates and tags, never message/cache values."""
import json,re,subprocess,sys
from pathlib import Path
from collections import Counter,defaultdict
from run import BASE,PUBLIC,EMPTY,run,sha
HERE=Path(__file__).resolve().parent
OUT=BASE/'output'
Q15=Path('/home/watermelon/.local/share/pstoff-spike20/output/q15-validated-public.pst')
CANDIDATE=OUT/'q16-final-public.pst'
assert sha(Q15)=='e65984caded46d3c19603c45e19c54a82efc9b469f05796a2ba866e481d9f906'
assert sha(EMPTY)=='c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'
def checked(binary,source,*args):
 r=run(binary,source,*args)
 assert r.returncode==0,(binary,r.stdout[:300],r.stderr[:1000])
 return r.stdout.strip()
def rows_in_scan(d):
 alias=d['Alias'];assert re.fullmatch('gate-[a-f0-9]{12}',alias)
 ps='$p=Join-Path $env:TEMP "PstOffPublicSpike/'+alias+'-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
 log=json.loads(subprocess.check_output(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],text=True))
 return sorted(re.findall(r'RowID = ([0-9A-F]+)',log))
def gate(p):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(p)],text=True,capture_output=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 d=json.loads(r.stdout);d['strict_exit']=r.returncode;d['sha256']=sha(p)
 assert d['ScanInputUnchanged'] and d['original_output_unchanged']
 return d
def saved_gate(p):
 d=json.loads(p.with_suffix('.json').read_text());assert d['sha256']==sha(p)
 assert d['ScanFinished'] and d['LogReportsFinished'] and d['ScanInputUnchanged'] and d['original_output_unchanged']
 assert d['gate']=='FAIL' and d['DiagnosticLineCount']==3
 assert rows_in_scan(d)==['200084','2000E4','200184']
 d['strict_exit']=1;d['exact_original_three_mismatch_rows']=True
 return d
def summarize_provenance(p):
 ls=json.loads(p.read_text());counts=Counter();tags=defaultdict(Counter);rowids=set();tables=set();different_rows=set();occurrences=set()
 for l in ls:
  if not l.startswith('Q16 '):continue
  fields=dict(s.split('=',1) for s in l.split()[1:]);rowids.add(fields['row']);tables.add(fields['table']);occurrences.add((fields['table'],fields['row']))
  counts[fields['action']]+=1;tags[fields['tag']][fields['action']]+=1
  if fields['action']=='retain_cached':
   state='retained_present' if fields['present']=='true' else 'retained_absent';counts[state]+=1;tags[fields['tag']][state]+=1
  if fields.get('differed')=='true':counts['differed']+=1;tags[fields['tag']]['differed']+=1;different_rows.add(fields['row'])
 return {'distinct_items':len(rowids),'row_occurrences':len(occurrences),'tables':len(tables),'rows_with_differences':len(different_rows),'cell_counts':dict(counts),'by_tag':{k:dict(v) for k,v in sorted(tags.items())}}
report={'scope':'PUBLIC only; Q16 rows from post-Q12/Q15 PCs, documented computed rules, unresolved cache retained','source_sha256':sha(PUBLIC),'q15_baseline_sha256':sha(Q15),'candidate_path':str(CANDIDATE),'candidate':saved_gate(CANDIDATE),'provenance':summarize_provenance(OUT/'q16-final-public-provenance.json'),'audit':checked('audit',Q15,'/output/'+CANDIDATE.name),'preservation':json.loads(checked('cell-diff',Q15,'/output/'+CANDIDATE.name)),'policy_guards':checked('q16-guards',CANDIDATE),'q15_guards':checked('q15-guards',CANDIDATE),'microsoft_reader':checked('ms-probe',CANDIDATE),'trials':{},'final_source_replay':saved_gate(OUT/'q16-final-source-replay.pst')}
assert report['preservation']['identical_properties']==1550
assert report['preservation']['row_ids_preserved']==67
assert report['preservation']['changed_cells_by_id']=={'0042':3,'3013':3}
assert report['preservation']['added_cells_by_id']=={'0e03':4,'65c6':3}
assert not report['preservation']['removed_cells_by_id']
trials=[('trial01-subject-only','only:0037001f'),('trial02-sender-only','only:0042001f'),('trial03-conversation-only','only:30130102'),('trial04-sender-conversation','only:0042001f,30130102'),('trial05-display-only','only:0e03001f'),('trial06-secure-submit-only','only:65c60003'),('trial07-hasattach-only','only:0e1b000b'),('trial08-flags-only','only:0e070003'),('trial09-size-only','only:0e080003'),('trial10-donor-shape-v2','shape'),('trial11-clear-local-cache','diagnostic-clear:66120003,66280014'),('trial12-clear-local-status','diagnostic-clear:0e170003,66120003,66280014')]
for label,mode in trials:
 p=OUT/(label+'.pst');d=saved_gate(p);diff=json.loads(checked('cell-diff',Q15,'/output/'+p.name));audit=checked('audit',Q15,'/output/'+p.name)
 report['trials'][label]={'mode':mode,'strict_scan':d,'preservation':diff,'audit':audit,'diagnostic_only':mode=='shape' or mode.startswith('diagnostic-clear:')}
invalid=OUT/'strict-control-invalid-zeros.pst'
if not invalid.exists():
 with invalid.open('xb') as f:f.write(bytes(4096))
assert invalid.read_bytes()==bytes(4096)
report['controls']={'valid_empty':gate(EMPTY),'invalid_zeros':gate(invalid)}
assert report['controls']['valid_empty']['gate']=='PASS' and report['controls']['valid_empty']['DiagnosticLineCount']==0
assert report['controls']['invalid_zeros']['gate']=='FAIL'
report['donor']={'public_path':'java-libpst/src/test/resources/dist-list.pst','sha256':sha(OUT/'control-dist-list.pst'),'donor_normal_row_mismatch_reported':False,'whole_file_valid':False,'strict_diagnostic_count':133,'shape_limit':'Uses column order from a normal row with no reported row mismatch; donor file has schema errors and other failing rows. All target columns and decoded values retained. Not evidence from a globally valid normal-item file.','downloaded_libyal_donor_rejected':{'sha256':sha(OUT/'control-libyal-outlook.pst'),'strict_diagnostic_count':408}}
report['independent_libpff']={'run':False,'reason':'No clean candidate. Conditional independent comparison deferred; no previous result relabelled.'}
report['verdict']='FAIL/3. No sufficient column set or cause isolated within Q16 and 12 bounded diagnostic trials. No new item-PC policy applied. Exact next product decision is not established.'
(HERE/'evidence').mkdir(exist_ok=True)
(HERE/'evidence/results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:report[k] for k in ['candidate','audit','preservation','policy_guards','verdict']},indent=2))
