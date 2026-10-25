#!/usr/bin/env python3
"""Run explicit V1 exports plus independent guest controls under TCG.

Invoke under suite/fast_opt_locked.py (this script does not acquire a lock).
"""
import argparse
import json
from pathlib import Path
import tempfile

import run

# One configuration owns each mode's object roster, bridge, entry symbols and
# controls. This keeps those selections together as new system families arrive.
CAMPAIGNS = {
    'observe': dict(prefix='descriptor', bridge='descriptor_bridge.S',
                    symbols=['descriptor_observe'], inline=[
                        ('no-observation', 'ret'),
                        ('corrupt-canary', 'sgdt 1(%rdi)\nsidt 17(%rdi)\nmovb $0, (%rdi)\nret')]),
    'table-loads': dict(prefix='descriptor-load', bridge='descriptor_bridge.S',
                        symbols=['descriptor_load'], defines=['KEEL_TABLE_LOADS'], inline=[
                            ('no-load', 'ret'),
                            ('swapped-tables', 'lgdt (%rsi)\nlidt (%rdi)\nret')]),
    'status': dict(prefix='descriptor-status', bridge='descriptor_status_bridge.S',
                   symbols=['descriptor_status'], reference='descriptor_status_reference.S', controls=[
                       ('reference', None, False),
                       ('normalized-r16', 'KEEL_NORMALIZE_R16', True),
                       ('wide-memory', 'KEEL_WIDE_MEMORY', True),
                       ('narrow-smsw', 'KEEL_NARROW_SMSW', True),
                       ('clobbered-old', 'KEEL_CLOBBER_OLD', True)]),
    'msr': dict(prefix='msr', bridge='msr_bridge.S', symbols=['msr_observe', 'msr_constant'],
                extra_exports=['msr-remat.o'], reference='msr_reference.S', controls=[
                    ('reference', None, False),
                    ('omitted-write', 'KEEL_OMIT_WRITE', True),
                    ('narrow-read', 'KEEL_NARROW_READ', True),
                    ('wrong-index', 'KEEL_WRONG_INDEX', True),
                    ('clobbered-live', 'KEEL_CLOBBER_LIVE', True)]),
    'cpu-control': dict(prefix='cpu-control', bridge='cpu_control_bridge.S',
                        symbols=['cpu_control'], reference='cpu_control_reference.S', controls=[
                            ('reference', None, False),
                            ('cache-omitted', 'KEEL_OMIT_CACHE', False),
                            ('omitted-clts', 'KEEL_OMIT_CLTS', True),
                            ('altered-cr0', 'KEEL_ALTER_CR0', True),
                            ('clobbered-live', 'KEEL_CLOBBER_LIVE', True)]),
    'system-loads': dict(prefix='system-load', bridge='system_load_bridge.S',
                         symbols=['system_reg', 'system_mem'], reference='system_load_reference.S', controls=[
                             ('reference', None, False),
                             ('omitted-lldt', 'KEEL_OMIT_LLDT', True),
                             ('omitted-ltr', 'KEEL_OMIT_LTR', True),
                             ('omitted-lmsw', 'KEEL_OMIT_LMSW', True),
                             ('clobbered-live', 'KEEL_CLOBBER_LIVE', True)]),
    'invlpg': dict(prefix='invlpg', bridge='invlpg_bridge.S', symbols=['invalidate'],
                   reference='invlpg_reference.S', controls=[
                       ('reference', None, False),
                       ('omitted-invalidation', 'KEEL_OMIT_INVALIDATE', True),
                       ('bad-marker', 'KEEL_BAD_MARKER', True),
                       ('bad-address-result', 'KEEL_BAD_ADDRESS', True)]),
}


def definitions(names):
    return [arg for name in names for arg in ('--defsym', f'{name}=1')]


def stub(symbols, body='ret'):
    return ('.text\n.code64\n' + ''.join(f'.global {s}\n{s}:\n{body}\n' for s in symbols)
            + '.section .note.GNU-stack,"",@progbits\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exports', type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    for mode in CAMPAIGNS:
        if mode != 'observe':
            modes.add_argument(f'--{mode}', dest='mode', action='store_const', const=mode)
    parser.set_defaults(mode='observe')
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--as', dest='assembler', default='as')
    parser.add_argument('--ld', dest='linker', default='ld')
    parser.add_argument('--objcopy', default='objcopy')
    parser.add_argument('--timeout', type=float, default=5)
    args = parser.parse_args()
    if not 0 < args.timeout <= 30:
        parser.error('timeout must be positive and at most 30 seconds')
    config = CAMPAIGNS[args.mode]
    names = [f'{config["prefix"]}-{s}-{a}-{spill}.o'
             for s in ('Baseline', 'Trees', 'Full')
             for a in ('Fast', 'Full') for spill in ('false', 'true')]
    names += config.get('extra_exports', [])
    if sorted(p.name for p in args.exports.glob('*.o')) != sorted(names):
        parser.error(f'exports must contain exactly the {len(names)} expected payload objects')
    assembler, linker, objcopy, qemu = map(
        run.tool, (args.assembler, args.linker, args.objcopy, args.qemu))
    work = Path(tempfile.mkdtemp(prefix='keel-descriptor-campaign-', dir='/tmp'))
    print(json.dumps(dict(artifacts=str(work))), flush=True)
    bridge = work / 'bridge.o'
    run.checked([assembler, '--64', *definitions(config.get('defines', [])), '-o', str(bridge),
                 str(run.HERE / 'cases' / config['bridge'])])
    cases = [(name, args.exports / name, False) for name in names]
    if 'reference' in config:
        for name, define, negative in config['controls']:
            obj = work / f'{name}.o'
            run.checked([assembler, '--64', *definitions([define] if define else []),
                         '-o', str(obj), str(run.HERE / 'cases' / config['reference'])])
            cases.append((name, obj, negative))
        controls = [('no-observation', 'ret')]
    else:
        controls = config['inline']
    # A fault or timeout cannot satisfy the expected FAIL transcript/exit pair.
    for name, body in controls:
        source, obj = work / f'{name}.S', work / f'{name}.o'
        source.write_text(stub(config['symbols'], body))
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
