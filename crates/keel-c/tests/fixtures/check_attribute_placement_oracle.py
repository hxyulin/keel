"""Recheck the retained ROUND3 named-attachment oracle with local Clang.

Usage: python3 crates/keel-c/tests/fixtures/check_attribute_placement_oracle.py
Sources, ASTs and diagnostics go under target/tmp/round3-correction-validation.
The Rust regression separately asserts 42 accepted and 23 unsupported rows.
"""
from pathlib import Path
import json
import subprocess

fixture = Path(__file__).resolve().parent
root = fixture.parents[3]
out = root / "target/tmp/round3-correction-validation/clang-oracle"
rows = (fixture / "attribute-placement-oracle.tsv").read_text().splitlines()
assert len(rows) == 65
attributes = {
    "noinline": "NoInlineAttr",
    "always_inline": "AlwaysInlineAttr",
    "nothrow": "NoThrowAttr",
    "returns_twice": "ReturnsTwiceAttr",
}
for attr, kind in attributes.items():
    dest = out / attr
    dest.mkdir(parents=True, exist_ok=True)
    for i, line in enumerate(rows):
        expect, source = line.split("\t", 1)
        c = dest / f"{i:02}.c"
        c.write_text(source.replace("noinline", attr))
        probe = subprocess.run(
            ["clang", "-std=c11", "-fsyntax-only", "-Xclang", "-ast-dump=json", str(c)],
            text=True, capture_output=True,
        )
        c.with_suffix(".json").write_text(probe.stdout)
        c.with_suffix(".stderr").write_text(probe.stderr)
        assert probe.returncode == 0, (attr, i, probe.stderr)
        decl = [x for x in json.loads(probe.stdout)["inner"] if x.get("name") == "h"][-1]
        assert decl["kind"] == "FunctionDecl"
        named = any(x.get("kind") == kind for x in decl.get("inner", []))
        assert named == (expect == "1"), (attr, i, decl)
    print(f"{attr}: 65 Clang FunctionDecl rows; named attachment matches retained oracle")
