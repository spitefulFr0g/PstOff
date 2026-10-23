#!/usr/bin/env python3
"""Exploratory public-fixture SCANPST gate. Nonzero exit means not clean."""
import hashlib,json,subprocess,sys
from pathlib import Path
p=Path(sys.argv[1]).resolve()
allowed=[Path("/home/watermelon/.local/share/pstoff-spike19/output"),Path("/home/watermelon/.local/share/pstoff-spike18/output"),Path('/home/watermelon/.local/share/pstoff-spike17/output'),Path('/home/watermelon/.local/share/pstoff-spike16/output'),Path('/home/watermelon/.local/share/pstoff-spike22/output'),Path(json.loads(Path('/home/watermelon/.local/share/pstoff-spike22/sources.json').read_text())['microsoft'])/'crates/pst/examples/Empty.pst']
if not any(p == a or (a.is_dir() and p.is_relative_to(a)) for a in allowed):raise SystemExit('Refusing input outside public spike output/control directories')
h=hashlib.sha256(p.read_bytes()).hexdigest()
def winpath(path):return subprocess.run(['wslpath','-w',str(path)],check=True,capture_output=True,text=True).stdout.strip()
script=Path(__file__).resolve().with_name('validate-public-windows-strict.ps1')
args=['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',winpath(script),'-InputPst',winpath(p),'-Alias','gate-'+__import__('uuid').uuid4().hex[:12]]
r=subprocess.run(args,capture_output=True,text=True,timeout=40)
if r.returncode:
 print(json.dumps({'gate':'ERROR','validator_exit':r.returncode}));raise SystemExit(2)
data=json.loads(r.stdout.strip())
clean=data['ScanFinished'] and data['ScanInputUnchanged'] and data['LogExists'] and data['LogReportsFinished'] and data['DiagnosticLineCount']==0
unchanged=h==hashlib.sha256(p.read_bytes()).hexdigest()
data.update({'gate':'PASS' if clean and unchanged else 'FAIL','original_output_unchanged':unchanged})
print(json.dumps(data))
raise SystemExit(0 if clean and unchanged else 1)
