#!/usr/bin/env python3
"""Local static emission and ARM correctness, never benchmarks or remotes.

Run from the checkout root after the adjacent_abs integration test has emitted
target/clang-gap-round2-20261021/adjacent-after.keel. Every compiler/native
invocation is serialized by the required campaign wrapper.
"""
import json
import pathlib
import platform
import subprocess

ROOT = pathlib.Path.cwd()
OUT = ROOT / 'target/clang-gap-round2-20261021'
FIX = ROOT / 'crates/keel-opt/tests/adjacent_abs'
KEEL = ROOT / 'target/adjacent-cargo/debug/keel'
LOCK = ['python3', 'suite/fast_opt_locked.py', str(OUT / 'lock.jsonl')]


def locked(*args):
    return subprocess.run(LOCK + list(map(str, args)), check=True,
                          capture_output=True, text=True).stdout


def main():
    before = OUT / 'adjacent-before.keel'
    before.write_text((FIX / 'adjacent.keel').read_text().replace(
        '@rk_adjacent(', '@rk_adjacent_before('))
    results = []
    for arch, triple in [('arm', 'aarch64-apple-darwin'),
                         ('x86', 'x86_64-unknown-linux-gnu')]:
        for allocation in ['fast', 'full']:
            for selector in ['none', 'folds', 'all']:
                label = f'{arch}-{allocation}-{selector}'
                common = ['--target', triple, '--regalloc', allocation,
                          '--isel', selector]
                assemblies = []
                for variant, src in [('before', before),
                                     ('after', OUT / 'adjacent-after.keel')]:
                    asm = OUT / f'{label}-{variant}.s'
                    obj = OUT / f'{label}-{variant}.o'
                    # Full pipeline is intentionally applied AFTER the new pass;
                    # no registration/default schedule is changed by this test.
                    locked(KEEL, 'compile', '-O', *common, src, '-o', asm)
                    locked(KEEL, 'compile', '-O', *common, '--emit-object', src, '-o', obj)
                    assemblies.append(asm)
                result = dict(arch=arch, allocation=allocation, selector=selector,
                              assembly=True, object=True, native=None)
                if arch == 'arm' and platform.machine() == 'arm64':
                    exe = OUT / f'{label}-correctness'
                    locked('clang', '-O2', '-Wall', '-Wextra', '-Werror',
                           FIX / 'native.c', *assemblies, '-o', exe)
                    result['native'] = json.loads(locked(exe))
                results.append(result)
    (OUT / 'validation.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(dict(configurations=len(results),
                          native_arm=sum(r['native'] is not None for r in results),
                          timed=False)))


if __name__ == '__main__':
    main()
