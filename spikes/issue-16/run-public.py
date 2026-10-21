#!/usr/bin/env python3
"""Bounded public-only runtime checks. Build audited candidates first (README)."""
import hashlib
import json
from pathlib import Path
import subprocess

BASE = Path.home() / '.local/share/pstoff-spike16'
INPUT = BASE / 'input'
OUTPUT = BASE / 'output'
BIN = BASE / 'target/release'
PUBLIC = {
    'example-2013.ost': (Path('/tmp/pstres-src/rjohnsondev_java-libpst/repo/src/test/resources/example-2013.ost'), 'b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d'),
    'Empty.pst': (Path('/tmp/pstres-src/microsoft_outlook-pst-rs/repo/crates/pst/examples/Empty.pst'), 'c16ae985d12011ad510ccb3b10f19d61b0c3b3311fa4696fb56b87812bd8d5c5'),
}
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(label, binary, source, *args):
    # A fresh mount namespace contains no home directory or private store. Source
    # (including an output under independent-reader examination) is read-only.
    command = ['bwrap', '--unshare-all', '--die-with-parent', '--new-session',
               '--ro-bind', '/usr', '/usr', '--ro-bind', '/lib', '/lib',
               '--ro-bind', '/lib64', '/lib64', '--proc', '/proc', '--dev', '/dev',
               '--tmpfs', '/tmp', '--dir', '/input', '--ro-bind', str(source), '/input/store',
               ('--bind' if '--convert' in args else '--ro-bind'), str(OUTPUT), '/output', '--ro-bind', str(BIN / binary), '/candidate',
               '--clearenv', '--setenv', 'LANG', 'C.UTF-8', '--chdir', '/output',
               '/candidate', '/input/store', *args]
    before = sha(source)
    result = subprocess.run(command, capture_output=True, text=True, timeout=60)
    assert sha(source) == before, 'source changed'
    (OUTPUT / (label + '.stdout')).write_text(result.stdout)
    (OUTPUT / (label + '.stderr')).write_text(result.stderr)
    runs[label] = {'exit': result.returncode, 'command': command,
                   'stdout_sha256': hashlib.sha256(result.stdout.encode()).hexdigest(),
                   'stderr_sha256': hashlib.sha256(result.stderr.encode()).hexdigest()}
    return result.stdout

INPUT.mkdir(parents=True, exist_ok=True)
OUTPUT.mkdir(parents=True, exist_ok=True)
runs = {}
for checkout, revision in [('/tmp/pstres-sp00nznet_pstfree/repo', 'b6e2ff31592f74458f0481d4671251c92044d8f0'), ('/tmp/pstres-src/microsoft_outlook-pst-rs/repo', 'cfb721daee538acc50d2bad6faac5ce724f6037d')]:
    assert subprocess.check_output(['git', '-C', checkout, 'rev-parse', 'HEAD'], text=True).strip() == revision
    assert not subprocess.check_output(['git', '-C', checkout, 'diff', 'HEAD', '--', 'src', 'crates', 'Cargo.toml', '.cargo'], text=True).strip()
for name, (original, expected) in PUBLIC.items():
    assert sha(original) == expected
    copy = INPUT / name
    if not copy.exists():
        copy.write_bytes(original.read_bytes())
    assert sha(copy) == expected
    copy.chmod(0o444)
source = INPUT / 'example-2013.ost'
for label, args in [('summary', ()), ('verify', ('--verify',))]:
    run('ost-' + label, 'pstfree', source, *args)
run('conversion', 'pstfree', source, '--convert', '/output/example-2013-pstfree.pst')
pst = OUTPUT / 'example-2013-pstfree.pst'
assert pst.is_file()
for label, args in [('summary', ()), ('verify', ('--verify',))]:
    run('pst-' + label, 'pstfree', pst, *args)
for label, path in [('ost', source), ('pst', pst), ('empty-control', INPUT / 'Empty.pst')]:
    run('ms-' + label, 'ms-probe', path)
for label, path in [('ost', source), ('pst', pst)]:
    run('pc-' + label, 'pc-probe', path)
# Raw values stay local. Publish aggregate comparison only; it is the candidate
# comparing itself, covers top-level PCs/TCs only, and is not full preservation.
a = (OUTPUT / 'pc-ost.stdout').read_text().splitlines()
b = (OUTPUT / 'pc-pst.stdout').read_text().splitlines()
def keyed(lines):
    return {' '.join(line.split()[:2]): line for line in lines}
aa, bb = keyed(a), keyed(b)
semantic = {
    'source_pc_ok': sum(x.startswith('PC ') for x in a),
    'result_pc_ok': sum(x.startswith('PC ') for x in b),
    'source_pc_errors': [x for x in a if x.startswith('PC_ERROR')],
    'result_pc_errors': [x for x in b if x.startswith('PC_ERROR')],
    'source_tc_ok': sum(x.startswith('TC ') for x in a),
    'result_tc_ok': sum(x.startswith('TC ') for x in b),
    'source_tc_errors': [x for x in a if x.startswith('TC_ERROR')],
    'result_tc_errors': [x for x in b if x.startswith('TC_ERROR')],
    'source_missing_subnode_mentions': sum(x.count('MissingSubnode(') for x in a),
    'result_missing_subnode_mentions': sum(x.count('MissingSubnode(') for x in b),
    'source_raw_value_mentions': sum(x.count('Raw {') for x in a),
    'result_raw_value_mentions': sum(x.count('Raw {') for x in b),
    'identical_property_values': sum(aa[k] == bb[k] for k in aa.keys() & bb.keys() if k.startswith('VALUE ')),
    'identical_table_rows': sum(aa[k] == bb[k] for k in aa.keys() & bb.keys() if k.startswith('TCROW ')),
    'source_property_values': sum(x.startswith('VALUE ') for x in a),
    'result_property_values': sum(x.startswith('VALUE ') for x in b),
    'source_table_rows': sum(x.startswith('TCROW ') for x in a),
    'result_table_rows': sum(x.startswith('TCROW ') for x in b),
    'identical_diagnostic_lines': sum(aa[k] == bb[k] for k in aa.keys() & bb.keys()),
    'different_diagnostic_keys': sorted(k for k in aa.keys() & bb.keys() if aa[k] != bb[k]),
    'missing_diagnostic_keys': sorted(aa.keys() - bb.keys()),
    'added_diagnostic_keys': sorted(bb.keys() - aa.keys()),
}
for name, (original, expected) in PUBLIC.items():
    assert sha(original) == sha(INPUT / name) == expected
report = {'runs': runs, 'semantic': semantic, 'public_fixture_sha256': {n: h for n, (_, h) in PUBLIC.items()},
          'output': {'path': str(pst), 'size': pst.stat().st_size, 'sha256': sha(pst)}}
(OUTPUT / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'semantic': semantic, 'output': report['output'], 'exits': {k: v['exit'] for k, v in runs.items()}}, indent=2))
