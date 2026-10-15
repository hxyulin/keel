#!/usr/bin/env python3
"""Compare prebuilt x86 object-generation pipelines on the read-only 44-file C corpus.

Works on macOS and Linux, always targeting x86_64 Linux ELF. Measures complete
Keel compilation/object emission, or the same new Keel emitting text followed
by Clang assembly. No linking, execution, or Clang C compilation is timed.
"""
import argparse
import json
import os
import platform
import time
from pathlib import Path

from compare_native_toolchain import invoke, paired_summary, sha, text_section


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['before', 'after', 'prepared', 'out']:
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--pairs', type=int, default=7)
    p.add_argument('--cpu', type=int)
    p.add_argument('--before-revision')
    p.add_argument('--after-revision')
    a = p.parse_args()
    if a.pairs < 3:
        p.error('use at least three paired samples')
    for name in ['before', 'after', 'prepared', 'out']:
        setattr(a, name, getattr(a, name).resolve())
    if a.out == a.prepared or a.out.is_relative_to(a.prepared):
        p.error('output must be outside prepared inputs')
    if a.cpu is not None:
        if not hasattr(os, 'sched_setaffinity'):
            p.error('--cpu requires Linux CPU affinity support')
        os.sched_setaffinity(0, {a.cpu})
    inputs = [(suite, src) for suite in ['lua', 'quickjs', 'sqlite']
              for src in sorted((a.prepared / suite / 'pre').glob('*.i'))]
    if len(inputs) != 44:
        p.error(f'expected 44 prepared inputs, found {len(inputs)}')
    a.out.mkdir(parents=True, exist_ok=True)
    target = 'x86_64-unknown-linux-gnu'
    r = {'host': platform.node(), 'platform': platform.platform(), 'target': target,
         'cpu_affinity': a.cpu, 'before_commit': a.before_revision,
         'after_commit': a.after_revision,
         'binaries': {k: {'path': str(v), 'sha256': sha(v)}
                      for k, v in [('before', a.before), ('after', a.after)]},
         'clang': invoke(['clang', '--version']).decode().splitlines()[0],
         'method': 'Prebuilt compilers; one warmup, alternating forward/reverse paired serial '
                   '44-file batches. Wall time includes CLI startup, C compilation, assembly, '
                   'input reads and output writes. External uses the after compiler then Clang '
                   'assembly of its source. Excludes linking and application runtime. '
                   'Not a Clang C-codegen comparison. All targets are x86 Linux ELF, including on Mac.',
         'inputs': [{'suite': suite, 'path': str(src), 'sha256': sha(src)} for suite, src in inputs],
         'modes': {}, 'correctness': {}}

    def save():
        (a.out / 'report.json').write_text(json.dumps(r, indent=2) + '\n')

    for mode, flags in [('baseline', []), ('opt', ['-O']), ('debug', ['-g']),
                        ('opt-select', ['-O', '--isel', 'all'])]:
        commands = {'before': [], 'after': [], 'external': []}
        sizes = {k: {'object_bytes': 0, 'text_bytes': 0} for k in commands}
        for suite, src in inputs:
            out = a.out / mode / suite
            out.mkdir(parents=True, exist_ok=True)
            objects = {}
            for name, compiler in [('before', a.before), ('after', a.after)]:
                obj = out / (src.stem + '.' + name + '.o')
                cmd = [compiler, 'cc', '--target', target, *flags, '--emit-object', src, '-o', obj]
                commands[name].append(cmd)
                invoke(cmd)
                objects[name] = obj
            assert objects['before'].read_bytes() == objects['after'].read_bytes(), (mode, src)
            asm, obj = out / (src.stem + '.s'), out / (src.stem + '.external.o')
            generate = [a.after, 'cc', '--target', target, *flags, src, '-o', asm]
            assemble = ['clang', '-target', target, '-x', 'assembler', '-c', asm, '-o', obj]
            commands['external'].extend([generate, assemble])
            invoke(generate)
            invoke(assemble)
            objects['external'] = obj
            for name, path in objects.items():
                sizes[name]['object_bytes'] += path.stat().st_size
                sizes[name]['text_bytes'] += len(text_section(path))
        r['correctness'][mode] = {'objects_compared': len(inputs), 'before_after_byte_identical': True,
                                  'after_text_accepted_by_clang': True, 'aggregate_sizes': sizes}
        save()
        if mode not in ['baseline', 'opt']:
            print(mode, 'correctness PASS', flush=True)
            continue
        samples = {k: [] for k in commands}
        for i in range(a.pairs + 1):
            order = list(commands) if i % 2 == 0 else list(commands)[::-1]
            for name in order:
                start = time.perf_counter_ns()
                for cmd in commands[name]:
                    invoke(cmd)
                if i:
                    samples[name].append((time.perf_counter_ns() - start) / 1e6)
        r['modes'][mode] = paired_summary(samples, 'before')
        save()
        print(mode, r['modes'][mode], flush=True)


if __name__ == '__main__':
    main()
