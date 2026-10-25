#!/usr/bin/env python3
"""Run exactly twelve V1 exports plus descriptor negative controls under TCG.

Invoke under suite/fast_opt_locked.py (this script does not acquire a lock).
"""
import argparse
import json
from pathlib import Path
import tempfile

import run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exports', type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--table-loads', action='store_true',
                       help='validate LGDT/LIDT exports using a trusted observer')
    modes.add_argument('--status', action='store_true',
                       help='validate all twelve SLDT/STR/SMSW forms')
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--as', dest='assembler', default='as')
    parser.add_argument('--ld', dest='linker', default='ld')
    parser.add_argument('--objcopy', default='objcopy')
    parser.add_argument('--timeout', type=float, default=5)
    args = parser.parse_args()
    if not 0 < args.timeout <= 30:
        parser.error('timeout must be positive and at most 30 seconds')
    prefix = ('descriptor-status' if args.status else
              'descriptor-load' if args.table_loads else 'descriptor')
    names = [f'{prefix}-{s}-{a}-{spill}.o'
             for s in ('Baseline', 'Trees', 'Full')
             for a in ('Fast', 'Full') for spill in ('false', 'true')]
    if sorted(p.name for p in args.exports.glob('*.o')) != sorted(names):
        parser.error('exports must contain exactly the twelve expected descriptor objects')
    assembler, linker, objcopy, qemu = map(
        run.tool, (args.assembler, args.linker, args.objcopy, args.qemu))
    work = Path(tempfile.mkdtemp(prefix='keel-descriptor-campaign-', dir='/tmp'))
    print(json.dumps(dict(artifacts=str(work))), flush=True)
    bridge = work / 'bridge.o'
    defines = ['--defsym', 'KEEL_TABLE_LOADS=1'] if args.table_loads else []
    bridge_source = 'descriptor_status_bridge.S' if args.status else 'descriptor_bridge.S'
    run.checked([assembler, '--64', *defines, '-o', str(bridge),
                 str(run.HERE / 'cases' / bridge_source)])
    cases = [(name, args.exports / name, False) for name in names]
    # Controls must return normally with incorrect state or output. A fault
    # or timeout cannot satisfy the expected FAIL transcript and exit status.
    if args.status:
        for name, define in (
                ('reference', None),
                ('normalized-r16', 'KEEL_NORMALIZE_R16'),
                ('wide-memory', 'KEEL_WIDE_MEMORY'),
                ('narrow-smsw', 'KEEL_NARROW_SMSW'),
                ('clobbered-old', 'KEEL_CLOBBER_OLD')):
            obj = work / f'{name}.o'
            defines = ['--defsym', f'{define}=1'] if define else []
            run.checked([assembler, '--64', *defines, '-o', str(obj),
                         str(run.HERE / 'cases/descriptor_status_reference.S')])
            cases.append((name, obj, define is not None))
        source, obj = work / 'no-observation.S', work / 'no-observation.o'
        source.write_text('.text\n.code64\n.global descriptor_status\n'
                          'descriptor_status:\nret\n'
                          '.section .note.GNU-stack,"",@progbits\n')
        run.checked([assembler, '--64', '-o', str(obj), str(source)])
        cases.append(('no-observation', obj, True))
    else:
        controls = (
            ('no-load', 'ret'),
            ('swapped-tables', 'lgdt (%rsi)\nlidt (%rdi)\nret'),
        ) if args.table_loads else (
            ('no-observation', 'ret'),
            ('corrupt-canary', 'sgdt 1(%rdi)\nsidt 17(%rdi)\nmovb $0, (%rdi)\nret'))
        symbol = 'descriptor_load' if args.table_loads else 'descriptor_observe'
        for name, body in controls:
            source, obj = work / f'{name}.S', work / f'{name}.o'
            source.write_text(f'.text\n.code64\n.global {symbol}\n'
                              f'{symbol}:\n' + body +
                              '\n.section .note.GNU-stack,"",@progbits\n')
            run.checked([assembler, '--64', '-o', str(obj), str(source)])
            cases.append((name, obj, True))
    passed = True
    with (work / 'results.jsonl').open('w') as log:
        for name, obj, negative in cases:
            case = work / name
            case.mkdir()
            image = run.build(case, [bridge, obj], assembler, linker, objcopy)
            result = run.execute(qemu, image, case, args.timeout)
            expected = (result['status'] == 'guest-error'
                        and result['returncode'] == 35
                        and result['transcript'] == (run.READY + run.FAIL).decode()
                        if negative else result['status'] == 'pass'
                        and result['transcript'] == (run.READY + run.PASS).decode())
            result.update(case=name, artifacts=str(case), negative=negative,
                          expected_verdict=expected)
            line = json.dumps(result, sort_keys=True)
            print(line, flush=True)
            log.write(line + '\n')
            log.flush()
            passed &= expected
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
