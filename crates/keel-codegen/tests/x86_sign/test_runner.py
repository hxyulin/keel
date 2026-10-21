"""Portable rejection checks for the Linux export runner (no compiler runs)."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

runner = pathlib.Path(__file__).with_name("run_native.py")
with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    (root / "expected-count.json").write_text("1\n")
    (root / "case.o").write_bytes(b"not executed")
    result = subprocess.run([sys.executable, str(runner), "--dir", str(root)], capture_output=True, text=True)
    assert result.returncode != 0 and "missing matching C oracles" in result.stderr, result
    (root / "case.c").write_text("int main(void){return 0;}\n")
    (root / "expected-count.json").write_text("2\n")
    result = subprocess.run([sys.executable, str(runner), "--dir", str(root)], capture_output=True, text=True)
    assert result.returncode != 0 and "expected 2 objects, found 1" in result.stderr, result
    (root / "case.o").unlink()
    (root / "expected-count.json").write_text("0\n")
    result = subprocess.run([sys.executable, str(runner), "--dir", str(root)], capture_output=True, text=True)
    assert result.returncode != 0 and "expected 0 objects, found 0" in result.stderr, result
print("runner rejects missing C, incorrect count, and empty exports")

with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    (root / "expected-count.json").write_text("1\n")
    for ext in ("o", "c", "s"):
        (root / f"case.{ext}").write_bytes(b"fixture")
    hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in root.glob("case.*")}
    manifest = {"schema": "keel.sign-select-oracles.v1", "stems": ["different"], "sha256": hashes}
    (root / "manifest.json").write_text(json.dumps(manifest))
    result = subprocess.run([sys.executable, str(runner), "--dir", str(root)], capture_output=True, text=True)
    assert result.returncode != 0 and "oracle stems differ" in result.stderr, result
    manifest["stems"] = ["case"]
    manifest["sha256"]["case.o"] = "0" * 64
    (root / "manifest.json").write_text(json.dumps(manifest))
    result = subprocess.run([sys.executable, str(runner), "--dir", str(root)], capture_output=True, text=True)
    assert result.returncode != 0 and "oracle hash mismatch" in result.stderr, result
print("runner rejects substituted names and modified bytes")
