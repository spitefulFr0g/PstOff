import importlib.machinery
import importlib.util
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "check"
SPEC = importlib.util.spec_from_loader("check_script", importlib.machinery.SourceFileLoader("check_script", str(SCRIPT)))
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)


class CheckTests(unittest.TestCase):
    def test_modes_and_failure_propagation(self):
        for args, mode in (([], "--check"), (["--full"], "--check"), (["--fast"], "--check-fast")):
            with self.subTest(args=args), mock.patch.object(check.subprocess, "run", return_value=mock.Mock(returncode=7)) as run:
                self.assertEqual(check.main(args), 7)
                self.assertEqual(run.call_args.args[0][-1], mode)

    def test_conflicting_modes_launch_nothing(self):
        with mock.patch.object(check.subprocess, "run") as run, mock.patch("sys.stderr"):
            with self.assertRaises(SystemExit) as raised:
                check.main(["--fast", "--full"])
            self.assertEqual(raised.exception.code, 2)
            run.assert_not_called()
