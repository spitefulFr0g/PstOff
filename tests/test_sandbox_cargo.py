import importlib.util
import os
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "sandbox-cargo.py"
SPEC = importlib.util.spec_from_file_location("sandbox_cargo", SCRIPT)
sandbox_cargo = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sandbox_cargo)


class SandboxCargoTests(unittest.TestCase):
    def test_fast_mode_skips_only_the_large_memory_gate(self):
        self.assertEqual(sandbox_cargo.check_commands(), sandbox_cargo.CHECK_COMMANDS)
        fast = sandbox_cargo.check_commands(fast=True)
        for full_command, fast_command in zip(sandbox_cargo.CHECK_COMMANDS, fast):
            if full_command == ("test", "--locked", "--workspace"):
                self.assertEqual(fast_command, full_command + (
                    "--", "--skip", "large_generated_ost_has_fixed_separate_process_peak"))
            else:
                self.assertEqual(fast_command, full_command)
        with tempfile.TemporaryDirectory() as temp:
            script = Path(temp) / "guest.cmd"
            sandbox_cargo.make_guest_script(script, (), is_check=True, fast=True)
            self.assertIn("--skip large_generated_ost_has_fixed_separate_process_peak", script.read_text())
            self.assertIn("python-tests.exit", script.read_text())

    def test_progress_reports_before_completion_and_retries_partial_markers(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch("builtins.print") as output:
            root = Path(temp)
            reporter = sandbox_cargo.GuestProgress(root)
            (root / "stage.txt").write_text("cargo-3: test", encoding="ascii")
            (root / "cargo-2.exit").write_text("", encoding="ascii")
            reporter()
            self.assertNotIn("cargo-2.exit", reporter.completed)
            (root / "cargo-2.exit").write_text("1", encoding="ascii")
            reporter()
            reporter()
            messages = [call.args[0] for call in output.call_args_list]
            self.assertTrue(any("cargo-3: test" in message for message in messages))
            self.assertEqual(sum("cargo-2 finished: exit 1" in message for message in messages), 1)
            self.assertFalse((root / "finished.txt").exists())
            with mock.patch.object(sandbox_cargo.time, "monotonic", return_value=reporter.last_report + 31):
                reporter()
            self.assertIn("cargo-3: test", output.call_args.args[0])

    def test_checks_cover_every_workspace_crate(self):
        commands = sandbox_cargo.CHECK_COMMANDS
        self.assertIn(("fmt", "--all", "--", "--check"), commands)
        self.assertIn(("test", "--locked", "--workspace"), commands)
        clippy = next(command for command in commands if command[0] == "clippy")
        self.assertIn("--workspace", clippy)
        self.assertIn("--all-targets", clippy)

    def test_python_tests_run_in_guest_and_propagate_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            script = Path(temp) / "guest.cmd"
            sandbox_cargo.make_guest_script(script, (), is_check=True)
            contents = script.read_text()
            self.assertIn(r"C:\PstOffPython\python.exe -I -S -B -m unittest discover -s C:\PstOffSource\tests -v", contents)
            self.assertIn(r"echo %STEP_EXIT% > C:\PstOffOutput\python-tests.exit", contents)
            self.assertIn('if not %STEP_EXIT%==0 set "FINAL_EXIT=%STEP_EXIT%"', contents)
            for i in range(len(sandbox_cargo.CHECK_COMMANDS)):
                (Path(temp) / f"cargo-{i}.exit").write_text("0", encoding="ascii")
            with mock.patch("builtins.print"):
                self.assertEqual(sandbox_cargo.print_guest_output(Path(temp), is_check=True), 2)
                (Path(temp) / "python-tests.exit").write_text("1", encoding="ascii")
                self.assertEqual(sandbox_cargo.print_guest_output(Path(temp), is_check=True), 1)

    def test_python_and_git_are_only_readonly_check_mappings(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            config_path = root / "run.wsb"
            sandbox_cargo.make_config(config_path, *[root / name for name in (
                "source", "input", "cache", "rust", "vc", "kits", "output", "python", "git"
            )])
            config = ET.parse(config_path).getroot()
            mappings = {m.findtext("SandboxFolder"): m.findtext("ReadOnly") for m in config.findall("./MappedFolders/MappedFolder")}
            self.assertEqual(mappings[r"C:\PstOffPython"], "true")
            self.assertEqual(mappings[r"C:\PstOffGit"], "true")
            self.assertEqual([path for path, readonly in mappings.items() if readonly != "true"], [r"C:\PstOffOutput"])

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
        result = mock.Mock(
            returncode=0,
            stdout='"WindowsSandbox.exe","4321","Console","1","12 K"\n',
        )
        with mock.patch.object(sandbox_cargo.subprocess, "run", return_value=result):
            self.assertEqual(sandbox_cargo.sandbox_processes(), [4321])

    def test_process_enumeration_failure_is_not_treated_as_no_sandbox(self):
        result = mock.Mock(returncode=1, stdout="", stderr="Access denied")
        with mock.patch.object(sandbox_cargo.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(RuntimeError, "Could not establish"):
                sandbox_cargo.sandbox_processes()

    def test_malformed_matching_process_record_is_rejected(self):
        for stdout in (
            '"WindowsSandbox.exe"\n',
            '"WindowsSandbox.exe","not-a-pid","Console","1","12 K"\n',
            '"WindowsSandbox.exe","0","Console","1","12 K"\n',
        ):
            result = mock.Mock(returncode=0, stdout=stdout, stderr="")
            with self.subTest(stdout=stdout):
                with mock.patch.object(sandbox_cargo.subprocess, "run", return_value=result):
                    with self.assertRaisesRegex(RuntimeError, "Malformed WindowsSandbox"):
                        sandbox_cargo.sandbox_processes()

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

    def test_snapshot_refreshes_backdated_edits_before_restoring_cargo_cache(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "repo"
            root.mkdir()
            source = root / "changed.rs"
            source.write_text("updated test source")
            # Model an edit made while an earlier guest was still compiling.
            os.utime(source, (1, 1))
            listed = mock.Mock(stdout=b"changed.rs\0")
            started = time.time()
            with mock.patch.object(sandbox_cargo.subprocess, "run", return_value=listed):
                sandbox_cargo.copy_snapshot(root, Path(temp) / "snapshot")
            staged = Path(temp) / "snapshot" / "changed.rs"
            self.assertEqual(staged.read_text(), "updated test source")
            self.assertGreaterEqual(staged.stat().st_mtime, started - 1)

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
