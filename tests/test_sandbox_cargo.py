import importlib.util
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "sandbox-cargo.py"
SPEC = importlib.util.spec_from_file_location("sandbox_cargo", SCRIPT)
sandbox_cargo = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sandbox_cargo)


class SandboxCargoTests(unittest.TestCase):
    def test_config_disables_network_and_maps_only_output_writable(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = [root / name for name in ("source", "input", "cache", "rust", "vc", "kits", "output")]
            for path in paths:
                path.mkdir()
            config_path = root / "run.wsb"
            sandbox_cargo.make_config(config_path, *paths)
            config = ET.parse(config_path).getroot()

            self.assertEqual(config.findtext("Networking"), "Disable")
            self.assertEqual(config.findtext("ClipboardRedirection"), "Disable")
            mapped = config.findall("./MappedFolders/MappedFolder")
            self.assertEqual(len(mapped), 7)
            writable = [item.findtext("SandboxFolder") for item in mapped if item.findtext("ReadOnly") != "true"]
            self.assertEqual(writable, [r"C:\PstOffOutput"])

    def test_guest_script_writes_result_marker_and_all_check_commands(self):
        with tempfile.TemporaryDirectory() as temp:
            script = Path(temp) / "guest.cmd"
            commands = sandbox_cargo.make_guest_script(script, (), is_check=True)
            contents = script.read_text()

            self.assertEqual(commands, sandbox_cargo.CHECK_COMMANDS)
            self.assertIn("cargo metadata --locked --format-version 1", contents)
            self.assertIn("cargo fmt", contents)
            self.assertIn("cargo clippy", contents)
            self.assertIn("cargo test", contents)
            self.assertIn('set "CARGO_TARGET_DIR=C:\\PstOffBuild\\target"', contents)
            self.assertIn("robocopy C:\\PstOffBuild\\target C:\\PstOffOutput\\target", contents)
            self.assertIn("if exist C:\\PstOffBuild\\target robocopy", contents)
            self.assertIn("echo %FINAL_EXIT% > C:\\PstOffOutput\\finished.txt", contents)

    def test_guest_probe_checks_network_and_source_write(self):
        with tempfile.TemporaryDirectory() as temp:
            script = Path(temp) / "probe.cmd"
            sandbox_cargo.make_guest_script(script, (), is_check=False, is_probe=True)
            contents = script.read_text()

            self.assertIn("Test-NetConnection", contents)
            self.assertIn("source_write=blocked", contents)
            self.assertIn("C:\\PstOffSource\\sandbox-write-probe.tmp", contents)

    def test_existing_sandbox_process_is_detected(self):
        result = mock.Mock(stdout='"WindowsSandbox.exe","4321","Console","1","12 K"\n')
        with mock.patch.object(sandbox_cargo.subprocess, "run", return_value=result):
            self.assertEqual(sandbox_cargo.sandbox_processes(), [4321])

    def test_unfinished_guest_times_out(self):
        process = mock.Mock()
        process.poll.return_value = None
        with tempfile.TemporaryDirectory() as temp:
            marker = Path(temp) / "finished.txt"
            self.assertEqual(sandbox_cargo.wait_for_guest(process, marker, timeout_seconds=0), "timeout")

    def test_timeout_closes_only_the_launched_sandbox_process(self):
        process = mock.Mock(pid=4321)
        with mock.patch.object(sandbox_cargo.subprocess, "run") as run:
            sandbox_cargo.close_sandbox(process)

        run.assert_called_once()
        self.assertEqual(run.call_args.args[0], ["taskkill", "/PID", "4321", "/T", "/F"])
        process.wait.assert_called_once_with(timeout=30)

    def test_snapshot_excludes_ignored_data_and_target(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "repo"
            root.mkdir()
            (root / ".gitignore").write_text("ignored-data/\n/target/\n")
            (root / "tracked.txt").write_text("tracked")
            (root / "ignored-data").mkdir()
            (root / "ignored-data" / "fixture.bin").write_bytes(b"ignored")
            (root / "target").mkdir()
            (root / "target" / "cache").write_text("build")
            import subprocess

            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", ".gitignore", "tracked.txt"], cwd=root, check=True)
            destination = Path(temp) / "snapshot"
            sandbox_cargo.copy_snapshot(root, destination)

            self.assertEqual((destination / "tracked.txt").read_text(), "tracked")
            self.assertFalse((destination / "ignored-data").exists())
            self.assertFalse((destination / "target").exists())

    def test_run_directory_cleanup_is_confined_to_temp_root(self):
        with tempfile.TemporaryDirectory() as temp:
            run_dir = Path(temp) / "PstOffSandboxCargo-run-test"
            run_dir.mkdir()
            sandbox_cargo.cleanup_run_dir(run_dir, Path(temp))
            self.assertFalse(run_dir.exists())

    def test_run_directory_cleanup_rejects_other_paths(self):
        with tempfile.TemporaryDirectory() as temp:
            run_dir = Path(temp) / "not-a-run"
            run_dir.mkdir()
            with self.assertRaises(RuntimeError):
                sandbox_cargo.cleanup_run_dir(run_dir, Path(temp))
            self.assertTrue(run_dir.exists())


if __name__ == "__main__":
    unittest.main()
