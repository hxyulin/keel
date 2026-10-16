#!/usr/bin/env python3
"""Correctness-only serial validation, to run after an explicit root lease grant.
Uses an already built Keel; does not build Rust or measure performance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--retained-hot', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[4]
    out = args.out.resolve()
    out.relative_to(root / 'suite' / 'reports')  # Generated evidence must be ignored.
    out.mkdir(parents=True, exist_ok=False)
    keel = args.keel.resolve()
    fixture = Path(__file__).resolve().parent
    commands = []

    def run(argv):
        argv = [str(a) for a in argv]
        result = subprocess.run(argv, text=True, capture_output=True)
        commands.append(dict(argv=argv, status=result.returncode,
                             stdout=result.stdout, stderr=result.stderr))
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        if result.returncode:
            raise RuntimeError('command failed: ' + repr(argv))
        return result.stdout

    def opt(source, dest, passes=None):
        argv = [keel, 'opt', source, '-o', dest]
        if passes is not None:
            argv += ['-p', passes]
        run(argv)

    scalar = ('inline,simplify-cfg,canonicalize,instcombine,call-contracts,'
              'simplify-cfg,gvn,stack-forward,memory-forward,canonicalize,'
              'instcombine,dce,gcm')
    run([keel, 'cc', '--emit-ir', '-p', scalar, fixture / 'hot.c', '-o', out / 'before.keel'])
    opt(out / 'before.keel', out / 'unroll.keel', 'loop-unroll')
    before = (out / 'before.keel').read_text()
    after = (out / 'unroll.keel').read_text()
    # Assert real C and retained measured IR activation before codegen/native.
    def assert_unrolled(before, after, name):
        original = re.search(r'func @' + name + r'\(.*?\n}', before, re.S).group()
        transformed = re.search(r'func @' + name + r'\(.*?\n}', after, re.S).group()
        assert len(re.findall(r' = load\.', original)) == 1, original
        assert len(re.findall(r' = load\.', transformed)) == 3, transformed
        assert 'icmp.uge' in transformed and ' = isub ' in transformed, transformed
        assert re.search(r'block\d+\(v\d+: i\d+, v\d+: i\d+, v\d+: i\d+\)', transformed), transformed
    for name in ['hot32', 'hot64']:
        assert_unrolled(before, after, name)
    retained = args.retained_hot.resolve()
    (out / 'retained-before.keel').write_bytes(retained.read_bytes())
    opt(out / 'retained-before.keel', out / 'retained-unroll.keel', 'loop-unroll')
    assert_unrolled((out / 'retained-before.keel').read_text(),
                    (out / 'retained-unroll.keel').read_text(), 'hot')
    (out / 'activation.json').write_text(json.dumps(dict(
        fresh_C=['hot32', 'hot64'], retained_IR='hot',
        retained_path=str(retained),
        retained_sha256=hashlib.sha256(retained.read_bytes()).hexdigest()), indent=2) + '\n')
    opt(out / 'unroll.keel', out / 'twice.keel', 'loop-unroll')
    # Parsing renumbers result IDs in layout order (the inserted preheader
    # zero was allocated last). Compare against a no-pass parse/print baseline.
    run([keel, 'opt', '--opt-tier', 'none', out / 'unroll.keel', '-o', out / 'unroll-normalized.keel'])
    assert (out / 'unroll-normalized.keel').read_bytes() == (out / 'twice.keel').read_bytes()
    opt(out / 'unroll.keel', out / 'cleanup.keel')
    run([keel, 'cc', '--emit-ir', '-p', scalar, fixture / 'unsupported.c', '-o', out / 'unsupported.keel'])
    opt(out / 'unsupported.keel', out / 'unsupported-after.keel', 'loop-unroll')
    run([keel, 'opt', '--opt-tier', 'none', out / 'unsupported.keel', '-o', out / 'unsupported-normalized.keel'])
    assert (out / 'unsupported-normalized.keel').read_bytes() == (out / 'unsupported-after.keel').read_bytes()
    for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
        for variant in ['before', 'unroll', 'cleanup', 'retained-unroll']:
            stem = out / (variant + '-' + target)
            run([keel, 'compile', '-O', '--opt-tier', 'none', '--target', target,
                 out / (variant + '.keel'), '-o', stem.with_suffix('.s')])
            run([keel, 'compile', '-O', '--opt-tier', 'none', '--target', target, '--emit-object',
                 out / (variant + '.keel'), '-o', stem.with_suffix('.o')])
    assert platform.system() == 'Darwin' and platform.machine() == 'arm64', 'native driver requires local MacHost'
    common = ['clang', '-O2', '-fno-lto', '-fno-strict-aliasing', '-ffp-contract=off']
    run(common + ['-c', fixture / 'driver.c', '-o', out / 'driver.o'])
    run(common + ['-c', fixture / 'hot.c', '-o', out / 'clang.o'])
    for variant in ['before', 'unroll', 'cleanup', 'clang']:
        obj = out / ('clang.o' if variant == 'clang' else variant + '-aarch64-apple-darwin.o')
        exe = out / (variant + '.exe')
        run(['clang', '-fno-lto', out / 'driver.o', obj, '-o', exe])
        run([exe])
    artifacts = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in out.iterdir() if p.is_file()}
    (out / 'result.json').write_text(json.dumps(dict(
        status='correctness checks passed; no performance measurement',
        compiler_sha256=hashlib.sha256(keel.read_bytes()).hexdigest(),
        artifacts=artifacts), indent=2) + '\n')


if __name__ == '__main__':
    main()
