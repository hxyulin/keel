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
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--as', dest='assembler', default='as')
    parser.add_argument('--ld', dest='linker', default='ld')
    parser.add_argument('--objcopy', default='objcopy')
    parser.add_argument('--timeout', type=float, default=5)
    args = parser.parse_args()
    if not 0 < args.timeout <= 30:
        parser.error('timeout must be positive and at most 30 seconds')
    names = [f'descriptor-{s}-{a}-{spill}.o'
             for s in ('Baseline', 'Trees', 'Full')
             for a in ('Fast', 'Full') for spill in ('false', 'true')]
    if sorted(p.name for p in args.exports.glob('*.o')) != sorted(names):
        parser.error('exports must contain exactly the twelve expected descriptor objects')
    assembler, linker, objcopy, qemu = map(
        run.tool, (args.assembler, args.linker, args.objcopy, args.qemu))
    work = Path(tempfile.mkdtemp(prefix='keel-descriptor-campaign-', dir='/tmp'))
    print(json.dumps(dict(artifacts=str(work))), flush=True)
    bridge = work / 'bridge.o'
    run.checked([assembler, '--64', '-o', str(bridge),
                 str(run.HERE / 'cases/descriptor_bridge.S')])
    cases = [(name, args.exports / name, False) for name in names]
    # Missing observations must fail, as must a correct observation followed
    # by a one-byte canary overwrite. Both still return normally to the bridge.
    for name, body in (
            ('no-observation', 'ret'),
            ('corrupt-canary', 'sgdt 1(%rdi)\nsidt 17(%rdi)\nmovb $0, (%rdi)\nret')):
        source, obj = work / f'{name}.S', work / f'{name}.o'
        source.write_text('.text\n.code64\n.global descriptor_observe\n'
                          'descriptor_observe:\n' + body +
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
