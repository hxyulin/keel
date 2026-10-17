#!/usr/bin/env python3
"""Explicit, lease-only sign-extend-idiom correctness capture; no builds/timing.

Supply a compiler, same-source Clang, and NEW output directory. All commands
are serial with captured output. IR assertions run for every route before any
codegen/assembler/linker/native command. Hashing belongs to post-release audit.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--compiler', required=True, type=Path)
p.add_argument('--clang', required=True, type=Path)
p.add_argument('--output', required=True, type=Path)
a = p.parse_args()
out = a.output.resolve()
out.mkdir(exist_ok=False)
compiler, clang = a.compiler.resolve(), a.clang.resolve()
records = []

def run(tag, argv, timeout=30):
    argv = [str(x) for x in argv]
    rec = {'tag': tag, 'argv': argv, 'started_unix_ns': time.time_ns()}
    records.append(rec)
    try:
        result = subprocess.run(argv, capture_output=True, timeout=timeout)
        (out / (tag + '.stdout')).write_bytes(result.stdout)
        (out / (tag + '.stderr')).write_bytes(result.stderr)
        rec['returncode'] = result.returncode
        rec['finished_unix_ns'] = time.time_ns()
        (out / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        if result.returncode:
            raise RuntimeError(f'{tag}: exit {result.returncode}: {result.stderr.decode(errors="replace")}')
        return result.stdout.decode()
    except Exception as e:
        rec['exception'] = repr(e)
        (out / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        raise

def functions(text):
    return dict(re.findall(r'func @(\w+)\b([^}]*\n})', text))

# Use the actual bridge source body verbatim, rather than a reconstructed loop.
repo = Path(__file__).resolve().parents[3]
bridge = (repo / 'suite/pattern-lab/compiler-cases/recurrences.c').read_text()
body = re.search(r'uint64_t bridge_sext8\(.*?\n}', bridge, re.S).group()
source = 'typedef unsigned long long uint64_t;\n' + body + '\n' + '''
uint64_t sext_byte(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte < 128ULL ? byte : byte - 256ULL;
}
uint64_t inverted(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte >= 128ULL ? byte - 256ULL : byte;
}
unsigned int negative_width(unsigned int state) {
    unsigned int byte = state & 255U;
    return byte < 128U ? byte : byte - 256U;
}
uint64_t negative_bound(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte < 127ULL ? byte : byte - 256ULL;
}
uint64_t negative_mask(uint64_t state) {
    uint64_t byte = state & 511ULL;
    return byte < 128ULL ? byte : byte - 256ULL;
}
uint64_t negative_sub(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte < 128ULL ? byte : byte - 255ULL;
}
uint64_t negative_shared(uint64_t state) {
    uint64_t byte = state & 255ULL;
    uint64_t ext = byte < 128ULL ? byte : byte - 256ULL;
    return ext + byte;
}
volatile uint64_t cell;
uint64_t effects;
void effect(void) __attribute__((noinline));
void effect(void) { effects++; }
uint64_t negative_volatile(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte < 128ULL ? byte : byte - (cell, 256ULL);
}
uint64_t negative_call(uint64_t state) {
    uint64_t byte = state & 255ULL;
    return byte < 128ULL ? byte : byte - (effect(), 256ULL);
}
uint64_t negative_multiple(uint64_t state, uint64_t flag) {
    uint64_t byte = state & 255ULL;
    if (flag) return byte - 256ULL;
    return byte < 128ULL ? byte : byte - 256ULL;
}
'''
(out / 'sext.i').write_text(source)
names = ['bridge_sext8', 'sext_byte', 'inverted', 'negative_width',
         'negative_bound', 'negative_mask', 'negative_sub', 'negative_shared',
         'negative_volatile', 'negative_call', 'negative_multiple', 'effect', 'cell', 'effects']
oracle = source
for name in names:
    oracle = re.sub(r'\b' + name + r'\b', 'oracle_' + name, oracle)
(out / 'oracle.c').write_text(oracle)
run('compiler-passes', [compiler, 'passes'])
run('clang-version', [clang, '--version'])
assert 'sign-extend-idiom' in (out / 'compiler-passes.stdout').read_text()

# First phase: all fresh C input -> full optimizer -> explicitly named pass,
# then verified roundtrip. Every negative assertion precedes machine codegen.
routes = []
checks = {}
for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
    for mode in ['full', 'baseline-selection', 'debug']:
        tag = target + '-' + mode
        debug = ['-g'] if mode == 'debug' else []
        before = out / (tag + '-before.keel')
        normalized = out / (tag + '-normalized.keel')
        after = out / (tag + '-after.keel')
        clean = out / (tag + '-clean.keel')
        run(tag + '-C-opt', [compiler, 'cc', '-O', *debug, '--verify-each', '--target', target,
                             '--emit-ir', out / 'sext.i', '-o', before])
        run(tag + '-normalize', [compiler, 'opt', '--opt-tier', 'none', '--verify-each', before, '-o', normalized])
        run(tag + '-explicit', [compiler, 'opt', '-p', 'sign-extend-idiom', '--verify-each', normalized, '-o', after])
        run(tag + '-roundtrip', [compiler, 'opt', '--opt-tier', 'none', '--verify-each', after, '-o', out / (tag + '-roundtrip.keel')])
        before_f, after_f = functions(normalized.read_text()), functions(after.read_text())
        assert before_f.keys() == after_f.keys()
        positive = ['bridge_sext8', 'sext_byte', 'inverted']
        for name, f in before_f.items():
            if name in positive and mode != 'debug':
                assert 'trunc.i8' not in f and 'sext.i64' not in f, (tag, name, 'default unexpectedly activated')
                new = after_f[name]
                pair = re.search(r'(v\d+) = trunc\.i8 (v\d+)\n\s*(v\d+) = sext\.i64 \1\n\s*jump (block\d+)\(\3\)', new)
                assert pair, (tag, name, 'missing exact trunc/sext/jump dataflow', new)
                assert new.count('brif') + 1 == f.count('brif'), (tag, name, 'byte branch retained')
            else:
                # The debug frontend keeps locals in memory; the strict matcher
                # must preserve that shape and all its source stepping points.
                assert after_f[name] == f, (tag, name, 'unexpected fallback mutation')
        if mode == 'debug':
            assert 'load.i64' in before_f['bridge_sext8']
        run(tag + '-cleanup', [compiler, 'opt', '-p', 'dce,simplify-cfg', '--verify-each', after, '-o', clean])
        checks[tag] = {'fresh_C_positives': positive if mode != 'debug' else [],
                       'fallbacks': list(before_f.keys()) if mode == 'debug' else [n for n in before_f if n not in positive],
                       'verifier': 'enabled, input plus after each pass; explicit roundtrip',
                       'all_assertions_passed': True}
        flags = ['-O', '--opt-tier', 'none']
        if mode == 'baseline-selection':
            flags += ['--isel-tier', 'baseline']
        routes.append((tag, target, clean, flags))
(out / 'IR-activation.json').write_text(json.dumps(checks, indent=2) + '\n')

# This phase is unreachable unless all positive, fallback and verifier
# assertions for both targets/modes completed successfully.
for tag, target, ir, flags in routes:
    run(tag + '-text', [compiler, 'compile', *flags, '--verify-each', '--target', target, ir, '-o', out / (tag + '.s')])
    run(tag + '-direct-object', [compiler, 'compile', *flags, '--verify-each', '--target', target,
                                 '--emit-object', ir, '-o', out / (tag + '-direct.o')])
    run(tag + '-assemble', [clang, '-target', target, '-c', out / (tag + '.s'), '-o', out / (tag + '-assembled.o')])

# Independent Clang -O0 oracle is a separate TU and object; no LTO, no Keel
# reference object masquerading as an oracle. Check both return values and
# global effects. All 256 bytes occur with zero and nonzero high input bits.
run('oracle-compile', [clang, '-O0', '-fno-lto', '-c', out / 'oracle.c', '-o', out / 'oracle.o'])
driver = '''#include <stdio.h>
#include <stdint.h>
extern volatile uint64_t cell, oracle_cell;
extern uint64_t effects, oracle_effects;
'''
for name in ['sext_byte', 'inverted', 'negative_bound', 'negative_mask', 'negative_sub', 'negative_shared', 'negative_volatile', 'negative_call']:
    driver += f'extern uint64_t {name}(uint64_t), oracle_{name}(uint64_t);\n'
driver += '''extern unsigned int negative_width(unsigned int), oracle_negative_width(unsigned int);
extern uint64_t negative_multiple(uint64_t,uint64_t), oracle_negative_multiple(uint64_t,uint64_t);
extern uint64_t bridge_sext8(uint64_t,uint64_t,const uint64_t*,uint64_t), oracle_bridge_sext8(uint64_t,uint64_t,const uint64_t*,uint64_t);
int main(void) {
 unsigned long checks=0, failures=0;
 const uint64_t highs[]={0,0x123456789abcde00ULL,0x8000000000000000ULL,~255ULL};
 const uint64_t ns[]={0,1,2,17,257};
 for(unsigned h=0;h<4;h++) for(unsigned b=0;b<256;b++) {
  uint64_t x=highs[h]|b, got, want;
'''
for name in ['sext_byte', 'inverted', 'negative_bound', 'negative_mask', 'negative_sub', 'negative_shared', 'negative_volatile', 'negative_call']:
    driver += f'''cell=oracle_cell=x; effects=oracle_effects=0;
 got={name}(x); want=oracle_{name}(x); checks++;
 if(got!=want || effects!=oracle_effects) {{ failures++; printf("FAIL {name} %llu %llu %llu\\n",(unsigned long long)x,(unsigned long long)got,(unsigned long long)want); }}
'''
    if name in ['sext_byte', 'inverted']:
        driver += '''checks++; if(got!=(uint64_t)(int64_t)(int8_t)(uint8_t)x) failures++;
'''
driver += '''checks++; if(negative_width((unsigned)x)!=oracle_negative_width((unsigned)x)) failures++;
 for(unsigned flag=0;flag<3;flag++){checks++; if(negative_multiple(x,flag)!=oracle_negative_multiple(x,flag)) failures++;}
 for(unsigned k=0;k<5;k++) {
  uint64_t n=ns[k];
  got=bridge_sext8(n,x,0,~0ULL); want=oracle_bridge_sext8(n,x,0,~0ULL);
  checks++; if(got!=want) { failures++; printf("FAIL bridge n=%llu x=%llu\\n",(unsigned long long)n,(unsigned long long)x); }
  uint64_t math=x;
  for(uint64_t j=0;j<n;j++) {
    uint64_t low=math&255;
    math=(low<128 ? low : low-256)+0x9e3779b97f4a7c15ULL;
  }
  checks++; if(got!=math) failures++;
 }
 }
 printf("checks=%lu failures=%lu\\n",checks,failures);
 return failures ? 1 : 0;
}
'''
(out / 'driver.c').write_text(driver)
run('driver-compile', [clang, '-O0', '-fno-lto', '-c', out / 'driver.c', '-o', out / 'driver.o'])
results = {}
for tag, target, _, _ in routes:
    if target != 'aarch64-apple-darwin':
        continue
    for route in ['direct', 'assembled']:
        rt = tag + '-' + route
        run(rt + '-link', [clang, '-fno-lto', out / 'driver.o', out / 'oracle.o', out / (rt + '.o'), '-o', out / (rt + '-native')])
        result = run(rt + '-run', [out / (rt + '-native')])
        assert 'checks=24576 failures=0' in result, result
        results[rt] = result.strip()
(out / 'native-results.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps({'IR_routes': len(checks), 'object_routes': len(routes)*2, 'native': results}, indent=2))
