#!/usr/bin/env python3
"""Private LLVM fastcc import gates; run under fast_opt_locked.py."""
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
    fixtures = ROOT/'suite/llvm_import_fast_fixtures'
    keel = Path(a.keel).resolve()
    def import_(src, dst):
        return [sys.executable, importer, '--libllvm', a.libllvm, '--keel', keel, src, '-o', dst]
    try:
        report['compiler_sha256'] = digest(keel)
        report['importer_sha256'] = digest(importer)
        source = fixtures/'fast.ll'
        report['input_sha256'] = digest(source)
        run.require(import_(source, out/'fast.keel'))
        text = (out/'fast.keel').read_text()
        if text != (fixtures/'fast.keel').read_text():
            raise Failed('imported fast fixture changed; review the golden')
        if text.count('cc(fast) local') != 4:
            raise Failed('fast convention translation structure')
        run.require(['clang', '-O2', '-c', fixtures/'driver.c', '-o', out/'driver.o'])
        run.require(['clang', '-c', source, '-o', out/'reference.o'])
        run.require(['clang', out/'driver.o', out/'reference.o', '-o', out/'reference'])
        expected = b'516 private fastcc patterns passed\n'
        if Path(run.require([out/'reference'])['stdout']).read_bytes() != expected:
            raise Failed('reference oracle')
        for target in ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu']:
            for tier in ['none', 'fast', 'full']:
                for selector in ['baseline', 'full']:
                    for allocator in ['fast', 'full']:
                        for route in ['object', 'text']:
                            stem = out/('-'.join([target,tier,selector,allocator,route]))
                            obj = stem.with_suffix('.o')
                            flags = ['--target', target, '--verify-each', '--opt-tier', tier,
                                     '--isel-tier', selector, '--regalloc', allocator]
                            if route == 'object':
                                run.require([keel, 'compile', *flags, '--emit-object', out/'fast.keel', '-o', obj])
                            else:
                                asm = stem.with_suffix('.s')
                                run.require([keel, 'compile', *flags, out/'fast.keel', '-o', asm])
                                run.require([keel, 'asm', '--target', target, asm, '-o', obj])
                            native = target == 'aarch64-apple-darwin'
                            if native:
                                run.require(['clang', out/'driver.o', obj, '-o', stem])
                                if Path(run.require([stem])['stdout']).read_bytes() != expected:
                                    raise Failed('candidate oracle')
                            report['checks'].append(dict(target=target,tier=tier,selector=selector,
                                allocator=allocator,route=route,native=native,status='passed'))
        header = source.read_text().split('declare i64')[0]
        negatives = {
            'signed-boolean': 'define private fastcc signext i1 @bad() { ret i1 true }',
            'external': 'define fastcc i64 @bad(i64 %x) { ret i64 %x }',
            'declaration': 'declare fastcc i64 @bad(i64)',
            'escaping-address': 'define internal fastcc i64 @bad(i64 %x) { ret i64 %x }\n@escaped = global ptr @bad',
            'mismatched-call': 'define internal fastcc i64 @bad(i64 %x) { ret i64 %x }\ndefine i64 @f(i64 %x) { %r = call i64 @bad(i64 %x)\nret i64 %r }',
            'escaping-argument': 'define internal fastcc void @bad(ptr %x) { ret void }\ndefine void @f() { call fastcc void @bad(ptr @bad)\nret void }',
        }
        for name, body in negatives.items():
            ll, dest = out/(name+'.ll'), out/'sentinel.keel'
            ll.write_text(header+body+'\n')
            dest.write_bytes(SENTINEL)
            row = run.run(import_(ll, dest))
            diagnostic = Path(row['stderr']).read_text()
            if (row['returncode'] != 1 or Path(row['stdout']).read_bytes()
                    or dest.read_bytes() != SENTINEL or not any(marker in diagnostic for marker in
                        (('i1 ABI',) if name == 'signed-boolean' else
                         ('fastcc requires', 'fastcc function address escapes')))):
                raise Failed('rejection gate '+name+': '+diagnostic)
            report['checks'].append(dict(rejection=name, diagnostic=diagnostic, status='passed'))
        report['status'] = 'passed'
    except Exception as e:
        report['error'] = str(e)
    finally:
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(status=report['status'], checks=len(report['checks']), error=report.get('error'))))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
