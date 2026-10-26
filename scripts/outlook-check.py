#!/usr/bin/env python3
"""Automated "opens in classic Outlook" check of one PST. Exit 0 PASS, 1 FAIL, 2 couldn't run.

    scripts/outlook-check.py out.pst --user-notified [--expect-sender NAME] [--expect-folder NAME ...]
                                     [--check-dir DIR] [--timeout SECONDS]

Starts Outlook as `outlook.exe /profile PstOffValidation`, drives it through UI Automation
(scripts/outlook-check.ps1) on a copy in the check folder, and prints one JSON verdict line.
It drives the user's real desktop: tell the user first, then pass --user-notified.
See docs/agents/faithful-checks.md and docs/agents/computer-use.md.
"""
import argparse, hashlib, json, os, re, shutil, subprocess, sys, time, unicodedata, uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRIVATE = HERE.parent / 'testdata' / 'private'
PROFILE = 'PstOffValidation'
DEFAULT_CHECK_DIR = Path(os.environ.get('USERPROFILE', str(Path.home()))) / 'PstOff-outlook-check'
DEFAULT_OUTLOOK = Path(r'C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE')
STANDARD_FOLDERS = ('Deleted Items',)
CLOSE_SECONDS = 60
# What the pre-Q16 output showed in the sender column (#18): another cell's message class.
MESSAGE_CLASS = re.compile(r'^(IPM|IPC|REPORT)\.', re.IGNORECASE)


class Refused(Exception):
    """The check won't run on this input at all."""


class Hung(Exception):
    """Outlook or the UI driver didn't finish in time."""


class DriverError(Exception):
    """The UI driver couldn't complete its steps."""


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def outlook_command(exe):
    # The profile is fixed here and nowhere else: the default profile holds the user's mailbox.
    return [str(exe), '/profile', PROFILE]


def sender_problem(sender):
    if sender is None or not sender.strip():
        return 'empty'
    if '\ufffd' in sender:
        return 'replacement character'
    if any(unicodedata.category(c) in ('Cc', 'Cs', 'Co', 'Cn') for c in sender):
        return 'control or unassigned character'
    if MESSAGE_CLASS.match(sender.strip()):
        return 'message class in sender column'
    return None


def has_folder(folders, name):
    return any(f == name or f.startswith(name + ' ') for f in folders)


def verdict(observed, expect_folders, expect_sender):
    """Problems that make the PST FAIL. Mail content stays out of them."""
    if not observed.get('Opened'):
        return ['Outlook did not open the data file']
    folders = observed.get('Folders') or []
    problems = ['missing folder: ' + name for name in expect_folders if not has_folder(folders, name)]
    messages = observed.get('Messages') or []
    for index, message in enumerate(messages, 1):
        problem = sender_problem(message.get('Sender'))
        if problem:
            problems.append('message %d in %s: sender column %s' % (index, message.get('Folder', '?'), problem))
        elif expect_sender is not None and message.get('Sender') != expect_sender:
            problems.append('message %d in %s: sender column does not read the expected sender'
                            % (index, message.get('Folder', '?')))
    if expect_sender is not None and not messages:
        problems.append('no messages listed to check the expected sender against')
    opened = observed.get('OpenedMessage')
    if messages and not opened:
        problems.append('no message could be opened')
    elif opened and opened.get('BodyFound') is False:
        problems.append('opened message shows no body')
    return problems


def refuse_private(path, what):
    if Path(path).resolve().is_relative_to(PRIVATE.resolve()):
        raise Refused('Refusing %s under testdata/private/: private files are for the user to check.' % what)


def remove_copy(path):
    for _ in range(20):
        try:
            path.unlink(missing_ok=True)
            return True
        except PermissionError:
            time.sleep(0.5)
    return False


def run_check(pst, check_dir, system, user_notified=False, expect_sender=None,
              expect_folders=STANDARD_FOLDERS, timeout_seconds=300, outlook_exe=DEFAULT_OUTLOOK):
    """Returns (verdict dict, exit code). Raises Refused before anything is launched."""
    pst, check_dir = Path(pst).resolve(), Path(check_dir).resolve()
    refuse_private(pst, 'a file')
    refuse_private(check_dir, 'a check folder')
    if pst.suffix.lower() != '.pst':
        raise Refused('Refusing %s: only PST copies are opened in Outlook.' % pst.name)
    if not user_notified:
        raise Refused('This check drives the real desktop. Tell the user first, then pass --user-notified.')

    result = {'gate': 'ERROR', 'Profile': PROFILE, 'problems': []}

    def could_not_run(reason):
        result['gate'] = 'ERROR'
        result['problems'].append(reason)
        return result, 2

    if system.outlook_running():
        return could_not_run('Outlook is already open; close it first')
    if not system.profile_exists(PROFILE):
        return could_not_run('Outlook profile %s does not exist (#7)' % PROFILE)

    before = sha256(pst)
    check_dir.mkdir(parents=True, exist_ok=True)
    copy = check_dir / ('pstoff-check-%s.pst' % uuid.uuid4().hex[:12])
    shutil.copyfile(pst, copy)
    observed = None
    try:
        try:
            pid = system.launch_outlook(outlook_command(outlook_exe))
        except OSError as error:
            return could_not_run('Outlook could not start: %s' % error)
        try:
            observed = system.drive(pid, copy, timeout_seconds)
        except (Hung, DriverError) as error:
            system.kill(pid)
            return could_not_run('%s: %s' % (type(error).__name__, error))
        if not system.wait_for_exit(pid, CLOSE_SECONDS):
            system.kill(pid)
            return could_not_run('Outlook did not close after the check and was killed')
    finally:
        result['CopyRemoved'] = remove_copy(copy)
        result['InputUnchanged'] = sha256(pst) == before
        if observed is not None:
            messages = observed.get('Messages') or []
            result.update({
                'Opened': observed.get('Opened'), 'OpenError': observed.get('OpenError'),
                'Folders': observed.get('Folders') or [], 'MessageCount': len(messages),
                'MessageOpened': bool(observed.get('OpenedMessage')),
                'ReducedFunctionality': observed.get('ReducedFunctionality'),
                'DataFileClosed': observed.get('DataFileClosed'),
            })

    if not observed.get('DataFileClosed'):
        return could_not_run('the data file could not be closed in Outlook')
    if not result['InputUnchanged']:
        return could_not_run('the input PST changed during the check')
    result['problems'] = verdict(observed, expect_folders, expect_sender)
    result['gate'] = 'FAIL' if result['problems'] else 'PASS'
    return result, 1 if result['problems'] else 0


class WindowsSystem:
    """The real process table, registry and UI driver."""

    def __init__(self):
        self.processes = {}

    def outlook_running(self):
        listing = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq OUTLOOK.EXE', '/FO', 'CSV', '/NH'],
                                 capture_output=True, text=True, check=True).stdout
        return any(line.upper().startswith('"OUTLOOK.EXE"') for line in listing.splitlines())

    def profile_exists(self, name):
        import winreg
        try:
            winreg.CloseKey(winreg.OpenKey(
                winreg.HKEY_CURRENT_USER, '\\'.join([r'Software\Microsoft\Office\16.0\Outlook\Profiles', name])))
            return True
        except OSError:
            return False

    def launch_outlook(self, command):
        process = subprocess.Popen(command)
        self.processes[process.pid] = process
        return process.pid

    def drive(self, pid, pst_copy, timeout_seconds):
        try:
            completed = subprocess.run(
                ['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                 '-File', str(HERE / 'outlook-check.ps1'), '-ProcessId', str(pid),
                 '-Pst', str(pst_copy), '-TimeoutSeconds', str(timeout_seconds)],
                capture_output=True, text=True, encoding='utf-8', timeout=timeout_seconds + 30)
        except subprocess.TimeoutExpired:
            raise Hung('no verdict from Outlook within %d seconds' % timeout_seconds)
        if completed.returncode:
            raise DriverError(completed.stderr.strip()[-2000:] or 'exit %d' % completed.returncode)
        try:
            return json.loads(completed.stdout.strip().splitlines()[-1].lstrip('\ufeff'))
        except (IndexError, ValueError):
            raise DriverError('no JSON observations from the UI driver')

    def wait_for_exit(self, pid, timeout_seconds):
        try:
            self.processes[pid].wait(timeout=timeout_seconds)
            return True
        except subprocess.TimeoutExpired:
            return False

    def kill(self, pid):
        subprocess.run(['taskkill', '/PID', str(pid), '/T', '/F'], capture_output=True)
        try:
            self.processes[pid].wait(timeout=30)
        except subprocess.TimeoutExpired:
            pass


def main(argv=None, system=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('pst', type=Path)
    parser.add_argument('--user-notified', action='store_true',
                        help='confirm the user was told that this drives their desktop')
    parser.add_argument('--expect-sender', help='every listed message must show this sender')
    parser.add_argument('--expect-folder', action='append',
                        help='a folder the store must show (default: %s)' % ', '.join(STANDARD_FOLDERS))
    parser.add_argument('--check-dir', type=Path, default=DEFAULT_CHECK_DIR)
    parser.add_argument('--outlook', type=Path, default=DEFAULT_OUTLOOK)
    parser.add_argument('--timeout', type=int, default=300)
    args = parser.parse_args(argv)
    try:
        result, code = run_check(
            args.pst, args.check_dir, system or WindowsSystem(), user_notified=args.user_notified,
            expect_sender=args.expect_sender, expect_folders=tuple(args.expect_folder or STANDARD_FOLDERS),
            timeout_seconds=args.timeout, outlook_exe=args.outlook)
    except Refused as error:
        result, code = {'gate': 'ERROR', 'problems': [str(error)]}, 2
    print(json.dumps(result))
    return code


if __name__ == '__main__':
    sys.exit(main())
