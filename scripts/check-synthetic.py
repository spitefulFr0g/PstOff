#!/usr/bin/env python3
"""Assert #23's generated samples with libpff under the existing bwrap wrapper.

Run in WSL: python3 scripts/libpff-check.py synthetic-check /mnt/c/.../output-v2/synthetic-23
Generate first with sandbox-cargo.py test -p pstoff-engine --test synthetic
export_for_libpff -- --ignored. This checks synthetic inputs, not Faithful output.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def digest(raw):
    return {'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}


def text(value):
    return digest(value.encode('utf-16le'))


def number(value, size=4):
    return digest(value.to_bytes(size, 'little'))


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def check(directory, sandboxed):
    inventories = {}
    for name in ('tree', 'rich', 'unreadable-item', 'unreadable-folder'):
        inventories[name] = json.loads(sandboxed('synthetic_probe.py', directory / (name + '.ost')))
    tree = inventories['tree']
    require(not tree['errors'], 'healthy tree has reader errors')
    root = tree['tree']
    require(root['id'] == 0x122 and not root['items'] and len(root['folders']) == 1, 'root tree mismatch')
    require(root['properties']['3001:001f'] == text('Synthetic root'), 'root property mismatch')
    child = root['folders'][0]
    require(child['id'] == 0x142 and not child['folders'], 'child tree mismatch')
    require(child['properties']['3001:001f'] == text('Synthetic child'), 'child property mismatch')
    require([i['id'] for i in child['items']] == [0x164, 0x184], 'Item tree mismatch')
    require([i['properties']['0e08:0003'] for i in child['items']] == [number(1234), number(56)], 'Item size mismatch')

    rich = inventories['rich']
    require(not rich['errors'], 'healthy rich sample has reader errors')
    root = rich['tree']
    require(root['id'] == 0x122 and not root['folders'], 'rich folder tree mismatch')
    items = root['items']
    classes = ['IPM.Note', 'IPM.Appointment', 'IPM.Contact', 'IPM.Task', 'IPM.StickyNote', 'IPM.Activity']
    require([i['id'] for i in items] == [0x144, 0x164, 0x184, 0x1a4, 0x1c4, 0x1e4], 'rich Item tree mismatch')
    for i, (item, cls) in enumerate(zip(items, classes)):
        require(item['properties']['001a:001f'] == text(cls), 'Item class mismatch')
        require(item['properties']['0e08:0003'] == number((i + 1) * 100), 'rich Item size mismatch')
    properties = items[0]['properties']
    require(properties['1000:001f'] == text('Synthetic body ' * 2000), 'large body mismatch')
    for tag in range(0x6800, 0x680a):
        require(properties[f'{tag:04x}:0102'] == digest(bytes([0x23]) * 1000), 'multi-page heap mismatch')
    require(items[1]['properties']['8000:0003'] == number(1), 'HeaderItem marker mismatch')
    require('1000:001f' not in items[1]['properties'], 'headers-only Item has a body')
    attachments = items[0]['attachments']
    require(len(attachments) == 3, 'attachment count mismatch')
    require(attachments[0]['payload'] == digest(b'B' * 20000), 'attachment stream mismatch')
    require(attachments[2]['properties']['3705:0003'] == number(2), 'external method mismatch')
    require(attachments[2]['properties']['370d:001f'] == text('https://example.invalid/synthetic'), 'external reference mismatch')
    native = rich['native']
    require(native['nameid'] == [{
        '0002:0102': digest(bytes.fromhex('0820060000000000c000000000000046')),
        '0003:0102': digest(bytes.fromhex('7885000006000000')),
    }], 'HeaderItem must be PSETID_Common LID 0x8578, not a string-named lookalike')
    require(native['324']['recipients'] == [{
        '0c15:0003': number(1), '3001:001f': text('Synthetic recipient'),
        '3003:001f': text('fixture@example.invalid'),
    }], 'recipient mismatch')
    require(native['324']['embedded'] == [{'001a:001f': text('IPM.Note'), '0e08:0003': number(17)}], 'embedded Item mismatch')
    cached = native['290']['cached']
    require(len(cached) == 6, 'cached table row count mismatch')
    require(cached[2]['67f2:0003'] == number(0x184), 'cached row identity mismatch')
    for tag, expected in {
        '3008:0040': number(132_000_000_000_000_000, 8),
        '65e2:0102': digest(bytes([1]) * 22), '65e3:0102': digest(bytes([2]) * 23),
    }.items():
        require(cached[2][tag] == expected, 'Q12/Q15 cached cell mismatch')
        require(tag not in items[2]['properties'], 'cached-only cell unexpectedly present on Item')

    for name, expected in [
        ('unreadable-item', {'parent': 0x142, 'index': 0, 'phase': 'items-open', 'error': 'OSError'}),
        ('unreadable-folder', {'id': 0x142, 'phase': 'properties', 'error': 'OSError'}),
    ]:
        errors = inventories[name]['errors']
        require(errors == [expected], name + ' did not fail at target read')
    print(json.dumps({'verdict': 'PASS', 'reader_version': rich['reader_version'],
                      'samples': {name: digest((directory / (name + '.ost')).read_bytes()) for name in inventories},
                      'healthy_samples': 2, 'independent_read_failures': 2,
                      'coverage': ['tree', 'sizes', 'six Item classes', 'multi-page heap',
                                   'body stream', 'attachment stream', 'recipients', 'embedded Item',
                                   'HeaderItem', 'external reference', 'Q12/Q15 cached cells']}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    # Retain the old command as a shim; all reader execution enters the wrapper.
    raise SystemExit(subprocess.run([
        sys.executable, str(Path(__file__).with_name('libpff-check.py')),
        'synthetic-check', str(parser.parse_args().directory),
    ]).returncode)
