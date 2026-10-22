#!/usr/bin/env python3
"""Replay diagnostic expectations only. This is NOT the zero-diagnostic gate."""
import argparse,subprocess,sys
from pathlib import Path
from run import BASE
p=argparse.ArgumentParser();p.add_argument('--prefix',default='bounded-replay');args=p.parse_args()
assert args.prefix and all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in args.prefix)
trials=[
 ('minimal-pair','65e2,65e3','item:100028'),
 ('minimal-key','65e2','item:100028'),
 ('minimal-pcl','65e3','item:100028'),
 ('minimal-promote-pair','0000','promote,item:100028'),
 ('minimal-promote-key','0000','promote:65e2,item:100028'),
 ('minimal-promote-pcl','0000','promote:65e3,item:100028'),
 ('promote-sync','0000','promote'),
 ('clear-pair','65e2,65e3',''),
 ('pair-align-both','65e2,65e3','align3013,align0042'),
 ('pair-all-defaults','65e2,65e3','align0042,zero:65c6,zero:0e03'),
 ('pair-promote-cn','65e2,65e3','promote:6628'),
 ('pair-align-defaults-flags','65e2,65e3','align3013,align0042,flags,zero:0036,zero:0057,zero:0058,zero:0e03,zero:65c6'),
]
for label,_,_ in trials:
 for suffix in ('.pst','-raw.pst','.json'):assert not (BASE/'output'/(args.prefix+'-'+label+suffix)).exists(),'Choose a new --prefix; no output overwritten'
here=Path(__file__).resolve().parent
for label,clears,tweaks in trials:
 subprocess.run([sys.executable,str(here/'targeted.py'),args.prefix+'-'+label,clears,tweaks],check=True,capture_output=True,text=True)
 print('Replayed diagnostic '+label,flush=True)
subprocess.run([sys.executable,str(here/'collect.py'),args.prefix],check=True)
