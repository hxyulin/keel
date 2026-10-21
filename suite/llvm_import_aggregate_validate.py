#!/usr/bin/env python3
"""Bounded Apple ARM two-word aggregate ABI interop. Run under fast_opt_locked.py."""
import argparse
import json
import html
from pathlib import Path
import sys
from cpp_coverage import Runner, Failed, digest, SENTINEL, classify_import

ROOT = Path(__file__).resolve().parents[1]
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
    importer = ROOT / 'suite/llvm_import.py'
    keel = Path(a.keel).resolve()
    def import_(src, dest, accept=True):
        argv = [sys.executable, importer, '--libllvm', a.libllvm, '--keel', keel, src, '-o', dest]
        return run.require(argv) if accept else run.run(argv)
    try:
        report['compiler_sha256'] = digest(keel)
        report['importer_sha256'] = digest(importer)
        fixtures = ROOT / 'suite/llvm_import_aggregate_fixtures'
        report['source_hashes'] = {f.name: digest(f) for f in fixtures.iterdir() if f.is_file()}
        report['clang_version'] = Path(run.require(['clang', '--version'])['stdout']).read_text()
        report['libllvm_sha256'] = digest(Path(a.libllvm))
        run.require(['clang', '-O2', '-c', fixtures/'driver.c', '-o', out/'driver.o'])
        for opt in ['O1', 'O2']:
            ll = out/f'pair-{opt}.ll'
            run.require(['clang', '-'+opt, '-S', '-emit-llvm', fixtures/'pair.c', '-o', ll])
            report['input_hashes'][ll.name] = digest(ll)
            run.require(['clang', '-c', ll, '-o', out/'reference.o'])
            run.require(['clang', out/'driver.o', out/'reference.o', '-o', out/'reference'])
            expected = Path(run.require([out/'reference'])['stdout']).read_bytes()
            if expected != b'1408 aggregate boundary patterns passed\n':
                raise Failed('reference oracle failed')
            src = out/f'pair-{opt}.keel'
            import_(ll, src)
            for tier in ['none', 'fast', 'full']:
                obj, exe = out/f'{opt}-{tier}.o', out/f'{opt}-{tier}'
                run.require([keel, 'compile', '--opt-tier', tier, '--emit-object', src, '-o', obj])
                run.require(['clang', out/'driver.o', obj, '-o', exe])
                if Path(run.require([exe])['stdout']).read_bytes() != expected:
                    raise Failed('candidate oracle failed')
                report['checks'].append(dict(opt=opt, tier=tier, patterns=1408, status='passed'))
            if digest(ll) != report['input_hashes'][ll.name]:
                raise Failed('input LLVM changed')
        run.require(['clang++', '-std=c++17', '-O2', '-c', fixtures/'view_driver.cpp', '-o', out/'view-driver.o'])
        for opt in ['O1', 'O2']:
            ll = out/f'view-{opt}.ll'
            run.require(['clang++', '-std=c++17', '-'+opt, '-S', '-emit-llvm', fixtures/'view.cpp', '-o', ll])
            report['input_hashes'][ll.name] = digest(ll)
            run.require(['clang++', '-c', ll, '-o', out/'view-reference.o'])
            run.require(['clang++', out/'view-driver.o', out/'view-reference.o', '-o', out/'view-reference'])
            expected = Path(run.require([out/'view-reference'])['stdout']).read_bytes()
            if expected != b'22 string_view patterns passed\n':
                raise Failed('string_view reference oracle failed')
            src = out/f'view-{opt}.keel'
            import_(ll, src)
            for tier in ['none', 'fast', 'full']:
                obj, exe = out/f'view-{opt}-{tier}.o', out/f'view-{opt}-{tier}'
                run.require([keel, 'compile', '--opt-tier', tier, '--emit-object', src, '-o', obj])
                run.require(['clang++', out/'view-driver.o', obj, '-o', exe])
                if Path(run.require([exe])['stdout']).read_bytes() != expected:
                    raise Failed('string_view candidate oracle failed')
                report['checks'].append(dict(fixture='std::string_view', opt=opt, tier=tier, patterns=22, status='passed'))
            if digest(ll) != report['input_hashes'][ll.name]:
                raise Failed('input LLVM changed')
        negative = {
            'return': 'define [2 x i64] @bad([2 x i64] %x) { ret [2 x i64] %x }',
            'one-word': 'declare void @bad([1 x i64])',
            'three-word': 'declare void @bad([3 x i64])',
            'struct': 'declare void @bad({i64,i64})',
            'float': 'declare void @bad([2 x double])',
            'narrow': 'declare void @bad([2 x i32])',
            'nested': 'declare void @bad([1 x [2 x i64]])',
            'byval': 'declare void @bad(ptr byval([2 x i64]))',
            'inreg': 'declare void @bad([2 x i64] inreg)',
            'load': 'define i64 @bad(ptr %p) { %x = load [2 x i64], ptr %p\n%r = extractvalue [2 x i64] %x, 0\nret i64 %r }',
            'phi': 'define i64 @bad([2 x i64] %x) { br label %next\nnext: %p = phi [2 x i64] [%x, %0]\n%r = extractvalue [2 x i64] %p, 0\nret i64 %r }',
            'partial-poison': 'declare i64 @g([2 x i64])\ndefine i64 @bad(i64 %x) { %a = insertvalue [2 x i64] poison, i64 %x, 0\n%r = call i64 @g([2 x i64] %a)\nret i64 %r }',
            'partial-undef': 'declare i64 @g([2 x i64])\ndefine i64 @bad(i64 %x) { %a = insertvalue [2 x i64] undef, i64 %x, 1\n%r = call i64 @g([2 x i64] %a)\nret i64 %r }',
            'poison-extract': 'define i64 @bad() { %r = extractvalue [2 x i64] poison, 0\nret i64 %r }',
            'constant-aggregate': 'declare i64 @g([2 x i64])\ndefine i64 @bad() { %r = call i64 @g([2 x i64] [i64 1, i64 2])\nret i64 %r }',
            'call-return': 'declare [2 x i64] @g()\ndefine i64 @bad() { %a = call [2 x i64] @g()\n%r = extractvalue [2 x i64] %a, 0\nret i64 %r }',
            'call-contract': 'declare i64 @g([2 x i64])\ndefine i64 @bad([2 x i64] %a) { %r = call i64 @g([2 x i64] inreg %a)\nret i64 %r }',
            'late-function': 'define i64 @good([2 x i64] %a) { %r = extractvalue [2 x i64] %a, 1\nret i64 %r }\ndefine i128 @bad(i128 %x) { ret i128 %x }',
        }
        for name, body in negative.items():
            ll, dest = out/f'reject-{name}.ll', out/'preserved.keel'
            ll.write_text(HEADER+body+'\n')
            dest.write_bytes(SENTINEL)
            before = digest(ll)
            r = import_(ll, dest, False)
            if (r['returncode'] != 1 or Path(r['stdout']).read_bytes()
                    or dest.read_bytes() != SENTINEL or digest(ll) != before
                    or classify_import(r['returncode'], Path(r['stdout']).read_bytes(),
                                       Path(r['stderr']).read_text(), dest.read_bytes() == SENTINEL) != 'rejected'):
                raise Failed('rejection gate failed: '+name)
            report['checks'].append(dict(rejection=name, status='passed', diagnostic=Path(r['stderr']).read_text().strip(), source_preserved=True, destination_preserved=True))
        report['status'] = 'passed'
    except Exception as e:
        report['error'] = str(e)
    finally:
        (out/'report.html').write_text('<!doctype html><meta charset="utf-8"><title>Aggregate ABI correctness</title><style>body{font:15px system-ui;max-width:1000px;margin:40px auto}pre{white-space:pre-wrap}</style><h1>Bounded two-word aggregate ABI</h1><p>Untouched Clang O1/O2 input. Cross-object Clang → Keel → Clang direct and callback assertions. No performance claims. Explicit byval transport retains register exhaustion and stack alignment.</p><pre>'+html.escape(json.dumps(report, indent=2))+'</pre>')
        (out/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(status=report['status'], checks=len(report['checks']), error=report.get('error'))))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
