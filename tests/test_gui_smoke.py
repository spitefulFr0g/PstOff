import hashlib
import importlib.util
import json
import stat
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "gui-smoke.py"
SPEC = importlib.util.spec_from_file_location("gui_smoke", SCRIPT)
gui_smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gui_smoke)


class GuiSmokeTests(unittest.TestCase):
    def test_guest_conversions_use_the_writable_mapping(self):
        guest = (SCRIPT.parent / "gui-smoke-guest.ps1").read_text()
        self.assertIn('$conversionDir = "$outputDir\\conversions"', guest)
        self.assertIn('$arguments = @(\'--out\', "`"$conversionDir`"")', guest)
        self.assertNotIn('$work\\out', guest)

    def test_guest_captures_held_progress_before_releasing_worker(self):
        guest = (SCRIPT.parent / "gui-smoke-guest.ps1").read_text()
        held = guest.index("No held engine progress event.")
        shot = guest.index("Shot $window '5-converting'")
        release = guest.index('Set-Content -Path "$outputDir\\progress-release.txt"')
        self.assertLess(held, shot)
        self.assertLess(shot, release)

    def test_guest_publishes_a_complete_result_marker(self):
        guest = (SCRIPT.parent / "gui-smoke-guest.ps1").read_text()
        write = guest.index('Set-Content -Path "$outputDir\\finished.tmp"')
        publish = guest.index('Move-Item -LiteralPath "$outputDir\\finished.tmp" -Destination "$outputDir\\finished.txt"')
        self.assertLess(write, publish)
        self.assertNotIn('Set-Content -Path "$outputDir\\finished.txt"', guest)

    def test_outputs_and_sources_survive_success_failure_timeout_and_early_close(self):
        for state, guest_exit in (("finished", 0), ("finished", 1), ("timeout", 1), ("closed", 1)):
            with self.subTest(state=state, guest_exit=guest_exit), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                exe = root / "test.exe"
                exe.write_bytes(b"MZ")
                fixtures = root / "generated"
                fixtures.mkdir()
                (fixtures / "1-mailbox.ost").write_bytes(b"generated source")
                destination = root / "retained"

                def guest(process, marker, timeout):
                    conversions = marker.parent / "conversions"
                    conversions.mkdir()
                    for suffix in ("pst", "json", "txt"):
                        (conversions / f"1-mailbox.{suffix}").write_bytes(b"generated output")
                    marker.write_text(str(guest_exit), encoding="ascii")
                    return state

                with mock.patch.object(gui_smoke.sandbox_cargo, "temp_root", return_value=root), mock.patch.object(
                    gui_smoke.sandbox_cargo, "sandbox_processes", return_value=[]
                ), mock.patch.object(gui_smoke.subprocess, "Popen"), mock.patch.object(
                    gui_smoke.sandbox_cargo, "wait_for_guest", side_effect=guest
                ), mock.patch.object(gui_smoke.sandbox_cargo, "close_sandbox") as close, mock.patch.object(gui_smoke.time, "sleep"):
                    code = gui_smoke.run(exe, fixtures, destination, 5)

                self.assertEqual(code, guest_exit if state == "finished" else 2)
                self.assertEqual(close.call_count, 0 if state == "closed" else 1)
                for suffix in ("pst", "json", "txt"):
                    self.assertEqual((destination / "conversions" / f"1-mailbox.{suffix}").read_bytes(), b"generated output")
                self.assertEqual((destination / "sources" / "1-mailbox.ost").read_bytes(), b"generated source")
                evidence = json.loads((destination / "artifacts.json").read_text())
                self.assertEqual(evidence["conversions/1-mailbox.pst"]["sha256"], hashlib.sha256(b"generated output").hexdigest())
                self.assertEqual(evidence["sources/1-mailbox.ost"]["bytes"], len(b"generated source"))

    def test_retention_failure_preserves_staging_for_recovery(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            exe = root / "test.exe"
            exe.write_bytes(b"MZ")
            fixtures = root / "generated"
            fixtures.mkdir()
            (fixtures / "source.ost").write_bytes(b"generated source")

            def guest(process, marker, timeout):
                (marker.parent / "generated.pst").write_bytes(b"generated output")
                marker.write_text("0", encoding="ascii")
                return "finished"

            with mock.patch.object(gui_smoke.sandbox_cargo, "temp_root", return_value=root), mock.patch.object(
                gui_smoke.sandbox_cargo, "sandbox_processes", return_value=[]
            ), mock.patch.object(gui_smoke.subprocess, "Popen"), mock.patch.object(
                gui_smoke.sandbox_cargo, "wait_for_guest", side_effect=guest
            ), mock.patch.object(gui_smoke.sandbox_cargo, "close_sandbox"), mock.patch.object(
                gui_smoke, "retain_output", side_effect=OSError("disk full")
            ):
                with self.assertRaisesRegex(OSError, "disk full"):
                    gui_smoke.run(exe, fixtures, root / "out", 5)
            staging, = root.glob("PstOffGuiSmoke-run-*")
            self.assertEqual((staging / "output" / "generated.pst").read_bytes(), b"generated output")

    def test_retention_rejects_links_before_copying_guest_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            output = root / "output"
            output.mkdir()
            link = output / "host-file-link"
            link.write_bytes(b"synthetic output")
            source = root / "input" / "fixtures"
            source.mkdir(parents=True)
            destination = root / "retained"
            original_lstat = Path.lstat

            def lstat_with_link(path):
                if path == link:
                    return SimpleNamespace(st_mode=stat.S_IFLNK, st_file_attributes=0)
                return original_lstat(path)

            with mock.patch.object(Path, "lstat", lstat_with_link), mock.patch.object(
                gui_smoke.shutil, "copytree"
            ) as copytree:
                with self.assertRaisesRegex(RuntimeError, "containing a link"):
                    gui_smoke.retain_output(output, root / "input", destination)
            copytree.assert_not_called()

    def test_refuses_to_overwrite_retained_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            destination = root / "retained"
            destination.mkdir()
            (destination / "existing.pst").write_bytes(b"prior synthetic output")
            with mock.patch.object(gui_smoke.sandbox_cargo, "temp_root", return_value=root), mock.patch.object(
                gui_smoke.sandbox_cargo, "sandbox_processes", return_value=[]
            ), mock.patch.object(gui_smoke.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(RuntimeError, "Refusing to overwrite"):
                    gui_smoke.run(root / "unused.exe", root, destination, 5)
            popen.assert_not_called()
            self.assertEqual((destination / "existing.pst").read_bytes(), b"prior synthetic output")

    def test_second_process_check_leaves_unowned_sandbox_untouched(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with mock.patch.object(gui_smoke.sandbox_cargo, "temp_root", return_value=root), mock.patch.object(
                gui_smoke.sandbox_cargo, "sandbox_processes", side_effect=[[], [42]]
            ), mock.patch.object(gui_smoke, "stage_input"), mock.patch.object(
                gui_smoke.subprocess, "Popen"
            ) as popen, mock.patch.object(gui_smoke.sandbox_cargo, "close_sandbox") as close:
                code = gui_smoke.run(root / "unused.exe", root, root / "out", 5)
            self.assertEqual(code, gui_smoke.sandbox_cargo.EXIT_SANDBOX_RUNNING)
            popen.assert_not_called()
            close.assert_not_called()

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
