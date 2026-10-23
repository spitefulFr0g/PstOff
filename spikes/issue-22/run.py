#!/usr/bin/env python3
"""Public-only, read-only input, network-isolated exploratory candidate runner."""
import hashlib,sys,subprocess,json
from pathlib import Path
BASE=Path('/home/watermelon/.local/share/pstoff-spike22')
BIN=Path('/home/watermelon/.local/share/pstoff-spike22/target/release')
ROOTS=json.loads((BASE/'sources.json').read_text())
PUBLIC=Path(ROOTS['java'])/'src/test/resources/example-2013.ost'
EMPTY=Path(ROOTS['microsoft'])/'crates/pst/examples/Empty.pst'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def run(binary,source,*args,flags=()):
 source=Path(source).resolve()
 assert source in (PUBLIC,EMPTY) or source == Path('/home/watermelon/.local/share/pstoff-spike21/output/q16-final-public.pst') or source == Path('/home/watermelon/.local/share/pstoff-spike20/output/q15-validated-public.pst') or source == Path('/home/watermelon/.local/share/pstoff-spike18/output/q12-public.pst') or source.is_relative_to(BASE/'output') or source.is_relative_to(Path('/home/watermelon/.local/share/pstoff-spike16/output')) or source.is_relative_to(Path('/home/watermelon/.local/share/pstoff-spike17/output'))
 if source==PUBLIC:assert sha(source)=='b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d'
 before=sha(source)
 cmd=['bwrap','--unshare-all','--die-with-parent','--new-session','--ro-bind','/usr','/usr','--ro-bind','/lib','/lib','--ro-bind','/lib64','/lib64','--proc','/proc','--dev','/dev','--tmpfs','/tmp','--dir','/input','--ro-bind',str(source),'/input/store',('--bind' if binary in ('convert','map-spike','donor-schema') else '--ro-bind'),str(BASE/'output'),'/output','--ro-bind',str(BIN/binary),'/candidate','--clearenv','--setenv','LANG','C.UTF-8']
 for f in flags:cmd+=['--setenv',f,'1']
 cmd+=['--chdir','/output','/candidate','/input/store',*args]
 r=subprocess.run(cmd,capture_output=True,text=True,timeout=60)
 assert sha(source)==before
 return r
if __name__=='__main__':
 r=run(sys.argv[1],sys.argv[2],*sys.argv[3:],flags=filter(None,__import__('os').environ.get('SPIKE_FLAGS','').split(',')))
 print(r.stdout,end='');print(r.stderr,end='',file=sys.stderr);sys.exit(r.returncode)
