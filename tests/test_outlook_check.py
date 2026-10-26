import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "outlook-check.py"
SPEC = importlib.util.spec_from_file_location("outlook_check", SCRIPT)
outlook_check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(outlook_check)


def observation(**overrides):
    data = {
        "Opened": True,
        "OpenError": None,
        "StoreName": "Personal Folders",
        "Folders": ["Deleted Items", "Inbox", "Sent Items"],
        "Messages": [],
        "OpenedMessage": None,
        "ReducedFunctionality": False,
        "DataFileClosed": True,
    }
    data.update(overrides)
    return data


class FakeSystem:
    """Stands in for Outlook and the process table; records what the check asked for."""

    def __init__(self, observed=None, running=False, profile=True, driver_hangs=False, exits=True):
        self.observed = observed if observed is not None else observation()
        self.running = running
        self.profile = profile
        self.driver_hangs = driver_hangs
        self.exits = exits
        self.launched = []
        self.driven = []
        self.killed = []
        self.copy_existed_during_drive = None

    def outlook_running(self):
        return self.running

    def profile_exists(self, name):
        return self.profile and name == "PstOffValidation"

    def launch_outlook(self, command):
        self.launched.append(command)
        return 4321

    def drive(self, pid, pst_copy, timeout_seconds):
        self.driven.append((pid, pst_copy))
        self.copy_existed_during_drive = pst_copy.exists()
        if self.driver_hangs:
            raise outlook_check.Hung("driver timed out")
        return self.observed

    def wait_for_exit(self, pid, timeout_seconds):
        return self.exits

    def kill(self, pid):
        self.killed.append(pid)


class OutlookCheckTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.check_dir = self.root / "check"
        self.check_dir.mkdir()
        self.pst = self.root / "out.pst"
        self.pst.write_bytes(b"!BDN synthetic")

    def tearDown(self):
        self.temp.cleanup()

    def run_check(self, system, **kwargs):
        options = {"user_notified": True}
        options.update(kwargs)
        return outlook_check.run_check(self.pst, self.check_dir, system, **options)

    def test_outlook_is_only_started_with_the_validation_profile(self):
        command = outlook_check.outlook_command(Path("C:/Office/OUTLOOK.EXE"))
        self.assertEqual(command[1:], ["/profile", "PstOffValidation"])

        system = FakeSystem()
        self.run_check(system)
        self.assertEqual(len(system.launched), 1)
        self.assertEqual(system.launched[0][1:], ["/profile", "PstOffValidation"])

    def test_refuses_files_under_private_test_data(self):
        private = outlook_check.PRIVATE / "mailbox.pst"
        system = FakeSystem()
        with self.assertRaises(outlook_check.Refused):
            outlook_check.run_check(private, self.check_dir, system, user_notified=True)
        self.assertEqual(system.launched, [])

    def test_refuses_a_check_folder_under_private_test_data(self):
        system = FakeSystem()
        with self.assertRaises(outlook_check.Refused):
            outlook_check.run_check(self.pst, outlook_check.PRIVATE / "check", system, user_notified=True)
        self.assertEqual(system.launched, [])

    def test_refuses_to_run_without_telling_the_user_first(self):
        system = FakeSystem()
        with self.assertRaises(outlook_check.Refused):
            self.run_check(system, user_notified=False)
        self.assertEqual(system.launched, [])

    def test_refuses_to_run_when_outlook_is_already_open(self):
        system = FakeSystem(running=True)
        result, code = self.run_check(system)
        self.assertEqual(code, 2)
        self.assertEqual(result["gate"], "ERROR")
        self.assertEqual(system.launched, [])

    def test_does_not_run_without_the_validation_profile(self):
        system = FakeSystem(profile=False)
        result, code = self.run_check(system)
        self.assertEqual(code, 2)
        self.assertEqual(system.launched, [])

    def test_drives_a_copy_in_the_check_folder_never_the_original(self):
        system = FakeSystem()
        result, code = self.run_check(system)

        (pid, driven), = system.driven
        self.assertEqual(pid, 4321)
        self.assertNotEqual(driven.resolve(), self.pst.resolve())
        self.assertEqual(driven.parent.resolve(), self.check_dir.resolve())
        self.assertTrue(system.copy_existed_during_drive)
        self.assertFalse(driven.exists(), "the copy Outlook wrote to is removed afterwards")
        self.assertTrue(result["InputUnchanged"])
        self.assertEqual(self.pst.read_bytes(), b"!BDN synthetic")

    def test_empty_store_with_the_standard_folders_passes(self):
        result, code = self.run_check(FakeSystem())
        self.assertEqual((result["gate"], code), ("PASS", 0))

    def test_missing_standard_folder_fails(self):
        system = FakeSystem(observation(Folders=["Inbox"]))
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("FAIL", 1))
        self.assertIn("Deleted Items", " ".join(result["problems"]))

    def test_file_outlook_will_not_open_fails(self):
        system = FakeSystem(observation(Opened=False, OpenError="not an Outlook data file", Folders=[]))
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("FAIL", 1))

    def test_readable_sender_and_opened_message_pass(self):
        messages = [{"Folder": "Inbox", "Sender": "Microsoft Outlook", "Subject": "s"}]
        system = FakeSystem(observation(Messages=messages, OpenedMessage={"Sender": "Microsoft Outlook"}))
        result, code = self.run_check(system, expect_sender="Microsoft Outlook")
        self.assertEqual((result["gate"], code), ("PASS", 0))

    def test_garbled_sender_column_fails(self):
        for garbled in ["\ua757\ufffd`", "IPM.Note", "", "a\x01b"]:
            with self.subTest(sender=garbled):
                messages = [{"Folder": "Inbox", "Sender": garbled, "Subject": "s"}]
                system = FakeSystem(observation(Messages=messages, OpenedMessage={"Sender": "x"}))
                result, code = self.run_check(system)
                self.assertEqual((result["gate"], code), ("FAIL", 1))

    def test_unexpected_sender_fails(self):
        messages = [{"Folder": "Inbox", "Sender": "Someone Else", "Subject": "s"}]
        system = FakeSystem(observation(Messages=messages, OpenedMessage={"Sender": "Someone Else"}))
        result, code = self.run_check(system, expect_sender="Microsoft Outlook")
        self.assertEqual((result["gate"], code), ("FAIL", 1))

    def test_messages_listed_but_none_opened_fails(self):
        messages = [{"Folder": "Inbox", "Sender": "Microsoft Outlook", "Subject": "s"}]
        system = FakeSystem(observation(Messages=messages, OpenedMessage=None))
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("FAIL", 1))

    def test_hung_outlook_is_killed_and_reported_as_could_not_run(self):
        system = FakeSystem(driver_hangs=True)
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("ERROR", 2))
        self.assertEqual(system.killed, [4321])
        (_, driven), = system.driven
        self.assertFalse(driven.exists())

    def test_outlook_that_cannot_start_is_reported_as_could_not_run(self):
        system = FakeSystem()

        def missing(command):
            raise FileNotFoundError(command[0])

        system.launch_outlook = missing
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("ERROR", 2))
        self.assertEqual(list(self.check_dir.iterdir()), [])

    def test_outlook_that_will_not_close_is_killed_and_never_passes(self):
        system = FakeSystem(exits=False)
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("ERROR", 2))
        self.assertEqual(system.killed, [4321])

    def test_data_file_left_open_never_passes(self):
        system = FakeSystem(observation(DataFileClosed=False))
        result, code = self.run_check(system)
        self.assertEqual((result["gate"], code), ("ERROR", 2))

    def test_verdict_reports_reduced_functionality(self):
        result, _ = self.run_check(FakeSystem(observation(ReducedFunctionality=True)))
        self.assertIs(result["ReducedFunctionality"], True)
        result, _ = self.run_check(FakeSystem())
        self.assertIs(result["ReducedFunctionality"], False)

    def test_main_prints_one_json_line_and_exits_with_the_gate_code(self):
        import io, json
        from contextlib import redirect_stdout

        out = io.StringIO()
        with redirect_stdout(out):
            code = outlook_check.main(
                [str(self.pst), "--check-dir", str(self.check_dir), "--user-notified"], system=FakeSystem())
        lines = out.getvalue().splitlines()
        self.assertEqual(len(lines), 1)
        self.assertEqual(json.loads(lines[0])["gate"], "PASS")
        self.assertEqual(code, 0)


class DriverScriptTests(unittest.TestCase):
    """The PowerShell UI driver can't run here, so pin its safety properties statically."""

    def setUp(self):
        self.text = (SCRIPT.parent / "outlook-check.ps1").read_text(encoding="utf-8")

    def test_driver_uses_only_built_in_ui_automation_assemblies(self):
        self.assertIn("Add-Type -AssemblyName UIAutomationClient", self.text)
        self.assertNotIn("Install-Module", self.text)
        self.assertNotIn("Import-Module", self.text)

    def test_driver_never_starts_outlook_or_uses_addstoreex(self):
        lowered = self.text.lower()
        self.assertNotIn("start-process", lowered)
        self.assertNotIn("addstoreex", lowered)
        self.assertNotIn("outlook.application", lowered)

    def test_driver_attaches_only_to_the_launched_process(self):
        self.assertIn("[Parameter(Mandatory=$true)][int]$ProcessId", self.text)
        self.assertIn("ProcessIdProperty", self.text)


if __name__ == "__main__":
    unittest.main()
