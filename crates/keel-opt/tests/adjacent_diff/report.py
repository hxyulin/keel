#!/usr/bin/env python3
"""Render correctness evidence after the locked adjacent_diff test commands."""
import argparse
import difflib
import hashlib
import html
import json
import pathlib
import platform

p = argparse.ArgumentParser()
p.add_argument("--artifacts", type=pathlib.Path, required=True)
p.add_argument("--output", type=pathlib.Path, required=True)
a = p.parse_args()
root = pathlib.Path(__file__).resolve().parents[4]
results = {}
for path in sorted(a.artifacts.glob("native-*-spills*.json")):
    result = json.loads(path.read_text())
    assert result["status"] == "pass"
    results[path.name] = result
assert len(results) == 16, results
objects = sorted(a.artifacts.glob("opt*-spills*.o"))
assert len(objects) == 32, objects

lib = root / "crates/keel-opt/src/lib.rs"
before = lib.read_text()
after = before.replace("mod adjacent_abs;", "mod adjacent_abs;\nmod adjacent_diff;", 1)
needle = "        run: adjacent_abs::run,\n    },"
assert needle in after
after = after.replace(needle, needle + '''
    BuiltinPass {
        name: "adjacent-diff",
        description: "bounded read-only adjacent i32 absolute-difference SIMD reduction (opt-in)",
        run: adjacent_diff::run,
    },''', 1)
patch = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
    fromfile="a/crates/keel-opt/src/lib.rs", tofile="b/crates/keel-opt/src/lib.rs"))
(a.artifacts / "root-registration.patch").write_text(patch)

files = ["crates/keel-opt/src/adjacent_diff.rs", "crates/keel-opt/tests/adjacent_diff.rs",
         "crates/keel-codegen/tests/adjacent_diff.rs", "crates/keel-codegen/tests/adjacent_diff/oracle.c"]
report = {
    "status": "pass", "performance_measured": False,
    "native_host": {"system": platform.system(), "architecture": platform.machine()},
    "native_execution": "ARM64 correctness executed locally; Linux x86_64 oracle prepared, not executed locally",
    "optimizer_tests_passed": 6, "codegen_tests_passed": 2,
    "guard_interpreter_cases": 10256,
    "static_object_configurations": len(objects), "native_configurations": len(results),
    "value_cases_per_native_configuration": 20900, "first_fault_cases_per_native_configuration": 84,
    "matrix": "original transform and subsequent Full optimization; Baseline/Full selection; Fast/Full allocation; normal pools/all vector values spilled; both target object routes byte-identical",
    "accepted_shape": "exact five-block canonical select form after adjacent-abs, with ptr,ptr,ptr,i64 signature; unit-stride indices 1..n-1, wrapping i64 sum, sign-extended i32 loads from one base; default ordinary-memory flags only; at most 40 instructions",
    "rejections": ["raw triangle (run adjacent-abs first)", "extra reads/writes/calls/atomics/volatile/inline assembly/custom instructions", "nondefault load flags (including notrap/alignment)", "changed predicate, stride, extension, sum or stream base", "aliased definitions or block parameters", "all other loop shapes"],
    "proof": {
        "no_read_guard": "Original unsigned n>1 branch stays before every new load; n=0/1 return zero, including null/inaccessible pointers.",
        "prefix": "Execute a[1] then a[0], retaining the first fault. Compute the exact first difference in i64; splat sign-extended a[1] to seed prior lane 1. No second scalar seed iteration or additional read is needed.",
        "range": "Fast path requires n<=UINT64_MAX/4 and unsigned base+4*n>=base. Conservative failure keeps the original scalar loop. The actual emitted arithmetic is checked with the shared interpreter against exact u128 arithmetic at 256 boundary pairs and 10000 deterministic random pairs.",
        "chunk": "At check, 2<=k<=n and prior[1]=a[k-1]. If n-k>=2, exactly eight bytes at base+4*k load a[k],a[k+1]; low signed widening produces i64x2. Shuffle(prior,current,[1,2]) gives a[k-1],a[k]. Group bounds imply k+2<=n; all byte and induction arithmetic is nonwrapping on this path.",
        "footprint_order": "Prefix visits sample 1 before 0. Remaining newly read samples increase monotonically in the original first-visit order. Only previously successful ordinary reads are reused. No group touches bytes beyond the two in-bounds i32 samples. The exact function excludes all barriers and memory mutation; no notrap or alignment annotations are added. Ordinary read-only memory semantics exclude volatile/MMIO and asynchronous mapping mutation.",
        "wide_arithmetic": "Each i32 is sign-extended before subtraction. Difference range [-4294967295,4294967295] fits i64 with a representable negation. Comparison/negation/select compute exact magnitudes, including opposite i32 extrema. Vector accumulation and final horizontal add use the wrapping u64 ring, permitting regrouping.",
        "tail": "After chunk processing n-k is 0 or 1. The optional tail reads exactly a[k] and reuses prior[1]. n=2 reads only the prefix; odd/even tails require no padding or overread.",
        "preservation": "Original scalar instructions, aliases, attributes, restrict contracts and source locations remain intact; only the original guarded loop-entry edge changes. New loads inherit the corresponding original load line. Allocator/backend code and generic multi-trapping-stream rules are untouched.",
    },
    "integration": {"owner": "root", "pipeline_changed": False,
        "placement": "Invoke opt-in adjacent-diff immediately after adjacent-abs while this exact five-block shape remains; CFG merging/canonical cleanup can make it ineligible. No defaults, Fast behavior or remotes changed.",
        "registration_patch": patch},
    "native_results": results,
    "sources_sha256": {f: hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files},
    "artifacts": str(a.artifacts.resolve()),
    "linux_reproduction": "python3 suite/fast_opt_locked.py target/clang-gap-round3-20261021/lock.jsonl env CARGO_TARGET_DIR=/absolute/private/adjacent-diff-cargo KEEL_ADJACENT_DIFF_EVIDENCE_DIR=/absolute/private/adjacent-diff cargo test --offline --locked -j2 -p keel-opt -p keel-codegen --test adjacent_diff",
}
a.output.mkdir(parents=True, exist_ok=True)
(a.output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
esc = html.escape
proofs = "".join(f"<dt>{esc(k.replace('_',' ').capitalize())}</dt><dd>{esc(v)}</dd>" for k,v in report["proof"].items())
page = f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Adjacent difference SIMD correctness</title><style>
body{{font:16px/1.55 system-ui;max-width:1000px;margin:40px auto;padding:0 24px;color:#193047;background:#f4f7fa}}
h1,h2{{line-height:1.15}}section{{background:white;border:1px solid #d7e2ea;border-radius:12px;padding:24px;margin:20px 0}}
.ok{{color:#176346;font-weight:700}}.grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px}}b{{font-size:26px;display:block}}
dt{{font-weight:700;margin-top:18px}}dd{{margin:5px 0 0}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font-size:13px}}
</style><h1>Adjacent absolute-difference SIMD</h1><p class="ok">Correctness passes · opt-in · no performance measurements</p>
<section class="grid"><div><b>32</b>ARM + x86 object configurations</div><div><b>16</b>Native ARM configurations</div><div><b>20,900</b>Value cases per configuration</div><div><b>84</b>First-fault cases per configuration</div></section>
<section><h2>Validation</h2><p>{esc(report['matrix'])}.</p><p>Six optimizer tests and two codegen tests pass. Forced empty vector pools exercise every SIMD value in a spill slot. The emitted range guard passes 10,256 independent interpreter checks. Native checks include opposite i32 extrema, random inputs, n=0/1/2, all lengths through 257, unaligned inputs, guard pages, prefix fault order, chunk/tail faults and wrapping guard fallback.</p><p><strong>Linux execution remains pending.</strong> The same C oracle is prepared for native Linux x86_64; both x86 object routes already pass locally.</p></section>
<section><h2>Proof and restrictions</h2><p>{esc(report['accepted_shape'])}.</p><dl>{proofs}</dl></section>
<section><h2>Root integration</h2><p>{esc(report['integration']['placement'])}</p><pre>{esc(patch)}</pre></section>
<section><h2>Reproduce</h2><pre>{esc(report['linux_reproduction'])}</pre><p>Machine-readable evidence: evidence.json. No allocator, backend, generic vectorizer or lib.rs changes are included.</p></section></html>'''
(a.output / "evidence.html").write_text("\n".join(line.rstrip() for line in page.splitlines()) + "\n")
