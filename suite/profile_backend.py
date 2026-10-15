#!/usr/bin/env python3
"""Profile AArch64 text and structured backends on existing .i or .keel inputs.

Build: cargo build --release -p keel-asm --features backend-profile --example profile_backend
Frontend parsing, IR verification, and optional optimization are outside timing.
"""
import argparse
import hashlib
import json
import math
import platform
from pathlib import Path
import statistics
import subprocess


def summarize(rows):
    result = {}
    for mode in ('text', 'structured'):
        runs = [run for row in rows for run in row['runs'] if run['mode'] == mode]
        # A suite total is the sum of each input's median, not a pooled median
        # across differently sized translation units.
        metrics = {key: sum(statistics.median(run[key] for run in row['runs'] if run['mode'] == mode)
                            for row in rows)
                   for key in ('plain_codegen_ms', 'profiled_codegen_ms', 'assemble_ms', 'macho_ms')}
        stage_ms = {stage: sum(sum(run['stages'][stage]['self_ms'] for run in row['runs'] if run['mode'] == mode) / row['rounds']
                              for row in rows) for stage in runs[0]['stages']}
        total = sum(stage_ms.values())
        overhead_ratios = [run['profiled_codegen_ms'] / run['plain_codegen_ms'] for run in runs]
        result[mode] = {**metrics, 'stage_self_ms': stage_ms,
                        'stage_shares': {stage: value / total for stage, value in stage_ms.items()},
                        'profile_capture_overhead_median_ratio': statistics.median(overhead_ratios),
                        'profile_capture_overhead_ratio_p10_p90': [sorted(overhead_ratios)[int((len(overhead_ratios)-1)*q)] for q in (.1,.9)]}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profiler', type=Path, default=Path('target/release/examples/profile_backend'))
    parser.add_argument('--input-dir', type=Path, help='all .i files in one directory')
    parser.add_argument('--input', type=Path, action='append', default=[])
    parser.add_argument('--rounds', type=int, default=9)
    parser.add_argument('--opt', action='store_true')
    parser.add_argument('--debug', action='store_true')
    parser.add_argument('--selection-all', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    paths = sorted(set(args.input + (list(args.input_dir.glob('*.i')) if args.input_dir else [])))
    if not paths or args.rounds < 3:
        parser.error('provide inputs and at least three rounds')
    profiler = args.profiler.resolve()
    report = {'host': platform.platform(), 'cpu': (subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip() if platform.system() == 'Darwin' else platform.processor()),
              'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'profiler_sha256': hashlib.sha256(profiler.read_bytes()).hexdigest(),
              'method': 'AArch64 Apple Darwin; full allocator; text and structured output paths. Backend-only, warm-cache, serial. Read, frontend, verification and optional IR optimization excluded. Two warmups; balanced alternating text/structured and plain/capture order. Outer times include output destruction; instrumented module root excludes returned output destruction. Exclusive stage times subtract child scopes and sum to root time; stage shares use means, wall times use summed per-file medians. Mach-O writer uses a prebuilt structured assembly for both modes. Capture overhead compares active collection against inactive collection in the same feature-enabled binary; default builds compile out instrumentation. Objects from both output paths and output with/without active capture must match.',
              'inputs': []}
    for path in paths:
        command = [str(profiler), str(path.resolve()), str(args.rounds)]
        for flag in ('opt', 'debug', 'selection_all'):
            if getattr(args, flag):
                command.append('--' + flag.replace('_', '-'))
        row = json.loads(subprocess.check_output(command, text=True))
        for sample in row['runs']:
            exclusive = sum(stage['self_ms'] for stage in sample['stages'].values())
            root = sample['stages']['module_overhead']['inclusive_ms']
            if not math.isclose(exclusive, root, abs_tol=1e-8):
                raise RuntimeError(f'non-reconciling profile: {path}: {exclusive} != {root}')
        row['source_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        report['inputs'].append(row)
        print(path.name, row['functions'], 'functions', row['ir_instructions'], 'IR instructions', flush=True)
    report['summary'] = summarize(report['inputs'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['summary'], indent=2))


if __name__ == '__main__':
    main()
