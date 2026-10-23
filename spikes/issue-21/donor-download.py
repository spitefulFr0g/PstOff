#!/usr/bin/env python3
"""Optional rejected supplemental public donor; data only, own fresh /tmp directory."""
from pathlib import Path
import tempfile,urllib.request,json,hashlib
from run import BASE
rev='f018eb7c91c8356123b021ecd28ae894dd500210'
url=f'https://raw.githubusercontent.com/libyal/testdata/{rev}/pst/outlook.pst'
out=BASE/'output/control-libyal-outlook.pst';assert not out.exists()
p=Path(tempfile.mkdtemp(prefix='pstoff21-libyal-'))/'outlook.pst'
r=urllib.request.Request(url,headers={'User-Agent':'PstOff-research (https://github.com/spitefulFr0g/PstOff)'})
p.write_bytes(urllib.request.urlopen(r).read())
with out.open('xb') as f:f.write(p.read_bytes())
with (BASE/'donor-source.json').open('x') as f:json.dump({'url':url,'commit':rev,'download_path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()},f,indent=2)
print('Downloaded public data; do not execute it.')
