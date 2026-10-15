"""Diagnostic source expansion of three sampled QuickJS helpers, not an inliner.

Writes only into the requested work directory. Uses the untouched profile build
for other objects and compares baseline/candidate output, tests and paired timing.
"""
import argparse
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import time

from measure_passes import summary
import random

ROOT = pathlib.Path(__file__).resolve().parent.parent
TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|/\*.*?\*/|//[^\n]*|\b(JS_NewInt32|JS_NewBool|JS_DupValue)\s*\(', re.S)


def expand(source, counts, serial):
    output, end = [], 0
    for match in TOKEN.finditer(source):
        if match.start() < end or not match[1]:
            continue
        depth, position, split = 1, match.end(), None
        while depth:
            char = source[position]
            if char in "\"'":
                quote = char; position += 1
                while source[position] != quote:
                    position += 2 if source[position] == '\\' else 1
            elif char in '([{':
                depth += 1
            elif char in ')]}':
                depth -= 1
            elif char == ',' and depth == 1:
                assert split is None
                split = position
            position += 1
        # Leave definitions in place; only expression call sites are expanded.
        if source[position:].lstrip().startswith('{'):
            continue
        assert split is not None, source[match.start():position]
        left = expand(source[match.end():split], counts, serial)
        right = expand(source[split+1:position-1], counts, serial)
        serial[0] += 1
        suffix = serial[0]
        ctx, value, pointer = (f'__profile_{name}_{suffix}' for name in ['ctx', 'value', 'pointer'])
        name = match[1]
        if name == 'JS_DupValue':
            expression = f'''({{ JSContext *{ctx}=({left}); JSValue {value}=({right});
 if(((unsigned)((int32_t){value}.tag)>=(unsigned)JS_TAG_FIRST)){{
 JSRefCountHeader *{pointer}=(JSRefCountHeader *){value}.u.ptr;{pointer}->ref_count++;
 }} {value}; }})'''
        else:
            tag = 'JS_TAG_INT' if name == 'JS_NewInt32' else 'JS_TAG_BOOL'
            stored = value if name == 'JS_NewInt32' else f'({value}!=0)'
            expression = f'''({{ JSContext *{ctx}=({left}); int32_t {value}=({right});
 (JSValue){{ (JSValueUnion){{ .int32={stored} }}, {tag} }}; }})'''
        output.extend([source[end:match.start()], expression])
        end = position
        counts[name] = counts.get(name, 0) + 1
    return ''.join(output) + source[end:]


def run(command, **kwargs):
    result = subprocess.run(list(map(str, command)), capture_output=True, timeout=180, **kwargs)
    if result.returncode:
        raise RuntimeError(f'{command}: {result.returncode}\n{result.stderr.decode(errors="replace")}')
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=pathlib.Path, required=True)
    parser.add_argument('--profile', type=pathlib.Path, required=True)
    parser.add_argument('--prepared', type=pathlib.Path, default=ROOT / 'target/suite')
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--pairs', type=int, default=11)
    args = parser.parse_args()
    if args.pairs < 3:
        parser.error('use at least three paired samples')
    work = args.work_dir.resolve(); work.mkdir(parents=True, exist_ok=True)
    profile = json.loads(args.profile.read_text())['apps']['quickjs']
    source_path = args.prepared / 'quickjs/pre/quickjs.i'
    source = source_path.read_text(); counts = {}
    expanded = expand(source, counts, [0])
    transformed = work / 'quickjs-expanded.i'; transformed.write_text(expanded)
    asm, obj, candidate = (work / n for n in ['quickjs-expanded.s', 'quickjs-expanded.o', 'qjs-expanded'])
    run([args.compiler, 'cc', '-O', transformed, '-o', asm])
    run(['clang', '-c', asm, '-o', obj])
    objects = [obj if pathlib.Path(u['source']).stem == 'quickjs' else pathlib.Path(u['normal_object']) for u in profile['units']]
    run(['clang', *objects, '-o', candidate, '-lm', '-lpthread'])
    baseline = pathlib.Path(profile['binaries']['normal'])
    tests = work / 'tests'
    if not tests.exists():
        shutil.copytree(args.prepared / 'quickjs/quickjs-2024-01-13/tests', tests)
    checks = []
    for name, flags in [('test_closure', []), ('test_language', []), ('test_builtin', []),
                        ('test_loop', []), ('test_std', []), ('test_worker', []),
                        ('test_op_overloading', ['--bignum']), ('test_bignum', ['--bignum']),
                        ('test_qjscalc', ['--qjscalc'])]:
        answers = [run([b, *flags, tests / f'{name}.js'], cwd=work) for b in [baseline, candidate]]
        assert answers[0] == answers[1], name
        checks.append(name)
    rows = []
    rng = random.Random(42)
    for bench in ['fib', 'sort', 'string']:
        samples = {'before': [], 'after': []}
        expected = None
        for iteration in range(args.pairs + 2):
            order = [('before', baseline), ('after', candidate)]
            if iteration % 2:
                order.reverse()
            for name, binary in order:
                started = time.perf_counter()
                answer = run([binary, '--stack-size', '4194304', ROOT / 'suite/quickjs/bench.js', bench])
                elapsed = (time.perf_counter() - started) * 1000
                if expected is None:
                    expected = answer
                assert expected == answer, bench
                if iteration >= 2:
                    samples[name].append(elapsed)
        rows.append({'bench': bench, **summary(samples, rng)})
        print(bench, rows[-1]['after_over_before'], flush=True)
    sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
    report = {'method': 'Diagnostic source expansion in quickjs.i only of JS_NewInt32, JS_NewBool and JS_DupValue; argument expressions evaluated once in typed temporaries; original reference-count branch preserved. Same Keel -O binary and unchanged other objects. Two warmups then alternating paired CLI runs. This is evidence for an inliner investigation, not a compiler inliner implementation or general forecast.',
              'expanded_call_sites': counts, 'tests_passed_both_variants': checks,
              'source_sha256': sha(source_path), 'expanded_source_sha256': sha(transformed),
              'compiler_sha256': sha(args.compiler), 'script_sha256': sha(__file__),
              'binary_sha256': {'baseline': sha(baseline), 'candidate': sha(candidate)},
              'samples': rows}
    (work / 'probe.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
