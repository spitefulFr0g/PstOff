#!/usr/bin/env python3
"""Independent offline raw-property comparison, pinned public OST vs accepted PST."""
import json,subprocess,hashlib
from pathlib import Path
BASE=Path('/home/watermelon/.local/share/pstoff-spike23'); HERE=Path(__file__).resolve().parent
roots=json.loads((BASE/'sources.json').read_text())
reader=Path('/home/watermelon/.local/share/pstoff-independent-reader/root')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def inventory(source,label):
 before=sha(source)
 cmd=['bwrap','--unshare-all','--die-with-parent','--new-session']
 for p in ['/usr','/lib','/lib64']:cmd+=['--ro-bind',p,p]
 cmd+=['--ro-bind',str(reader),'/reader','--ro-bind',str(HERE/'inventory.py'),'/probe.py','--dir','/input','--ro-bind',str(source),'/input/store','--proc','/proc','--dev','/dev','--tmpfs','/tmp','--clearenv','--setenv','PYTHONPATH','/reader/usr/lib/python3/dist-packages','--setenv','LD_LIBRARY_PATH','/reader/usr/lib/x86_64-linux-gnu','/usr/bin/python3','-B','/probe.py']
 r=subprocess.run(cmd,check=True,capture_output=True,text=True,timeout=60)
 assert sha(source)==before
 data=json.loads(r.stdout);(BASE/'output'/f'libpff-{label}.json').write_text(json.dumps(data,indent=2)+'\n');return data
source=Path(roots['java'])/'src/test/resources/example-2013.ost'; result=BASE/'output/q17-public.pst'
a=inventory(source,'source');b=inventory(result,'accepted');ar=a['records'];br=b['records']
changed=sorted(k for k in ar.keys() & br.keys() if ar[k]!=br[k]);missing=sorted(ar.keys()-br.keys());added=sorted(br.keys()-ar.keys())
expected_changed=['00000021:0:3001:001f','00000021:0:67ff:0003']
expected_store={f'00000021:0:{tag}' for tag in ['35df:0003','35e0:0102','35e3:0102','35e7:0102']}
items={nid for nid in b['nodes'] if int(nid)&31 in [4,8]}
expected_added=expected_store | {f'{int(nid):08x}:0:{tag}' for nid in items for tag in ['3008:0040','65e2:0102','65e3:0102']}
unexpected={'changed':sorted(set(changed)-set(expected_changed)),'missing':missing,'added':sorted(set(added)-expected_added),'expected_changes_absent':sorted(set(expected_changed)-set(changed)),'expected_additions_absent':sorted(expected_added-set(added))}
d={'reader_version':a['reader_version'],'source_sha256':sha(source),'output_sha256':sha(result),'source_nodes':len(a['nodes']),'output_nodes':len(b['nodes']),'same_object_ids':a['nodes']==b['nodes'],'matched_raw_properties':sum(ar[k]==br[k] for k in ar.keys() & br.keys()),'changed_keys':changed,'added_counts_by_tag':{tag:sum(k.endswith(':'+tag) for k in added) for tag in sorted({k.split(':',2)[2] for k in added})},'missing_properties':len(missing),'source_reader_errors':a['errors'],'output_reader_errors':b['errors'],'unexpected_differences':unexpected,'coverage':'Reachable property-bearing graph; not exhaustive tables, attachments, recipients, embedded payloads or Outlook acceptance.'}
(HERE.parent/'evidence/libpff-comparison.json').write_text(json.dumps(d,indent=2)+'\n');print(json.dumps(d))
assert not any(unexpected.values()) and a['nodes']==b['nodes'] and not a['errors'] and not b['errors']
