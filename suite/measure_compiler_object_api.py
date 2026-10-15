#!/usr/bin/env python3
"""Compare current source round trip and structured object API on preprocessed C.

Build release keel and keel-asm's profile_pipeline example first. All commands
run serially; no Clang compilation is timed. Keeps sources, objects and samples.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time

from measure_assembler import run


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=Path('target/release/keel'))
    parser.add_argument('--profiler', type=Path, default=Path('target/release/examples/profile_pipeline'))
    parser.add_argument('--baseline-compiler', type=Path, help='optional saved compiler before the API change')
    parser.add_argument('--input-dir', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--library-pairs', type=int, default=11)
    parser.add_argument('--cli-pairs', type=int, default=7)
    parser.add_argument('--lua-tests', type=Path)
    args = parser.parse_args()
    if args.library_pairs < 3 or args.cli_pairs < 3:
        parser.error('use at least three pairs')
    inputs = sorted(args.input_dir.resolve().glob('*.i'))
    if not inputs:
        parser.error('--input-dir contains no .i inputs')
    compiler, profiler, work = args.compiler.resolve(), args.profiler.resolve(), args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    for mode in ('text', 'program', 'before-text'):
        (work / mode).mkdir(exist_ok=True)
    report = {'commit': run(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'host': platform.platform(), 'cpu': run(['sysctl', '-n', 'machdep.cpu.brand_string']).decode().strip(),
              'compiler_sha256': sha(compiler), 'profiler_sha256': sha(profiler), 'script_sha256': sha(Path(__file__)),
              'library_pairs': args.library_pairs, 'cli_pairs': args.cli_pairs, 'inputs': [],
              'method': 'Same current build, no optimization. Library: backend plus assembly/object emission and buffer destruction, balanced alternating order, two warmups; source reading, C frontend and IR validation excluded. CLI: complete preprocessed-C-to-object workflow, randomized paired warm-cache serial rounds, one warmup; source path invokes cc then asm, direct path cc --emit-object. Includes input/output and startup. No C preprocessing, linking or Clang compilation timed. Every whole object verified byte-identical. Native coverage is structured instructions / all source instructions; remaining text bytes include directives and unmigrated instructions.'}
    commands = {'text': [], 'program': []}
    if args.baseline_compiler:
        commands['before-text'] = []
        report['baseline_compiler_sha256'] = sha(args.baseline_compiler.resolve())
    for path in inputs:
        profile_command = [profiler, path, args.library_pairs]
        row = {'input': str(path), 'input_sha256': sha(path), 'profile_command': list(map(str, profile_command)),
               'library': json.loads(run(profile_command))}
        assembly, expected, actual = work / 'text' / (path.stem + '.s'), work / 'text' / (path.stem + '.o'), work / 'program' / (path.stem + '.o')
        text_commands = [[str(compiler), 'cc', str(path), '-o', str(assembly)],
                         [str(compiler), 'asm', str(assembly), '-o', str(expected)]]
        program_commands = [[str(compiler), 'cc', str(path), '--emit-object', '-o', str(actual)]]
        for command in text_commands + program_commands:
            run(command)
        assert expected.read_bytes() == actual.read_bytes(), path
        if args.baseline_compiler:
            before = args.baseline_compiler.resolve()
            before_asm, before_object = work / 'before-text' / (path.stem + '.s'), work / 'before-text' / (path.stem + '.o')
            before_commands = [[str(before), 'cc', str(path), '-o', str(before_asm)],
                               [str(before), 'asm', str(before_asm), '-o', str(before_object)]]
            for command in before_commands:
                run(command)
            assert before_object.read_bytes() == actual.read_bytes(), path
            row['before_commands'] = before_commands
            commands['before-text'].extend(before_commands)
        row.update(text_commands=text_commands, program_commands=program_commands, object_sha256=sha(actual))
        report['inputs'].append(row)
        commands['text'].extend(text_commands)
        commands['program'].extend(program_commands)
        print(path.name, 'native instructions:', row['library']['native_instructions'], '/', row['library']['total_instructions'], flush=True)
    samples = {mode: [] for mode in commands}
    rng = random.Random(20261015)
    for i in range(args.cli_pairs + 1):
        order = list(commands)
        rng.shuffle(order)
        for mode in order:
            start = time.perf_counter_ns()
            for command in commands[mode]:
                run(command)
            if i:
                samples[mode].append((time.perf_counter_ns() - start) / 1e6)

    def summary(samples):
        ratios = [a / b for a, b in zip(samples['text'], samples['program'])]
        boot = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(10000))
        return {'samples_ms': samples, 'median_ms': {k: statistics.median(v) for k, v in samples.items()},
                'text_over_program': statistics.median(ratios), 'speedup_bootstrap_95': [boot[249], boot[9749]]}

    report['cli'] = summary(samples)
    if args.baseline_compiler:
        report['cli']['before_text_over_program'] = statistics.median([a / b for a, b in zip(samples['before-text'], samples['program'])])
        report['cli']['text_after_over_before'] = statistics.median([a / b for a, b in zip(samples['text'], samples['before-text'])])
    report['library'] = summary({mode: [sum(x['library'][mode + '_ms'][i] for x in report['inputs'])
                                       for i in range(args.library_pairs)] for mode in ('text', 'program')})
    report['coverage'] = {key: sum(x['library'][key] for x in report['inputs'])
                          for key in ('native_instructions', 'total_instructions', 'text_bytes', 'remaining_text_bytes')}
    if args.lua_tests:
        exe = work / 'lua-program'
        link = ['clang', '-target', 'arm64-apple-macosx11.0.0', *map(str, sorted((work / 'program').glob('*.o'))), '-lm', '-o', str(exe)]
        run(link)
        command = [str(exe), '-e', '_U=true', 'all.lua']
        result = subprocess.run(command, cwd=args.lua_tests.resolve(), capture_output=True, timeout=180)
        log = work / 'lua-program-tests.log'
        log.write_bytes(result.stdout + result.stderr)
        report['runtime'] = {'link_command': link, 'test_command': command, 'test_cwd': str(args.lua_tests.resolve()),
                             'returncode': result.returncode, 'log': str(log), 'log_sha256': sha(log)}
        assert result.returncode == 0, result.stderr.decode(errors='replace')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ('library', 'cli', 'coverage')}, indent=2))


if __name__ == '__main__':
    main()
