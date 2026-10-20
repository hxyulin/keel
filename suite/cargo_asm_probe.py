#!/usr/bin/env python3
"""Offline single-CGU Cargo plumbing probe, native Apple ARM only.

Run under suite/fast_opt_locked.py. Uses the system linker in both replay arms;
this does not claim Keel-linker or dependency-rlib coverage.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'suite/compiler_asm_compat'))
import objinspect

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--keel', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    p.error('native Apple ARM required')
out = a.out.resolve()
out.mkdir(parents=True, exist_ok=False)
manifest = ROOT / 'suite/workload-pilots/cargo-smoke/Cargo.toml'
records = []
result = dict(commands=records, scope='one application CGU; shared rustc shim/stdlib and system linker')


def save():
    (out / 'results.json').write_text(json.dumps(result, indent=2) + '\n')


def run(argv):
    c = subprocess.run(list(map(str, argv)), cwd=ROOT, capture_output=True, text=True)
    records.append(dict(argv=list(map(str, argv)), rc=c.returncode, stdout=c.stdout, stderr=c.stderr))
    save()
    return c


result['versions'] = {tool: run([tool, '--version']).stdout for tool in ['cargo', 'rustc', 'clang']}
result['keel_sha256'] = hashlib.sha256(a.keel.read_bytes()).hexdigest()
result['sources'] = {str(f): hashlib.sha256(f.read_bytes()).hexdigest() for f in
                     [manifest, manifest.with_name('Cargo.lock'),
                      ROOT / 'suite/compiler_asm_compat/fixtures/rust/std_generic.rs']}
c = run(['cargo', 'rustc', '--offline', '--locked', '--manifest-path', manifest,
         '--target-dir', out / 'build', '--release', '--', '-C', 'codegen-units=1',
         '-C', 'save-temps', '--emit=asm,obj,link', '--print', 'link-args'])
if c.returncode:
    sys.exit('Cargo baseline failed; see results.json')
link = [shlex.split(line) for line in c.stdout.splitlines() if '-cgu.0.rcgu.o' in line]
assert len(link) == 1, 'expected exactly one final link'
link = link[0]
cgu = [x for x in link if x.endswith('-cgu.0.rcgu.o')]
assert len(cgu) == 1
assembly = Path(cgu[0]).with_suffix('.s')
assert assembly.is_file()
result['assembly'] = dict(path=str(assembly), sha256=hashlib.sha256(assembly.read_bytes()).hexdigest())
baseline = run([out / 'build/release/keel-cargo-smoke'])
assert baseline.returncode == 0
objects = {}
for arm, cmd in {
    'reference': ['clang', '-c', assembly],
    'keel': [a.keel.resolve(), 'asm', '--target', 'aarch64-apple-darwin', assembly],
}.items():
    obj = out / (arm + '.o')
    assembled = run([*cmd, '-o', obj])
    if assembled.returncode:
        result['blocked'] = arm + ' assembly'
        save()
        sys.exit('unsupported assembly; see results.json')
    objects[arm] = obj
    replay = [str(obj) if x == cgu[0] else x for x in link]
    replay[replay.index('-o') + 1] = str(out / arm)
    linked = run(replay)
    if linked.returncode:
        result['blocked'] = arm + ' replay link'
        save()
        sys.exit('replay link failed; see results.json')
    actual = run([out / arm])
    assert (actual.returncode, actual.stdout, actual.stderr) == (baseline.returncode, baseline.stdout, baseline.stderr)
result['structure'] = objinspect.compare(objinspect.parse(objects['reference']), objinspect.parse(objects['keel']))
result['runtime_passed'] = True
save()
print('Cargo baseline + reference-assembled + Keel-assembled application CGU agree; see results.json')
