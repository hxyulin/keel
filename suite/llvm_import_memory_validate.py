#!/usr/bin/env python3
"""Bounded LLVM memory-intrinsic import gates; run under fast_opt_locked.py."""
import argparse
import json
from pathlib import Path
import sys
from cpp_coverage import Runner, Failed, digest, SENTINEL

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--out', required=True)
    a = p.parse_args()
    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    run = Runner(out, 120)
    report = dict(status='failed', commands=run.commands, checks=[])
    importer = ROOT/'suite/llvm_import.py'
    fixtures = ROOT/'suite/llvm_import_memory_fixtures'
    keel = Path(a.keel).resolve()
    def import_(src, dst):
        return [sys.executable, importer, '--libllvm', a.libllvm, '--keel', keel, src, '-o', dst]
    try:
        report['compiler_sha256'] = digest(keel)
        report['importer_sha256'] = digest(importer)
        source = fixtures/'memory.ll'
        report['input_sha256'] = digest(source)
        run.require(import_(source, out/'memory.keel'))
        text = (out/'memory.keel').read_text()
        if 'decl @llvm.mem' in text or any('intrinsic '+n not in text for n in ['memmove', 'memset']):
            raise Failed('intrinsic translation structure')
        run.require(['clang', '-O2', '-c', fixtures/'driver.c', '-o', out/'driver.o'])
        run.require(['clang', '-c', source, '-o', out/'reference.o'])
        run.require(['clang', out/'driver.o', out/'reference.o', '-o', out/'reference'])
        expected = b'memory intrinsic oracle passed\n'
        if Path(run.require([out/'reference'])['stdout']).read_bytes() != expected:
            raise Failed('reference oracle')
        for tier in ['none', 'fast', 'full']:
            for route in ['object', 'text']:
                stem = out/(tier+'-'+route)
                obj = stem.with_suffix('.o')
                if route == 'object':
                    run.require([keel, 'compile', '--verify-each', '--opt-tier', tier, '--emit-object', out/'memory.keel', '-o', obj])
                else:
                    asm = stem.with_suffix('.s')
                    run.require([keel, 'compile', '--verify-each', '--opt-tier', tier, out/'memory.keel', '-o', asm])
                    run.require([keel, 'asm', asm, '-o', obj])
                run.require(['clang', out/'driver.o', obj, '-o', stem])
                if Path(run.require([stem])['stdout']).read_bytes() != expected:
                    raise Failed('candidate oracle')
                report['checks'].append(dict(tier=tier, route=route, status='passed'))
        original = source.read_text()
        negatives = {
            'volatile': original.replace('i1 false', 'i1 true'),
            'i32-length': original.replace('.i64', '.i32').replace('i64', 'i32'),
            'inline': original.replace('llvm.memcpy.p0.p0', 'llvm.memcpy.inline.p0.p0'),
            'ordinary-i1': original + '\ndefine i1 @bad() { ret i1 true }\n',
            'address': original + '\n@escaped = global ptr @llvm.memcpy.p0.p0.i64\n',
        }
        for name, text in negatives.items():
            ll, dest = out/(name+'.ll'), out/'sentinel.keel'
            ll.write_text(text)
            dest.write_bytes(SENTINEL)
            row = run.run(import_(ll, dest))
            diagnostic = Path(row['stderr']).read_text()
            if row['returncode'] != 1 or Path(row['stdout']).read_bytes() or dest.read_bytes() != SENTINEL:
                raise Failed('rejection gate '+name)
            # Parser/verifier errors are not feature rejection evidence.
            if name in ('i32-length', 'inline') and 'unsupported parameter/return ABI attribute' not in diagnostic:
                raise Failed('unsupported overload diagnostic')
            if name == 'address' and 'Invalid user of intrinsic instruction' not in diagnostic:
                raise Failed('LLVM verifier did not reject intrinsic address')
            if name == 'volatile' and 'volatile memory intrinsics' not in diagnostic:
                raise Failed('volatile rejection diagnostic')
            if name == 'ordinary-i1' and 'i1 ABI' not in diagnostic:
                raise Failed('ordinary ABI changed')
            report['checks'].append(dict(rejection=name, layer='LLVM verifier' if name == 'address' else 'importer', diagnostic=diagnostic, status='passed'))
        report['status'] = 'passed'
    except Exception as e:
        report['error'] = str(e)
    finally:
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(status=report['status'], checks=len(report['checks']), error=report.get('error'))))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
