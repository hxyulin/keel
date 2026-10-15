"""Compare an archived read-based driver with the default mapped-input driver.

Measures warm-cache command-line assembly, including parser and object output.
Keep inputs immutable during the experiment. Builds and tests run separately.
Use drivers whose compiler/assembler implementations match apart from input
loading, so parser changes do not get attributed to the input strategy.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import random
import time

from measure_assembler_candidates import run, summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=pathlib.Path, required=True)
    parser.add_argument('--read-compiler', type=pathlib.Path, required=True,
                        help='archived release driver using ordinary reads (e.g. cc01b96)')
    parser.add_argument('--input-dir', type=pathlib.Path, required=True)
    parser.add_argument('--input-group', default='lua', help='label for files from --input-dir')
    parser.add_argument('--extra-input', type=pathlib.Path, action='append', default=[])
    parser.add_argument('--work-dir', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--rounds', type=int, default=21)
    parser.add_argument('--warmups', type=int, default=2)
    args = parser.parse_args()
    if args.rounds < 5 or args.warmups < 1:
        parser.error('use at least 5 rounds and 1 warmup')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    compiler = args.compiler.resolve()
    read_compiler = args.read_compiler.resolve()
    if hashlib.sha256(compiler.read_bytes()).digest() == hashlib.sha256(read_compiler.read_bytes()).digest():
        parser.error('read and mapped drivers must be different binaries')
    sources = [(p, args.input_group) for p in sorted(args.input_dir.resolve().glob('*.s'))]
    if not sources:
        parser.error('no assembly inputs')
    sources += [(p.resolve(), 'large') for p in args.extra_input]
    report = {'compiler': str(compiler), 'host': platform.platform(),
              'process_load_at_start': run(['ps', '-axo', 'pid,pcpu,comm']).decode(),
              'compiler_sha256': hashlib.sha256(compiler.read_bytes()).hexdigest(),
              'read_compiler': str(read_compiler),
              'read_compiler_sha256': hashlib.sha256(read_compiler.read_bytes()).hexdigest(),
              'script_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
              'rounds': args.rounds, 'warmups': args.warmups, 'rows': [],
              'method': 'Warm filesystem cache. Randomized paired read/mmap order per input per round. Complete CLI time, including parsing, encoding and output. Lua total sums serial per-file times, not parallel build wall time. No cache eviction or cold-storage claim.'}
    rng = random.Random(42)
    for index, (path, group) in enumerate(sources):
        outputs = {name: work / f'{index}-{name}.o' for name in ['baseline', 'mmap']}
        commands = {name: [compiler if name == 'mmap' else read_compiler,
                           'asm', path, '-o', output] for name, output in outputs.items()}
        for _ in range(args.warmups):
            for command in commands.values():
                run(command)
        assert outputs['baseline'].read_bytes() == outputs['mmap'].read_bytes(), path
        samples = {name: [] for name in commands}
        for _ in range(args.rounds):
            order = list(commands)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                run(commands[name])
                samples[name].append((time.perf_counter_ns() - start) / 1e6)
        report['rows'].append({'name': path.stem, 'group': group,
                               'source_bytes': path.stat().st_size,
                               'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                               'commands': {k: list(map(str, v)) for k, v in commands.items()},
                               'objects_identical': True, 'samples_ms': samples,
                               'summary': summary(samples)})
        report['groups'] = {}
        for label in {item['group'] for item in report['rows']}:
            rows = [item for item in report['rows'] if item['group'] == label]
            totals = {name: [sum(item['samples_ms'][name][i] for item in rows)
                             for i in range(args.rounds)] for name in commands}
            report['groups'][label] = {'files': len(rows), 'samples_ms': totals,
                                       'summary': summary(totals)}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(path.stem, report['rows'][-1]['summary']['mmap'], flush=True)


if __name__ == '__main__':
    main()
