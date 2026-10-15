#!/usr/bin/env python3
"""Diagnostic pass timings, not a controlled Keel-versus-LLVM allocator race.

Clang -O0 + mem2reg is prepared outside timing. LLVM and Keel consume different
IRs/register models and insert spills at different stages. LLVM Fast includes
rewriting; Greedy uses separate analyses, coalescing and rewriting. Record both
Greedy O0 (optimized RA enabled) and O2 to expose machine-pipeline sensitivity.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import re
import statistics
import subprocess


CLOSURE_NAMES = {
    'Fast Register Allocator', 'Greedy Register Allocator',
    'Live Variable Analysis', 'Live Interval Analysis', 'Live Register Matrix',
    'Register Coalescer', 'Virtual Register Rewriter', 'Virtual Register Map',
    'Eliminate PHI nodes for register allocation', 'Two-Address instruction pass',
    'Live Stack Slot Analysis', 'Stack Slot Coloring',
}


def run(command):
    return subprocess.run(list(map(str, command)), check=True, capture_output=True, text=True, timeout=180)


def passes(text):
    active = False
    result = {}
    for line in text.splitlines():
        if 'Pass execution timing report' in line:
            active = True
        if not active:
            continue
        values = re.findall(r'(\d+\.\d+)\s+\([^)]*\)', line)
        if len(values) != 4:
            continue
        name = re.split(r'\([^)]*\)', line, maxsplit=4)[-1].strip()
        # LLVM 23 optimized builds print an instruction-count column.
        name = re.sub(r'^\d+\s+', '', name)
        if name == 'Total':
            break
        result[name] = float(values[3]) * 1000.
    if not result:
        raise ValueError('could not parse LLVM primary pass timing table')
    return result


def closure(timings):
    return {name: ms for name, ms in timings.items()
            if name in CLOSURE_NAMES or name.startswith('Slot index numbering')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--llvm-bin', type=Path, default=Path('/opt/homebrew/opt/llvm/bin'))
    parser.add_argument('--keel-profiler', type=Path, required=True)
    parser.add_argument('--input', type=Path, action='append', required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=7)
    args = parser.parse_args()
    if args.rounds < 3:
        parser.error('use at least 3 rounds')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    clang, opt, llc = [args.llvm_bin / name for name in ('clang', 'opt', 'llc')]
    report = {'host': platform.platform(), 'llvm_version': run([llc, '--version']).stdout,
              'keel_commit': run(['git','rev-parse','HEAD']).stdout.strip(),
              'keel_profiler_sha256': hashlib.sha256(args.keel_profiler.read_bytes()).hexdigest(),
              'method': __doc__, 'rounds': args.rounds, 'inputs': [],
              'sources': ['https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-23.1.2/llvm/lib/CodeGen/RegAllocFast.cpp',
                          'https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-23.1.2/llvm/lib/CodeGen/RegAllocGreedy.cpp',
                          'https://llvm.org/docs/CodeGenerator.html'],
              'limitations': ['Not identical allocation inputs: different frontends, IRs, instruction selection, virtual registers and physical register pools.',
                             'Keel allocation includes CFG/RPO/liveness/ranges/scan/slot assignment, but spill/copy emission happens later during lowering.',
                             'LLVM selected allocation-related passes include spill rewriting and may include preparatory work absent from Keel; pass grouping is explicit and approximate.',
                             'LLVM O0/O2 change machine pipelines; they do not run the C frontend or LLVM middle-end during measurement.',
                             'Pass timers are diagnostic instrumentation. Keel timings are inclusive wall time; LLVM pass wall times have 0.1 ms printed resolution.',
                             'No generated-code runtime or quality conclusion can be drawn from these timings.']}
    rng = random.Random(2312)
    for index, source in enumerate(args.input):
        raw = args.work_dir / f'{index}-{source.stem}.raw.bc'
        bitcode = args.work_dir / f'{index}-{source.stem}.bc'
        prepare = [clang, '-w', '-c', '-emit-llvm', '-O0', '-Xclang', '-disable-O0-optnone',
                   '-fno-omit-frame-pointer', '-mcpu=generic', '-march=armv8-a+lse', source.resolve(), '-o', raw]
        run(prepare)
        run([opt, '-passes=mem2reg', raw, '-o', bitcode])
        llvm_modes = {'llvm-fast-O0': ['-O0', '-regalloc=fast'],
                      'llvm-greedy-O0': ['-O0', '-regalloc=greedy', '-optimize-regalloc=true'],
                      'llvm-greedy-O2': ['-O2', '-regalloc=greedy']}
        commands = {name: [llc,*flags,'-mcpu=generic','-mattr=+lse','-time-passes','-filetype=null',bitcode,'-o','/dev/null']
                    for name,flags in llvm_modes.items()}
        samples = {name: [] for name in commands}
        for iteration in range(args.rounds + 1):
            order = list(commands)
            rng.shuffle(order)
            for name in order:
                result = run(commands[name])
                timings = passes(result.stderr)
                allocation = 'Fast Register Allocator' if name == 'llvm-fast-O0' else 'Greedy Register Allocator'
                if allocation not in timings:
                    raise ValueError(f'missing allocator timer: {name}')
                if iteration:
                    selected = closure(timings)
                    samples[name].append({'allocator_pass_ms':timings[allocation],
                                          'selected_passes_ms':selected, 'selected_sum_ms':sum(selected.values()),
                                          'all_passes_ms':timings,'raw_timing_report':result.stderr})
        keel = {}
        for fast in (False,True):
            command = [args.keel_profiler.resolve(),source.resolve(),'9']
            if fast:
                command.append('--regalloc-fast')
            result = json.loads(run(command).stdout)
            selected = [r for r in result['runs'] if r['mode']=='structured']
            keel['fast' if fast else 'full'] = {'allocation_ms':[r['stages']['allocation_overhead']['inclusive_ms'] for r in selected],
                                               'scan_ms':[r['stages']['linear_scan']['self_ms'] for r in selected],
                                               'ir_instructions':result['ir_instructions'], 'functions':result['functions']}
        summary = {name:{'allocator_pass_median_ms':statistics.median(r['allocator_pass_ms'] for r in runs),
                         'selected_passes_median_ms':statistics.median(r['selected_sum_ms'] for r in runs)} for name,runs in samples.items()}
        summary['keel'] = {name:{'allocation_median_ms':statistics.median(r['allocation_ms']),
                                'scan_median_ms':statistics.median(r['scan_ms'])} for name,r in keel.items()}
        report['inputs'].append({'source':str(source),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
                                  'bitcode_sha256':hashlib.sha256(bitcode.read_bytes()).hexdigest(),
                                  'prepare_command':list(map(str,prepare)),
                                  'commands':{k:list(map(str,v)) for k,v in commands.items()},
                                  'llvm':samples,'keel':keel,'summary':summary})
        print(source.name,json.dumps(summary),flush=True)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n')


if __name__ == '__main__':
    main()
