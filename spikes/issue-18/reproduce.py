#!/usr/bin/env python3
"""Q12 PUBLIC replay. Exit zero ONLY when the strict zero-diagnostic gate passes."""
import argparse,json,re,subprocess,sys
from pathlib import Path
from run import run,BASE,PUBLIC,EMPTY,sha
HERE=Path(__file__).resolve().parent
OUT=BASE/'output'
PRIOR=Path('/home/watermelon/.local/share/pstoff-spike17/output/schema-header-exclusive.pst')
parser=argparse.ArgumentParser();parser.add_argument('--label',default='q12-public');args=parser.parse_args()
assert re.fullmatch('[a-zA-Z0-9-]+',args.label)
label=args.label
report={'public_source_sha256':sha(PUBLIC),'policy':'Q12 accepted same-NID validated cache; no server-history claim'}
def scan(path):
 r=subprocess.run([sys.executable,str(HERE/'scan-loop-strict.py'),str(path)],capture_output=True,text=True,timeout=50)
 assert r.returncode in (0,1),(r.stdout,r.stderr)
 result=json.loads(r.stdout); assert (r.returncode==0)==(result['gate']=='PASS')
 return result
def checked(binary,source,*args,flags=()):
 r=run(binary,source,*args,flags=flags)
 assert r.returncode==0,(binary,r.stdout,r.stderr)
 return r.stdout.strip()
# RED comes first, without touching or regenerating the prior worker's output.
assert sha(PRIOR)=='485eebf759829911edfadf6c946de58fb02e2fa03ba300a0d56786751fc482ee'
report['prior_strict_red']=scan(PRIOR)
assert report['prior_strict_red']['gate']=='FAIL' and report['prior_strict_red']['DiagnosticLineCount']==51
# Test the new timestamp-presence assertion against the prior output in our sandbox.
# Copy only this PUBLIC fixture into this output root so the reader can see both files.
prior_copy=OUT/(label+'-prior-control.pst')
assert not prior_copy.exists();prior_copy.write_bytes(PRIOR.read_bytes())
r=run('audit',PUBLIC,'/output/'+prior_copy.name)
assert r.returncode!=0 and 'Q12 output must have every eligible timestamp' in r.stderr,(r.stdout,r.stderr)
report['presence_regression_red']={'exit':r.returncode,'summary':r.stdout.strip()}
report['policy_guards']=checked('q12-guards',PUBLIC)
raw=OUT/(label+'-raw.pst');final=OUT/(label+'.pst');existing=OUT/(label+'-existing.pst')
assert not any(p.exists() for p in [raw,final,existing]),'Refusing to overwrite prior outputs; choose --label'
conversion=checked('convert',PUBLIC,'/output/'+raw.name,flags=('PSTOFF_TC','PSTOFF_HEADER','PSTOFF_ALIAS','PSTOFF_Q12'))
assert 'nodes=308 dropped=0 problems=[]' in conversion
provenance=[x for x in conversion.splitlines() if x.startswith('Q12 ')]
assert len(provenance)==17 and all('reason=promoted_exact_same_nid_cache' in x for x in provenance)
report['per_item_provenance']=provenance
report['map_rebuild']=checked('map-spike',raw,'/output/'+final.name)
report['output']={'path':str(final),'sha256':sha(final),'size':final.stat().st_size}
report['strict_scan']=scan(final)
report['output_audit']=checked('audit',PUBLIC,'/output/'+final.name)
assert 'byte_identical_subnodes=136' in report['output_audit'] and 'identical_table_rows=67' in report['output_audit']
report['microsoft_reader']=checked('ms-probe',final)
# Whole-file writer path with existing timestamps: every message stream must stay exact.
existing_run=checked('convert',final,'/output/'+existing.name,flags=('PSTOFF_Q12',))
assert existing_run.count('reason=preserved_existing')==17 and 'promoted_exact' not in existing_run
report['existing_item_preservation']=checked('audit',final,'/output/'+existing.name)
report['source_cache_comparison']=checked('cache-diff',PUBLIC)
report['output_cache_comparison']=checked('cache-diff',final)
assert 'exact_cached_modtime_matches=18' in report['output_cache_comparison']
# Aggregate/key-only decoded-property preservation, never publish actual values.
a=checked('pc-probe',PUBLIC);b=checked('pc-probe',final)
key=lambda x:' '.join(x.split(' ')[:2])
a={key(x):x for x in a.splitlines()};b={key(x):x for x in b.splitlines()}
preservation={'identical_property_values':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('VALUE ')), 'identical_table_rows':sum(a[k]==b[k] for k in a.keys()&b.keys() if k.startswith('TCROW ')), 'changed_keys':sorted(k for k in a.keys()&b.keys() if a[k]!=b[k]),'missing_keys':sorted(a.keys()-b.keys()),'added_keys':sorted(b.keys()-a.keys())}
report['preservation']=preservation
assert preservation['identical_property_values']==1493 and preservation['identical_table_rows']==67
assert not preservation['missing_keys']
assert [k for k in preservation['changed_keys'] if k.startswith('VALUE ')]==['VALUE 00000021:3001','VALUE 00000021:67ff']
added=[k for k in preservation['added_keys'] if not k.startswith('VALUE 00000021:')]
assert len(added)==17 and all(k.endswith(':3008') for k in added)
assert len(preservation['added_keys'])==21
invalid=OUT/(label+'-invalid-zeros.pst');assert not invalid.exists();invalid.write_bytes(bytes(4096))
assert sha(EMPTY)=='c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'
report['controls']={'valid_empty':scan(EMPTY),'invalid_zeros':scan(invalid)}
assert report['controls']['valid_empty']['gate']=='PASS' and report['controls']['invalid_zeros']['gate']=='FAIL'
h=sha(final)[:12]
ps=f'$p=Join-Path $env:TEMP "PstOffPublicSpike/gate-{h}-scan.log"; [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($p)) | ConvertTo-Json -Compress'
r=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',ps],capture_output=True,text=True,check=True)
log=json.loads(r.stdout)
report['remaining']={name:len(re.findall(pattern,log,re.I)) for name,pattern in {'missing_modification_time':'Missing PR_LAST_MODIFICATION_TIME','table_object_mismatch':"row doesn.t match sub-object",'missing_column':'missing required column','failed_addref':'failed addref','rgbFP':'Problems with rgbFP','invalid_content_count':'Invalid PR_CONTENT_COUNT','bad_heap_map':'bad ibHnpm'}.items()}
report['strict_gate_pass']=report['strict_scan']['gate']=='PASS'
report['verdict']='Q12 policy/preservation checks PASS; strict zero-diagnostic gate '+report['strict_scan']['gate']+'. Outlook assessment belongs to parent.'
(OUT/(label+'-results.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'output':report['output'],'remaining':report['remaining'],'verdict':report['verdict']},indent=2))
sys.exit(0 if report['strict_gate_pass'] else 1)
