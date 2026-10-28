#!/usr/bin/env python3
"""Apple ARM boolean ABI interop. Run under fast_opt_locked.py."""
import argparse
import json
from pathlib import Path
import sys
from cpp_coverage import Runner, Failed, digest, SENTINEL

ROOT = Path(__file__).resolve().parents[2]
HEADER = ('target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"\n'
          'target triple = "arm64-apple-macosx11.0.0"\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--out', required=True)
    a = p.parse_args()
    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    run = Runner(out, 120)
    report = dict(status='failed', commands=run.commands, checks=[], input_hashes={})
    importer = ROOT / 'suite/ir/llvm_import.py'
    keel = Path(a.keel).resolve()
    def import_(src, dest, accept=True):
        argv = [sys.executable, importer, '--libllvm', a.libllvm, '--keel', keel, src, '-o', dest]
        return run.require(argv) if accept else run.run(argv)
    try:
        report['compiler_sha256'] = digest(keel)
        report['importer_sha256'] = digest(importer)
        fixtures = ROOT / 'suite/ir/llvm_import_bool_fixtures'
        run.require(['clang', '-O2', '-c', fixtures/'driver.c', '-o', out/'driver.o'])
        for opt in ['O0', 'O1', 'O2']:
            ll = out/f'bool-{opt}.ll'
            run.require(['clang', '-'+opt, '-S', '-emit-llvm', fixtures/'bool.c', '-o', ll])
            report['input_hashes'][ll.name] = digest(ll)
            run.require(['clang', '-c', ll, '-o', out/'reference.o'])
            run.require(['clang', out/'driver.o', out/'reference.o', '-o', out/'reference'])
            expected = Path(run.require([out/'reference'])['stdout']).read_bytes()
            if expected != b'1024 boolean patterns passed\n':
                raise Failed('reference oracle failed')
            src = out/f'bool-{opt}.keel'
            import_(ll, src)
            for tier in ['none', 'fast', 'full']:
                obj, exe = out/f'{opt}-{tier}.o', out/f'{opt}-{tier}'
                run.require([keel, 'compile', '--opt-tier', tier, '--emit-object', src, '-o', obj])
                run.require(['clang', out/'driver.o', obj, '-o', exe])
                if Path(run.require([exe])['stdout']).read_bytes() != expected:
                    raise Failed('candidate oracle failed')
                report['checks'].append(dict(opt=opt, tier=tier, patterns=1024, status='passed'))
            if digest(ll) != report['input_hashes'][ll.name]:
                raise Failed('input LLVM changed')
        negative = {
            'plain-return': 'define i1 @bad() { ret i1 true }',
            'plain-argument': 'define i32 @bad(i1 %x) { %r = zext i1 %x to i32\nret i32 %r }',
            'signed-return': 'define signext i1 @bad() { ret i1 true }',
            'signed-argument': 'define i32 @bad(i1 signext %x) { %r = zext i1 %x to i32\nret i32 %r }',
            'call-mismatch': 'declare zeroext i1 @g(i1 zeroext)\ndefine zeroext i1 @bad(i1 zeroext %x) { %r = call signext i1 @g(i1 zeroext %x)\nret i1 %r }',
            'indirect-plain': 'define i32 @bad(ptr %f) { %r = call i1 %f()\n%z = zext i1 %r to i32\nret i32 %z }',
        }
        for name, body in negative.items():
            ll, dest = out/f'reject-{name}.ll', out/'preserved.keel'
            ll.write_text(HEADER+body+'\n')
            dest.write_bytes(SENTINEL)
            r = import_(ll, dest, False)
            if (r['returncode'] != 1 or Path(r['stdout']).read_bytes()
                    or dest.read_bytes() != SENTINEL
                    or 'i1 ABI' not in Path(r['stderr']).read_text()):
                raise Failed('rejection gate failed: '+name)
            report['checks'].append(dict(rejection=name, status='passed'))
        report['status'] = 'passed'
    except Exception as e:
        report['error'] = str(e)
    finally:
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(status=report['status'], checks=len(report['checks']), error=report.get('error'))))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
