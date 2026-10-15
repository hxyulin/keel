"""Diagnostic exact unsigned /10 and %10 expansion in SQLite's hot formatter.

Changes only a retained optimized IR copy, using existing umulhi and shifts.
This measures a proposed scalar combine; it does not change the compiler.
"""
import argparse
import hashlib
import json
import pathlib
import random
import re
import subprocess
import time

from measure_passes import summary

ROOT = pathlib.Path(__file__).resolve().parent.parent
MAGIC = -3689348814741910323  # unsigned 0xcccccccccccccccd


def run(command, **kwargs):
    result = subprocess.run(list(map(str, command)), capture_output=True, timeout=180, **kwargs)
    if result.returncode:
        raise RuntimeError(f'{command}: {result.returncode}\n{result.stderr.decode(errors="replace")}')
    return result.stdout


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compiler', type=pathlib.Path, required=True)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--work-dir', type=pathlib.Path, required=True)
    p.add_argument('--pairs', type=int, default=11)
    args = p.parse_args()
    if args.pairs < 3:
        p.error('use at least three paired samples')
    work = args.work_dir.resolve(); work.mkdir(parents=True, exist_ok=True)
    app = json.loads(args.profile.read_text())['apps']['sqlite']
    unit = next(u for u in app['units'] if pathlib.Path(u['source']).stem == 'sqlite3')
    original = pathlib.Path(unit['ir']).read_text()
    function = re.search(r'^func @sqlite3FpDecode\([^\n]*\n.*?^}', original, re.M | re.S)[0]
    tens = set(re.findall(r'(v\d+) = iconst.i64 10\b', function))
    next_id = max(map(int, re.findall(r'\bv(\d+)\b', function))) + 1
    def value():
        nonlocal next_id
        next_id += 1
        return f'v{next_id}'
    changed = []
    def replace(match):
        result, op, numerator, divisor = match.groups()
        if divisor not in tens:
            return match[0]
        magic, high, shift, quotient = [value() for _ in range(4)]
        code = f'    {magic} = iconst.i64 {MAGIC}\n    {high} = umulhi {numerator}, {magic}\n    {shift} = iconst.i64 3\n'
        if op == 'udiv':
            code += f'    {result} = ushr {high}, {shift}'
        else:
            product = value()
            code += f'    {quotient} = ushr {high}, {shift}\n    {product} = imul {quotient}, {divisor}\n    {result} = isub {numerator}, {product}'
        changed.append({'result': result, 'operation': op, 'numerator': numerator})
        return code
    transformed = re.sub(r'^    (v\d+) = (udiv|urem) (v\d+), (v\d+)$', replace, function, flags=re.M)
    assert len(changed) == 2, changed
    ir = work / 'sqlite3-divide.keel'; ir.write_text(original.replace(function, transformed))
    asm, obj, candidate = [work / s for s in ['sqlite3-divide.s', 'sqlite3-divide.o', 'sqlite3-divide']]
    run([args.compiler, 'compile', '--opt-tier', 'none', ir, '-o', asm])
    assembly = asm.read_text()
    for name, alias in unit.get('darwin_import_aliases', {}).items():
        assembly = assembly.replace(alias, name)
    asm.write_text(assembly)
    run(['clang', '-c', asm, '-o', obj])
    objects = [obj if pathlib.Path(u['source']).stem == 'sqlite3' else pathlib.Path(u['normal_object']) for u in app['units']]
    run(['clang', *objects, '-o', candidate, '-lm', '-lpthread'])
    baseline = pathlib.Path(app['binaries']['normal'])
    # Native instruction validation against a variable-divisor C reference.
    check_ir = work / 'check.keel'
    check_ir.write_text(f'''func @quot(i64) -> i64 {{
block0(v0: i64):
 v1 = iconst.i64 {MAGIC}
 v2 = umulhi v0, v1
 v3 = iconst.i64 3
 v4 = ushr v2, v3
 return v4
}}
func @rem(i64) -> i64 {{
block0(v0: i64):
 v1 = iconst.i64 {MAGIC}
 v2 = umulhi v0, v1
 v3 = iconst.i64 3
 v4 = ushr v2, v3
 v5 = iconst.i64 10
 v6 = imul v4, v5
 v7 = isub v0, v6
 return v7
}}''')
    harness = work / 'check.c'
    harness.write_text('''#include <stdint.h>
extern uint64_t quot(uint64_t), rem(uint64_t);
volatile uint64_t denominator=10;
int main(void){uint64_t x=1;
for(unsigned i=0;i<100000;i++){x=x*6364136223846793005ull+1442695040888963407ull;
if(quot(x)!=x/denominator||rem(x)!=x%denominator)return 1;}
uint64_t edges[]={0,1,9,10,11,19,20,UINT64_MAX,UINT64_MAX-1,1ull<<63,(1ull<<63)-1};
for(unsigned i=0;i<sizeof(edges)/sizeof(edges[0]);i++)if(quot(edges[i])!=edges[i]/denominator||rem(edges[i])!=edges[i]%denominator)return 2;
return 0;}''')
    run([args.compiler, 'compile', '--opt-tier', 'none', check_ir, '-o', work/'check.s'])
    run(['clang', '-O2', harness, work/'check.s', '-o', work/'check'])
    run([work/'check'])
    for fixture in [ROOT/'suite/sqlite/queries.sql', ROOT/'suite/sqlite/bench.sql']:
        answers = [run([b, ':memory:'], input=fixture.read_bytes()) for b in [baseline, candidate]]
        assert answers[0] == answers[1], fixture
    edge_sql = b"select printf('%.17g',x),cast(x as text) from (select 1e-300 x union all select 1e308 union all select -1e-200 union all select 0.0 union all select 9007199254740991.0);"
    assert run([baseline, ':memory:'], input=edge_sql) == run([candidate, ':memory:'], input=edge_sql)
    samples = {'before': [], 'after': []}
    sql = (ROOT/'suite/sqlite/bench.sql').read_bytes()
    for iteration in range(args.pairs+2):
        order = [('before', baseline), ('after', candidate)]
        if iteration % 2:
            order.reverse()
        answers = []
        for name, binary in order:
            start = time.perf_counter(); answers.append(run([binary, ':memory:'], input=sql))
            elapsed = (time.perf_counter()-start)*1000
            if iteration>=2:
                samples[name].append(elapsed)
        assert answers[0] == answers[1]
    sha = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
    report = {'method': 'Only two i64 udiv/urem by constant 10 in sqlite3FpDecode are replaced in optimized IR with existing umulhi/shift/exact remainder. Same compiler, other functions/objects unchanged. Native quotient/remainder validated on 100000 pseudorandom uint64 inputs and 11 boundaries against volatile-divisor C. SQLite queries, float-format edge values and every timed stdout match baseline. Two warmups and alternating paired runs.',
              'changed_instructions': changed, 'compiler_sha256': sha(args.compiler),
              'original_ir_sha256': sha(unit['ir']), 'candidate_ir_sha256': sha(ir),
              'script_sha256': sha(__file__), 'binary_sha256': {'baseline': sha(baseline), 'candidate': sha(candidate)},
              **summary(samples, random.Random(42))}
    (work/'probe.json').write_text(json.dumps(report, indent=2)+'\n')
    print(report['after_over_before'], report['ratio_95_interval'], flush=True)


if __name__ == '__main__':
    main()
