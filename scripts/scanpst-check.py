#!/usr/bin/env python3
"""Scan-only SCANPST check of one PST. Exit 0 PASS, 1 FAIL, 2 couldn't run.

    scripts/scanpst-check.py out.pst [--timeout SECONDS]

See docs/agents/faithful-checks.md.
"""
import argparse, hashlib, json, os, subprocess, sys, uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRIVATE = HERE.parent / 'testdata' / 'private'


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def winpath(path):
    if os.name == 'nt':
        return str(path)
    return subprocess.run(['wslpath', '-w', str(path)], check=True, capture_output=True, text=True).stdout.strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('pst', type=Path)
    parser.add_argument('--timeout', type=int, default=600)
    args = parser.parse_args()
    pst = args.pst.resolve()
    if pst.is_relative_to(PRIVATE):
        sys.exit('Refusing a file under testdata/private/: private files are for the user to check.')

    before = sha256(pst)
    result = subprocess.run(
        ['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
         '-File', winpath(HERE / 'scanpst-check.ps1'), '-InputPst', winpath(pst),
         '-Alias', 'check-' + uuid.uuid4().hex[:12], '-TimeoutSeconds', str(args.timeout)],
        capture_output=True, text=True, timeout=args.timeout + 60)
    if result.returncode:
        print(json.dumps({'gate': 'ERROR', 'stderr': result.stderr.strip()[-2000:]}))
        sys.exit(2)

    data = json.loads(result.stdout.strip())
    data['InputUnchanged'] = sha256(pst) == before
    clean = (data['ScanFinished'] and data['ScanInputUnchanged'] and data['InputUnchanged']
             and data['LogExists'] and data['LogReportsFinished'] and data['DiagnosticLineCount'] == 0)
    data['gate'] = 'PASS' if clean else 'FAIL'
    print(json.dumps(data))
    sys.exit(0 if clean else 1)


if __name__ == '__main__':
    main()
