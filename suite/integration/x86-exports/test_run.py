"""Packet runner contracts, with a fake C compiler and a fake /proc/cpuinfo."""
import contextlib
import io
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402

# Writes a shell program that prints the first input file and exits with the
# status named in the second, if any.
FAKE_CC = """#!/bin/sh
out=program
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2;;
    *) inputs="$inputs $1"; shift;;
  esac
done
set -- $inputs
printf '#!/bin/sh\\ncat %s\\nexit %s\\n' "$1" "$(cat "${2:-/dev/null}" 2>/dev/null || echo 0)" > "$out"
chmod +x "$out"
"""


class Runner(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.exports = self.root / "exports"
        self.cc = self.root / "fake-cc"
        self.cc.write_text(FAKE_CC)
        self.cc.chmod(self.cc.stat().st_mode | stat.S_IEXEC)
        self.cpuinfo = self.root / "cpuinfo"
        self.cpuinfo.write_text("processor\t: 0\nflags\t\t: fpu sse2 pni sse4_2 avx avx2\n")

    def tearDown(self):
        self.temporary.cleanup()

    def packet(self, name, text="hi\n", status=None, **manifest):
        directory = self.exports / "keel-codegen" / "x86.case" / name
        directory.mkdir(parents=True)
        (directory / "out.txt").write_text(text)
        files = ["out.txt"]
        inputs = ["out.txt"]
        if status is not None:
            (directory / "status").write_text(str(status))
            files.append("status")
            inputs.append("status")
        body = dict(version=1, test="keel-codegen::x86::case", name=name, features=[],
                    files=files, build=[["cc", *inputs, "-o", "program"]],
                    run=["./program"], exit=0, stdout=None)
        body.update(manifest)
        (directory / "manifest.json").write_text(json.dumps(body))
        return directory

    def main(self, *extra):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            code = run.main([str(self.exports), "--cc", str(self.cc), "--any-host",
                             "--cpuinfo", str(self.cpuinfo), "--jobs", "2", *extra])
        return code, output.getvalue()

    def test_passing_failing_and_skipped_packets_are_counted(self):
        self.packet("pass", stdout="hi\n", features=["sse4.2", "avx2"])
        self.packet("wrong-status", status=3)
        self.packet("wrong-stdout", stdout="bye\n")
        self.packet("expected-status", status=7, exit=7)
        self.packet("needs-avx512", features=["avx512f"])
        self.packet("needs-tool", build=[["keel-no-such-tool"]])
        code, output = self.main()
        self.assertEqual(code, 1, output)
        self.assertIn("2 passed, 2 failed, 2 skipped of 6 packets", output)
        self.assertIn("FAIL keel-codegen/x86.case/wrong-status", output)
        self.assertIn("FAIL keel-codegen/x86.case/wrong-stdout", output)
        self.assertIn("skipped 1: host lacks avx512f", output)
        self.assertIn("skipped 1: keel-no-such-tool not found", output)

    def test_all_passing_exits_zero_and_filter_selects(self):
        self.packet("one", stdout="hi\n")
        self.packet("two", status=1)
        code, output = self.main("--filter", "one")
        self.assertEqual(code, 0, output)
        self.assertIn("1 passed, 0 failed, 0 skipped of 1 packets", output)

    def test_bad_manifest_and_missing_file_fail(self):
        self.packet("version", version=2)
        self.packet("unknown-feature", features=["avx513"])
        missing = self.packet("missing")
        (missing / "out.txt").unlink()
        code, output = self.main()
        self.assertEqual(code, 1)
        self.assertIn("unsupported manifest version 2", output)
        self.assertIn("unknown CPU features ['avx513']", output)
        self.assertIn("missing file out.txt", output)

    def test_keep_leaves_build_outputs(self):
        self.packet("kept", stdout="hi\n")
        keep = self.root / "keep"
        code, output = self.main("--keep", str(keep))
        self.assertEqual(code, 0, output)
        self.assertTrue((keep / "keel-codegen" / "x86.case" / "kept" / "program").is_file())

    def test_no_packets_fails(self):
        self.exports.mkdir()
        code, output = self.main()
        self.assertEqual(code, 1)
        self.assertIn("no packets", output)

    def test_feature_names_follow_cpuinfo_spelling(self):
        flags = run.host_flags(self.cpuinfo)
        self.assertEqual(run.missing_features(["sse3", "sse4.2", "lzcnt"], flags), ["lzcnt"])


if __name__ == "__main__":
    unittest.main()
