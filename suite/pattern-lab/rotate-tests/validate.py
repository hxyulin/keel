"""Explicit correctness-only rotate activation check; requires a leased host/compiler."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--compiler', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
a.output.mkdir(exist_ok=False)
out = a.output.resolve()
compiler = a.compiler.resolve()
commands = []
def run(label, argv):
    argv = list(map(str, argv))
    r = subprocess.run(argv, capture_output=True, timeout=60)
    (out / (label + '.stdout')).write_bytes(r.stdout)
    (out / (label + '.stderr')).write_bytes(r.stderr)
    commands.append(dict(label=label, argv=argv, returncode=r.returncode))
    (out / 'commands.json').write_text(json.dumps(commands, indent=2))
    assert r.returncode == 0, (label, r.stderr.decode())
    return r.stdout.decode()

def function(ir, name):
    return ir.split('func @' + name + '(', 1)[1].split('\n}', 1)[0]

def assert_rotate(body, count):
    # Check actual dataflow: rotate takes the first parameter and a literal count.
    params = re.search(r'block0\((v\d+):', body)
    constants = dict(re.findall(r'(v\d+) = iconst\.i(?:32|64) (-?\d+)', body))
    rotations = re.findall(r'= rotl (v\d+), (v\d+)', body)
    assert any(x == params[1] and constants.get(c) == str(count) for x, c in rotations), body

bridge = Path('suite/pattern-lab/compiler-cases/recurrences.c').read_text()
# Keep the original bridge function byte-for-byte; no adapted loop.
bridge = 'typedef unsigned long long uint64_t;\n' + 'uint64_t bridge_rotate(' + bridge.split('uint64_t bridge_rotate(', 1)[1].split('\nuint64_t bridge_udiv10', 1)[0]
source = bridge + '\n'
names = []
for width in (32, 64):
    ty = 'unsigned int' if width == 32 else 'unsigned long long'
    for count in range(1, width):
        name = f'rotate_{width}_{count}'
        names.append((name, ty, width, count))
        expr = f'(x << {count}) | (x >> {width-count})'
        if count % 2 == 0:
            expr = f'(x >> {width-count}) | (x << {count})'
        source += f'{ty} {name}({ty} x) {{ return {expr}; }}\n'
source += '''unsigned long long negative_dynamic(unsigned long long x, unsigned long long n) { return (x << n) | (x >> (64-n)); }
unsigned long long negative_different(unsigned long long x, unsigned long long y) { return (x << 51) | (y >> 13); }
long long negative_arithmetic(long long x) { return (x << 51) | (x >> 13); }
'''
(out / 'rotate.i').write_text(source)
# Independent compiler, separate translation unit, no LTO. Its C expressions
# define the expected values, including narrow unsigned modular arithmetic.
oracle = source
for name in ['bridge_rotate', 'negative_dynamic', 'negative_different', 'negative_arithmetic'] + [n[0] for n in names]:
    oracle = re.sub(r'\b' + name + r'\b', 'oracle_' + name, oracle)
(out / 'oracle.c').write_text(oracle)
clang = run('resolve-clang', ['/usr/bin/xcrun', '--find', 'clang']).strip()
run('clang-version', [clang, '--version'])
run('rustc-version', ['rustc', '-vV'])
run('cargo-version', ['cargo', '-V'])
run('source-revision', ['git', 'rev-parse', 'HEAD', 'HEAD^{tree}'])
run('source-status', ['git', 'status', '--short'])
modes = {'full': ['-O'], 'baseline-selection': ['-O', '--isel', 'none'], 'debug': ['-g', '-O']}
# Complete all activation/fallback assertions BEFORE any native compilation.
for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
    for mode, flags in modes.items():
        tag = target + '-' + mode
        base = [compiler, 'cc', *flags, '--verify-each', '--target', target, out / 'rotate.i']
        ir = run(tag + '-ir', [*base, '--emit-ir'])
        (out / (tag + '.ir')).write_text(ir)
        if mode != 'debug':
            b = function(ir, 'bridge_rotate')
            constants = dict(re.findall(r'(v\d+) = iconst.i64 (-?\d+)', b))
            assert any(constants.get(c) == '51' for _, c in re.findall(r'= rotl (v\d+), (v\d+)', b)), b
            for name, _, _, count in names:
                assert_rotate(function(ir, name), count)
        for negative in ['negative_dynamic', 'negative_different', 'negative_arithmetic']:
            assert not re.search(r'= rot[lr] ', function(ir, negative)), negative
        run(tag + '-asm', [*base, '-o', out / (tag + '.s')])
        if mode != 'debug':
            asm = (out / (tag + '.s')).read_text()
            # Restrict to bridge's function body, excluding IR comments.
            body = asm.split('_bridge_rotate:' if target.startswith('aarch64') else '"bridge_rotate":', 1)[1]
            body = body.split('Lbridge_rotate_end:', 1)[0] if target.startswith('aarch64') else body.split('.size', 1)[0]
            lines = [l for l in body.splitlines() if not l.lstrip().startswith((';', '#'))]
            pat = r'\bror\s+(x\d+),\s*(x\d+),\s*#13\b' if target.startswith('aarch64') else r'\brol\s+\w+,\s*51\b'
            assert re.search(pat, '\n'.join(lines)), body
        run(tag + '-object', [*base, '--emit-object', '-o', out / (tag + '.o')])
(out / 'activation-passed.json').write_text(json.dumps({'all_counts':94,'negative_functions':3,'targets':2,'modes':list(modes),'debug_activation_required':False},indent=2))
# Assemble emitted text too, independently of the compiler object encoder.
for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
    for mode in modes:
        tag = target + '-' + mode
        run(tag + '-assemble', [clang, '-target', target, '-c', out / (tag + '.s'), '-o', out / (tag + '-assembled.o')])
run('oracle-compile', [clang, '-O0', '-fno-lto', '-c', out / 'oracle.c', '-o', out / 'oracle.o'])
driver = '#include <stdio.h>\n#include <stdint.h>\n'
for name, ty, _, _ in names:
    driver += f'extern {ty} {name}({ty}); extern {ty} oracle_{name}({ty});\n'
driver += '''extern unsigned long long bridge_rotate(unsigned long long,unsigned long long,const void*,unsigned long long);
extern unsigned long long oracle_bridge_rotate(unsigned long long,unsigned long long,const void*,unsigned long long);
extern unsigned long long negative_dynamic(unsigned long long,unsigned long long),oracle_negative_dynamic(unsigned long long,unsigned long long);
extern unsigned long long negative_different(unsigned long long,unsigned long long),oracle_negative_different(unsigned long long,unsigned long long);
int main(void) { unsigned failures=0, checks=0;
uint64_t xs[]={0,1,2,0x80000000ULL,0xffffffffULL,0x8000000000000000ULL,~0ULL,0x0123456789abcdefULL};
for(unsigned i=0;i<8;i++) { uint64_t x=xs[i];
'''
for name, ty, _, _ in names:
    driver += f'checks++; if({name}(({ty})x)!=oracle_{name}(({ty})x)) {{ failures++; printf("FAIL {name} %llu\\n",(unsigned long long)x); }}\n'
driver += '''for(unsigned n=1;n<64;n++){checks++; if(negative_dynamic(x,n)!=oracle_negative_dynamic(x,n))failures++;}
for(unsigned j=0;j<8;j++){checks++; if(negative_different(x,xs[j])!=oracle_negative_different(x,xs[j]))failures++;}
unsigned ns[]={0,1,2,17,257};
for(unsigned n=0;n<5;n++){checks++; if(bridge_rotate(ns[n],x,0,~0ULL)!=oracle_bridge_rotate(ns[n],x,0,~0ULL))failures++;}
} printf("checks=%u failures=%u\\n",checks,failures);return failures?1:0;}
'''
(out / 'driver.c').write_text(driver)
run('driver-compile', [clang, '-O0', '-fno-lto', '-c', out / 'driver.c', '-o', out / 'driver.o'])
for mode in modes:
    for route in ['', '-assembled']:
        tag = 'aarch64-apple-darwin-' + mode + route
        run(tag + '-link', [clang, '-fno-lto', out / 'driver.o', out / 'oracle.o', out / (tag + '.o'), '-o', out / (tag + '-native')])
        run(tag + '-native', [out / (tag + '-native')])
identities = {}
for path in [compiler, Path(clang).resolve(), *out.iterdir()]:
    if path.is_file(): identities[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
(out / 'sha256.json').write_text(json.dumps(identities, indent=2))
print('All compiler/native jobs exited; correctness only, no timing.')
