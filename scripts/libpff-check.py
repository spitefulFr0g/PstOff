#!/usr/bin/env python3
"""Independent-reader checks with libpff, run under bwrap (no network, no home, read-only input).

    scripts/libpff-check.py compare source.ost out.pst [--out DIR]   exit 0: only explained differences
    scripts/libpff-check.py tree store.pst                           folder tree with message counts
    scripts/libpff-check.py synthetic-check generated-directory      assert generated #23 fixtures

See docs/agents/faithful-checks.md.
"""
import argparse, hashlib, json, os, runpy, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRIVATE = (HERE.parent / 'testdata' / 'private').resolve()
READER = Path(os.environ.get('PSTOFF_LIBPFF_ROOT', Path.home() / '.local/share/pstoff-independent-reader/root'))

# Explained differences, decided on issue #11. Keys are 'tag:type'.
STORE_NID = 0x21
STORE_CHANGED = {'3001:001f', '67ff:0003'}
STORE_ADDED = {'35df:0003', '35e0:0102', '35e3:0102', '35e7:0102'}
# Q12 LastModificationTime, Q15 ChangeKey and PredecessorChangeList, copied from the cache when missing.
ITEM_ADDED = {'3008:0040', '65e2:0102', '65e3:0102'}
ITEM_NID_TYPES = {0x04, 0x08}  # normal and associated messages


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sandboxed(probe, store):
    store = store.resolve()
    if store.is_relative_to(PRIVATE):
        sys.exit('Refusing a file under testdata/private/: private files are for the user to check.')
    if not (READER / 'usr/lib/python3/dist-packages').is_dir():
        sys.exit(f'libpff reader root not found at {READER}; set PSTOFF_LIBPFF_ROOT.')
    before = sha256(store)
    cmd = ['bwrap', '--unshare-all', '--die-with-parent', '--new-session']
    for path in ('/usr', '/lib', '/lib64'):
        cmd += ['--ro-bind', path, path]
    cmd += ['--ro-bind', str(READER), '/reader', '--ro-bind', str(HERE / 'libpff' / probe), '/probe.py',
            '--dir', '/input', '--ro-bind', str(store), '/input/store',
            '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--clearenv',
            '--setenv', 'PYTHONPATH', '/reader/usr/lib/python3/dist-packages',
            '--setenv', 'LD_LIBRARY_PATH', '/reader/usr/lib/x86_64-linux-gnu',
            '/usr/bin/python3', '-P', '-s', '-B', '/probe.py', '/input/store']
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
    if result.returncode:
        sys.exit(f'libpff reader failed on {store}:\n{result.stderr.strip()[-2000:]}')
    if sha256(store) != before:
        sys.exit(f'{store} changed while being read')
    return result.stdout


def explained(key, kind):
    nid, tag = int(key[:8], 16), key.split(':', 2)[2]
    if nid == STORE_NID:
        return tag in (STORE_CHANGED if kind == 'changed' else STORE_ADDED)
    return kind == 'added' and nid & 0x1f in ITEM_NID_TYPES and tag in ITEM_ADDED


def compare(source, output, out_dir):
    a = json.loads(sandboxed('inventory_probe.py', source))
    b = json.loads(sandboxed('inventory_probe.py', output))
    if out_dir:
        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / 'libpff-source.json').write_text(json.dumps(a, indent=2) + '\n')
        (out_dir / 'libpff-output.json').write_text(json.dumps(b, indent=2) + '\n')
    ar, br = a['records'], b['records']
    changed = sorted(k for k in ar.keys() & br.keys() if ar[k] != br[k])
    added = sorted(br.keys() - ar.keys())
    missing = sorted(ar.keys() - br.keys())
    unexplained = {
        'changed': [k for k in changed if not explained(k, 'changed')],
        'added': [k for k in added if not explained(k, 'added')],
        'missing': missing,
    }
    faithful = not any(unexplained.values()) and a['nodes'] == b['nodes'] and not a['errors'] and not b['errors']
    print(json.dumps({
        'verdict': 'PASS' if faithful else 'FAIL',
        'reader_version': a['reader_version'],
        'source_sha256': sha256(source), 'output_sha256': sha256(output),
        'same_object_graph': a['nodes'] == b['nodes'],
        'matched_properties': sum(ar[k] == br[k] for k in ar.keys() & br.keys()),
        'explained_changed': sorted(set(changed) - set(unexplained['changed'])),
        'explained_added_by_tag': {t: sum(k.endswith(':' + t) for k in added) for t in sorted({k.split(':', 2)[2] for k in added})},
        'unexplained': unexplained,
        'source_reader_errors': a['errors'], 'output_reader_errors': b['errors'],
        'coverage': 'Reachable property graph only: not table contents, attachment payloads, recipients or embedded messages.',
    }, indent=2))
    return 0 if faithful else 1


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    c = sub.add_parser('compare')
    c.add_argument('source', type=Path)
    c.add_argument('output', type=Path)
    c.add_argument('--out', type=Path, help='also write both full inventories here')
    t = sub.add_parser('tree')
    t.add_argument('store', type=Path)
    s = sub.add_parser('synthetic-probe', help='hash-only synthetic fixture inventory')
    s.add_argument('store', type=Path)
    s = sub.add_parser('synthetic-check', help='validate explicitly generated #23 fixtures')
    s.add_argument('directory', type=Path)
    args = parser.parse_args()
    if args.command == 'synthetic-check':
        check = runpy.run_path(str(HERE / 'check-synthetic.py'))['check']
        check(args.directory, sandboxed)
        return 0
    if args.command == 'synthetic-probe':
        print(sandboxed('synthetic_probe.py', args.store), end='')
        return 0
    if args.command == 'tree':
        print(sandboxed('tree_probe.py', args.store), end='')
        return 0
    return compare(args.source, args.output, args.out)


if __name__ == '__main__':
    sys.exit(main())
