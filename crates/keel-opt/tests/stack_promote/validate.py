#!/usr/bin/env python3
"""Serial correctness/static validation. No timing. Run from the worktree root.
Every compiler, native run and check subprocess acquires the investigation lock.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--keel', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('--investigation', type=Path, required=True)
p.add_argument('--lock-wrapper', type=Path, required=True)
p.add_argument('--lock-log', type=Path, required=True)
p.add_argument('--if-convert', action='store_true')
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
a.lock_log.parent.mkdir(parents=True, exist_ok=True)
fixture = Path(__file__).resolve().parent
keel, out = a.keel.resolve(), a.out.resolve()
commands = []
env = dict(os.environ)
def run(argv):
    argv = [str(x) for x in argv]
    r = subprocess.run(['python3', str(a.lock_wrapper), str(a.lock_log), *argv],
                       text=True, capture_output=True, env=env)
    commands.append(dict(argv=argv, status=r.returncode, stdout=r.stdout, stderr=r.stderr))
    (out / 'commands.json').write_text(json.dumps(commands, indent=2)+'\n')
    if r.returncode:
        raise RuntimeError(repr(argv)+'\n'+r.stderr)
    return r.stdout

def opt(src, dst, passes):
    run([keel, 'opt', '--verify-each', *(['-p', passes] if passes else ['--opt-tier', 'none']), src, '-o', dst])

promotion = 'stack-promote,phi-simplify,dce'
combined = promotion + (',if-convert,simplify-cfg,dce' if a.if_convert else '')
source = fixture / 'private.keel'
opt(source, out / 'promoted.keel', promotion)
opt(source, out / 'combined.keel', combined)
private = (out / 'promoted.keel').read_text()
assert len(re.findall(r' = load\.', private)) == 1, private  # input only
assert not re.search(r'\bstore\b', private), private
for tier in ['fast', 'full']:
    run([keel, 'opt', '--opt-tier', tier, '--verify-each', out / 'combined.keel', '-o', out / (tier+'.keel')])

# Real corpus: preserve the ordinary escaping memset. No claim of promotion.
real = a.investigation / 'raw.keel'
run([keel, 'opt', '--opt-tier', 'full', real, '-o', out / 'corpus-before.keel'])
opt(out / 'corpus-before.keel', out / 'corpus-after.keel', combined)
get_stack = lambda s: re.search(r'func @k_stackobj\(.*?\n}', s, re.S).group()
before = get_stack((out / 'corpus-before.keel').read_text())
after = get_stack((out / 'corpus-after.keel').read_text())
counts = lambda s: dict(loads=len(re.findall(r' = load\.', s)), stores=len(re.findall(r'\bstore\b', s)), branches=len(re.findall(r'\bbrif\b', s)))
assert counts(before) == counts(after), (before, after)
assert 'call @memset' in after
variants = dict(before=source, promoted=out/'promoted.keel', combined=out/'combined.keel',
                fast=out/'fast.keel', full=out/'full.keel')
# Match small-memory's 64-bit zeroing of the final 32-bit counter plus
# padding. Run the identical unsigned oracle before/after and through Full.
mixed = source.read_text()
assert mixed.count('store v3, v68') == 1
mixed = mixed.replace('store v3, v68', 'store v64, v68')
(out/'mixed-before.keel').write_text(mixed)
opt(out/'mixed-before.keel', out/'mixed-promoted.keel', promotion)
run([keel, 'opt', '--opt-tier', 'full', '--verify-each', out/'mixed-before.keel',
     '-o', out/'mixed-full.keel'])
for name in ['mixed-before', 'mixed-promoted', 'mixed-full']:
    variants[name] = out/(name+'.keel')
    if name != 'mixed-before':
        text = variants[name].read_text()
        assert counts(text)['loads'] == 1, text
        assert counts(text)['stores'] == 0, text
for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
    for name, src in variants.items():
        for ext, flag in [('s', []), ('o', ['--emit-object'])]:
            run([keel, 'compile', '-O', '--opt-tier', 'none', '--target', target, *flag, src,
                 '-o', out/(name+'-'+target+'.'+ext)])
    for name in ['corpus-before', 'corpus-after']:
        run([keel, 'compile', '-O', '--opt-tier', 'none', '--target', target,
             out/(name+'.keel'), '-o', out/(name+'-'+target+'.s')])
host = (platform.system(), platform.machine())
native_target = {('Darwin', 'arm64'): 'aarch64-apple-darwin',
                 ('Linux', 'x86_64'): 'x86_64-unknown-linux-gnu'}.get(host)
assert native_target, f'unsupported native oracle host: {host}'
run(['clang', '-O2', '-fno-lto', '-c', fixture/'oracle.c', '-o', out/'oracle.o'])
for name in variants:
    exe = out/(name+'.exe')
    run(['clang', out/'oracle.o', out/(name+'-'+native_target+'.o'), '-o', exe])
    run([exe])
# Independent whole-corpus oracle: fresh Clang output, no historical objects.
driver = Path('crates/keel-cranelift/kernels/driver.c')
if host[0] == 'Linux':
    text = driver.read_text()
    original = 'static u64 now(void) { return clock_gettime_nsec_np(CLOCK_UPTIME_RAW); }'
    replacement = 'static u64 now(void) { struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts); return (u64)ts.tv_sec * 1000000000ULL + (u64)ts.tv_nsec; }'
    assert text.count(original) == 1
    driver = out/'driver-linux.c'
    driver.write_text(text.replace(original, replacement))
run(['clang', '-O2', '-fwrapv', '-fno-lto', '-ffp-contract=off', '-c', driver, '-o', out/'driver.o'])
run(['clang', '-O2', '-fwrapv', '-fno-lto', '-ffp-contract=off', '-c', 'crates/keel-cranelift/kernels/kernels.c', '-o', out/'clang.o'])
run(['clang', out/'driver.o', out/'clang.o', '-o', out/'clang-check'])
expected = run([out/'clang-check', 'check'])
for tier in ['fast', 'full']:
    base = out/('whole-'+tier+'.keel')
    run([keel, 'opt', '--opt-tier', tier, real, '-o', base])
    opt(base, out/('whole-'+tier+'-after.keel'), combined)
    run([keel, 'compile', '-O', '--opt-tier', 'none', '--emit-object', out/('whole-'+tier+'-after.keel'), '-o', out/('whole-'+tier+'.o')])
    exe = out/('whole-'+tier+'-check')
    run(['clang', out/'driver.o', out/('whole-'+tier+'.o'), '-o', exe])
    actual = run([exe, 'check'])
    assert actual == expected, tier
    (out/('whole-'+tier+'-check.txt')).write_text(actual)
if a.if_convert:
    diamond = fixture.parent/'if_convert'
    run(['clang', '-O2', '-fno-lto', '-c', diamond/'oracle.c', '-o', out/'diamond-oracle.o'])
    for name, passes in [('before', ''), ('converted', 'if-convert,simplify-cfg,dce')]:
        opt(diamond/'diamonds.keel', out/('diamonds-'+name+'.keel'), passes)
    for tier in ['fast', 'full']:
        run([keel, 'opt', '--opt-tier', tier, out/'diamonds-converted.keel', '-o', out/('diamonds-'+tier+'.keel')])
    for name in ['before', 'converted', 'fast', 'full']:
        for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
            src = out/('diamonds-'+name+'.keel')
            for ext, flag in [('s', []), ('o', ['--emit-object'])]:
                run([keel, 'compile', '-O', '--opt-tier', 'none', '--target', target, *flag,
                     src, '-o', out/('diamonds-'+name+'-'+target+'.'+ext)])
        exe = out/('diamonds-'+name+'.exe')
        run(['clang', out/'diamond-oracle.o', out/('diamonds-'+name+'-'+native_target+'.o'), '-o', exe])
        run([exe])
summary = dict(status='correctness/static only; no timing', native=native_target,
    private_before=counts(source.read_text()), private_promoted=counts(private),
    mixed_before=counts(mixed), mixed_promoted=counts((out/'mixed-promoted.keel').read_text()),
    mixed_full=counts((out/'mixed-full.keel').read_text()),
    private_combined=counts((out/'combined.keel').read_text()), corpus_before=counts(before), corpus_after=counts(after),
    whole_driver_lines=len(expected.splitlines()), commands=len(commands),
    artifacts={x.name:hashlib.sha256(x.read_bytes()).hexdigest() for x in out.iterdir() if x.is_file()})
(out/'result.json').write_text(json.dumps(summary, indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k != 'artifacts'}, indent=2))
