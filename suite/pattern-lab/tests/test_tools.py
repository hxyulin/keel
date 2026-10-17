"""CLI contract tests using scripted fixtures, not real LLVM/native benchmarks."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


CLI = Path(__file__).resolve().parents[1] / "tools.py"
ARTIFACTS = CLI.parents[2] / "target" / "pattern-lab-tools-tests"


class ToolsCLI(unittest.TestCase):
    def setUp(self):
        # Caller may choose another ignored artifact root; no files in /tmp or
        # source directories are needed by these subprocess tests.
        root = Path(os.environ.get("PATTERN_LAB_TEST_ROOT", ARTIFACTS))
        root.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=root)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assembly = self.root / "input with spaces.s"
        self.assembly.write_text("add x0, x0, x1\n")
        self.output = self.root / "report.json"
        self.tool = self.fixture()

    def fixture(self, behavior="", version="print('LLVM fixture version 1\\nRegistered Targets:\\n  aarch64 - AArch64')", help_code="print('fixture help')"):
        tool = self.root / ("fake llvm-mca " + str(len(list(self.root.iterdir()))))
        tool.write_text(f"#!{sys.executable}\n" + textwrap.dedent(f"""\
import sys, time
if '--version' in sys.argv:
    {version}
elif '--help' in sys.argv:
    {help_code}
else:
""") + textwrap.indent(behavior or "print('Iterations: 100\\nTotal Cycles: 101')", "    ") + "\n")
        tool.chmod(0o755)
        return tool

    def invoke(self, *args, command="mca", output=None):
        arguments = [sys.executable, "-B", str(CLI), command,
                     "--output", str(output or self.output), "--llvm-mca", str(self.tool)]
        if command == "mca":
            arguments += ["--assembly", str(self.assembly), "--cpu", "apple-m5"]
        else:
            arguments += ["--llvm-exegesis", str(self.root / "absent-exegesis")]
        completed = subprocess.run(arguments + list(args), capture_output=True, text=True, timeout=20)
        path = output or self.output
        report = json.loads(path.read_text()) if path.exists() else None
        return completed, report

    def test_success_has_exact_evidence_and_unverified_model(self):
        completed, report = self.invoke("--triple", "aarch64-apple-darwin", "--arch", "aarch64", "--iterations", "17")
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["status"], "analyzed")
        self.assertEqual(report["tool"]["sha256"], hashlib.sha256(self.tool.read_bytes()).hexdigest())
        self.assertEqual(report["source"]["sha256"], hashlib.sha256(self.assembly.read_bytes()).hexdigest())
        self.assertEqual(report["execution"]["argv"], [str(self.tool), "-mcpu=apple-m5", "-iterations=17", "-mtriple=aarch64-apple-darwin", "-march=aarch64", str(self.assembly)])
        self.assertEqual(report["model"]["accuracy"], "unverified")
        self.assertEqual(report["metrics"]["total_cycles"], {"value": 101, "unit": "modeled cycles"})
        self.assertEqual(report["tool"]["recognized_target_capability"]["targets"], ["aarch64"])

    def test_discovery_never_analyzes_or_benchmarks(self):
        self.tool = self.fixture("raise RuntimeError('must not execute')")
        completed, report = self.invoke(command="discover")
        self.assertEqual(completed.returncode, 0)
        self.assertFalse(report["native_benchmark_executed"])
        self.assertEqual(report["tools"]["llvm-mca"]["status"], "available")
        self.assertEqual(report["tools"]["llvm-exegesis"]["status"], "missing")

    def test_explicit_missing_tool_has_no_fallback(self):
        self.tool = self.root / "missing"
        completed, report = self.invoke()
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(report["status"], "unsupported")
        self.assertEqual(report["tool"]["status"], "missing")
        self.assertEqual(report["metrics"], {})

    def test_nonexecutable_or_directory_not_accepted(self):
        self.tool.chmod(0o644)
        _, report = self.invoke()
        self.assertEqual(report["tool"]["status"], "missing")
        self.tool = self.root
        _, report = self.invoke(output=self.root / "second.json")
        self.assertEqual(report["tool"]["status"], "missing")

    def test_version_failure(self):
        self.tool = self.fixture(version="print('bad version', file=sys.stderr); sys.exit(3)")
        _, report = self.invoke()
        self.assertEqual(report["status"], "unsupported")
        self.assertEqual(report["tool"]["status"], "version_failure")
        self.assertEqual(report["tool"]["version_probe"]["returncode"], 3)

    def test_empty_version_failure(self):
        self.tool = self.fixture(version="pass")
        _, report = self.invoke()
        self.assertEqual(report["tool"]["status"], "version_failure")

    def test_help_failure_not_invented_capabilities(self):
        self.tool = self.fixture(version="print('LLVM fixture')", help_code="sys.exit(4)")
        _, report = self.invoke(command="discover")
        info = report["tools"]["llvm-mca"]
        self.assertEqual(info["help_probe"]["status"], "failed")
        self.assertEqual(info["recognized_target_capability"]["status"], "unknown")

    def test_nonzero_analysis_preserves_error(self):
        self.tool = self.fixture("print('unsupported instruction', file=sys.stderr); sys.exit(7)")
        _, report = self.invoke()
        self.assertEqual(report["status"], "analysis_failed")
        self.assertEqual(report["execution"]["returncode"], 7)
        self.assertIn("unsupported instruction", report["execution"]["stderr"])
        self.assertEqual(report["metrics"], {})

    def test_unknown_model_zero_exit_fails_closed(self):
        self.tool = self.fixture("print('Total Cycles: 10'); print('ignoring unrecognized processor', file=sys.stderr)")
        _, report = self.invoke()
        self.assertEqual(report["status"], "analysis_failed")
        self.assertEqual(report["metrics"], {})

    def test_stdout_diagnostic_zero_exit_fails_closed(self):
        self.tool = self.fixture("print('warning: instruction ignored\\nTotal Cycles: 10')")
        _, report = self.invoke()
        self.assertEqual(report["status"], "analysis_failed")

    def test_timeout_captures_partial_output(self):
        self.tool = self.fixture("print('partial evidence', flush=True); time.sleep(2)")
        _, report = self.invoke("--timeout", "0.1")
        self.assertEqual(report["status"], "analysis_failed")
        self.assertEqual(report["execution"]["status"], "timeout")
        self.assertIn("partial evidence", report["execution"]["stdout"])

    def test_malformed_summary(self):
        self.tool = self.fixture("print('Total Cycles: nan')")
        _, report = self.invoke()
        self.assertEqual(report["status"], "analysis_failed")

    def test_multiple_regions_do_not_sum(self):
        self.tool = self.fixture("print('Total Cycles: 10\\nTotal Cycles: 20')")
        _, report = self.invoke()
        self.assertEqual(report["status"], "analyzed")
        self.assertEqual(report["metrics"], {})

    def test_invalid_arguments_no_execution(self):
        for option, value in [("--iterations", "0"), ("--iterations", "100001"),
                              ("--iterations", "1.5"), ("--cpu", "x;touch nope"),
                              ("--cpu", "-help"), ("--triple", "a b"),
                              ("--timeout", "nan"), ("--timeout", "61")]:
            with self.subTest(option=option, value=value):
                completed, report = self.invoke(option, value)
                self.assertEqual(completed.returncode, 2)
                self.assertIsNone(report)

    def test_missing_directory_and_oversized_sources(self):
        for source in [self.root / "absent.s", self.root]:
            self.assembly = source
            completed, report = self.invoke(output=self.root / (source.name + ".json"))
            self.assertEqual(completed.returncode, 1)
            self.assertEqual(report["status"], "error")
        self.assembly = self.root / "large.s"
        with self.assembly.open("wb") as stream:
            stream.truncate(4 * 1024 * 1024 + 1)
        _, report = self.invoke()
        self.assertIn("exceeds", report["error"])

    def test_output_never_overwrites_or_follows_symlink(self):
        self.output.write_text('{"prior": true}\n')
        completed, _ = self.invoke()
        self.assertEqual(completed.returncode, 2)
        self.assertEqual(self.output.read_text(), '{"prior": true}\n')
        linked = self.root / "link.json"
        linked.symlink_to(self.output)
        completed, _ = self.invoke(output=linked)
        self.assertEqual(completed.returncode, 2)
        self.assertEqual(self.output.read_text(), '{"prior": true}\n')


if __name__ == "__main__":
    unittest.main()
