import importlib.util
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "gui-smoke.py"
SPEC = importlib.util.spec_from_file_location("gui_smoke", SCRIPT)
gui_smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gui_smoke)


class GuiSmokeTests(unittest.TestCase):
    def test_config_maps_only_inputs_read_only_and_output_writable(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "input").mkdir()
            (root / "output").mkdir()
            gui_smoke.make_config(root / "smoke.wsb", root / "input", root / "output")
            config = ET.parse(root / "smoke.wsb").getroot()

            self.assertEqual(config.findtext("Networking"), "Disable")
            self.assertEqual(config.findtext("ClipboardRedirection"), "Disable")
            mapped = config.findall("./MappedFolders/MappedFolder")
            self.assertEqual(
                [(m.findtext("SandboxFolder"), m.findtext("ReadOnly")) for m in mapped],
                [(gui_smoke.GUEST_INPUT, "true"), (gui_smoke.GUEST_OUTPUT, "false")],
            )
            self.assertIn("gui-smoke-guest.ps1", config.findtext("./LogonCommand/Command"))

    def test_staged_input_holds_only_the_exe_script_and_fixtures(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            exe = root / "pstoff-gui.exe"
            exe.write_bytes(b"MZ")
            fixtures = root / "fixtures"
            fixtures.mkdir()
            (fixtures / "a.ost").write_bytes(b"synthetic")
            (fixtures / "notes.txt").write_text("ignored")
            gui_smoke.stage_input(root / "input", exe, fixtures)

            staged = sorted(p.relative_to(root / "input").as_posix() for p in (root / "input").rglob("*"))
            self.assertEqual(
                staged,
                ["fixtures", "fixtures/a.ost", "gui-smoke-guest.ps1", "pstoff-gui.exe"],
            )

    def test_refuses_to_start_beside_another_sandbox(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.object(
            gui_smoke.sandbox_cargo, "temp_root", return_value=Path(temp)
        ), mock.patch.object(gui_smoke.sandbox_cargo, "sandbox_processes", return_value=[42]), mock.patch.object(
            gui_smoke.subprocess, "Popen"
        ) as popen:
            code = gui_smoke.run(Path(temp) / "x.exe", Path(temp), Path(temp) / "out", 5)

        self.assertEqual(code, gui_smoke.sandbox_cargo.EXIT_SANDBOX_RUNNING)
        popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
