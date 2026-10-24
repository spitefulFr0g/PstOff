#!/usr/bin/env python3
"""Stage the single pinned public OST; never execute downloaded files."""
import hashlib
from pathlib import Path
import shutil
import tempfile
import urllib.request

PIN = 'f158a64acf2a0e46ac3bd699bc7a5a8da6c40d26'
SHA = 'b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d'
DEST = Path(__file__).resolve().parent.parent / 'target/writer-fixtures/example-2013.ost'
URL = f'https://raw.githubusercontent.com/rjohnsondev/java-libpst/{PIN}/src/test/resources/example-2013.ost'

def verify(path):
    if hashlib.sha256(path.read_bytes()).hexdigest() != SHA:
        raise SystemExit('Public fixture hash mismatch; refusing to use or replace it.')

if DEST.exists():
    verify(DEST)
else:
    download = Path(tempfile.mkdtemp(prefix='pstoff-writer-public-')) / 'example-2013.ost'
    request = urllib.request.Request(URL, headers={
        'User-Agent': 'PstOff-research (https://github.com/spitefulFr0g/PstOff)'})
    with urllib.request.urlopen(request, timeout=60) as response, download.open('xb') as output:
        shutil.copyfileobj(response, output)
    verify(download)
    DEST.parent.mkdir(parents=True, exist_ok=True)
    with DEST.open('xb') as output, download.open('rb') as source:
        shutil.copyfileobj(source, output)
print(f'Pinned public fixture verified: {SHA}')
