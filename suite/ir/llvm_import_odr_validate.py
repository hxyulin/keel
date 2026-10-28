#!/usr/bin/env python3
"""General exported ODR/RTTI native differential checks (never localize ODR).

This script does not acquire a host lock.
--reference-only establishes Clang fixture behavior, not Keel correctness.
--out must be fresh. Full mode requires Apple AArch64, LLVM 23 and Keel.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
FIX = ROOT / 'suite/ir/llvm_import_odr_fixtures'
TOOL = ROOT / 'suite/ir/llvm_import.py'


def require(condition, detail):
    if not condition:
        raise AssertionError(detail)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference-only', action='store_true')
    parser.add_argument('--clang', default='clang++')
    parser.add_argument('--libllvm')
    parser.add_argument('--keel')
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    if not args.reference_only and not (args.libllvm and args.keel):
        parser.error('full mode requires --libllvm and --keel')
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = dict(status='failed', mode='reference-only' if args.reference_only else 'full',
                  keel_correctness_validated=False, commands=[], checks=[], modules={})
    snapshots = {}

    def run(command, ok=True, data=None):
        command = list(map(str, command))
        result = subprocess.run(command, input=data, capture_output=True, timeout=180)
        report['commands'].append(dict(argv=command, returncode=result.returncode,
                                      stdout=result.stdout.decode(errors='replace'),
                                      stderr=result.stderr.decode(errors='replace')))
        if ok:
            require(result.returncode == 0, report['commands'][-1])
        return result

    def unchanged():
        for path, original in snapshots.items():
            require(path.read_bytes() == original, f'input changed: {path}')

    def imp(source, dest=None, ok=True, data=None, keel=None):
        command = [sys.executable, TOOL, '--libllvm', args.libllvm,
                   '--keel', keel or args.keel, source]
        if dest is not None:
            command += ['-o', dest]
        result = run(command, ok=ok, data=data)
        unchanged()
        return result

    def execute(name, objects, expected):
        exe = out / name
        run([args.clang, *objects, '-o', exe])
        result = run([exe])
        require(result.stdout == expected, (name, result.stdout, expected))
        report['checks'].append(dict(kind='native', executable=name,
                                     independent_assertions=True))

    try:
        run([args.clang, '--version'])
        flags = ['-std=c++17', '-O1', '-fno-inline', '-fwrapv', '-ffp-contract=off',
                 '-Xclang', '-disable-lifetime-markers']
        paths = dict(left=FIX/'left.cpp', right=FIX/'right.cpp')
        paths.update({name: ROOT/f'suite/ir/cpp_coverage_fixtures/{name}.cpp'
                      for name in ('constexpr_runtime', 'virtual', 'eh')})
        sources, refs, drivers = {}, {}, {}
        for name, path in paths.items():
            ll = out/f'{name}.ll'
            run([args.clang, *flags, '-S', '-emit-llvm', path, '-o', ll])
            original = ll.read_bytes()
            snapshots[ll] = original
            sources[name] = ll
            text = original.decode()
            # Keep an exact inventory, including dead definitions and declarations.
            functions = re.findall(r'^(?:define|declare)\b[^@\n]*@([\w.$]+)\(', text, re.M)
            require(functions, f'empty module: {name}')
            report['modules'][name] = dict(sha256=hashlib.sha256(original).hexdigest(),
                functions=functions,
                odr_lines=[line for line in text.splitlines()
                           if re.search(r'\b(?:weak_odr|linkonce_odr)\b', line)],
                rtti_lines=[line for line in text.splitlines()
                            if line.startswith('@_ZTI') or line.startswith('@_ZTV')],
                intrinsics=sorted(set(re.findall(r'@llvm\.[\w.]+', text))))
            refs[name] = out/f'{name}.reference.o'
            # The very same, untouched LLVM module is the reference compiler input.
            run([args.clang, '-O1', '-c', ll, '-o', refs[name]])
        for name in ('left', 'right'):
            module = report['modules'][name]
            require(any('define weak_odr ' in line for line in module['odr_lines']), module)
            require(any('define linkonce_odr ' in line for line in module['odr_lines']), module)
            require(any(line.startswith('@function_slot = linkonce_odr') and 'ptr @_Z13inline_helperi' in line
                        for line in module['odr_lines']), module)
            require(any(line.startswith('@_ZTI7Derived =') for line in module['rtti_lines']), module)
        drivers['mixed'] = out/'mixed.driver.o'
        run([args.clang, *flags, '-c', FIX/'driver.cpp', '-o', drivers['mixed']])
        expected = dict(mixed=b'ODR: identity, storage, dispatch; 27 EH paths independently asserted\n')
        for order in (('left', 'right'), ('right', 'left')):
            execute(f'mixed.reference.{order[0]}-first',
                    [refs[n] for n in order] + [drivers['mixed']], expected['mixed'])
        for index, name in enumerate(('constexpr_runtime', 'virtual', 'eh')):
            drivers[name] = out/f'{name}.driver.o'
            run([args.clang, *flags, f'-DCASE={index}', '-c', FIX/'original_driver.cpp', '-o', drivers[name]])
            expected[name] = b'original: 6 independently asserted inputs\n'
            execute(f'{name}.reference', [drivers[name], refs[name]], expected[name])
        if args.reference_only:
            report['skipped'] = ['Keel import/verification', 'None/Fast/Full replacement matrix',
                                 'negative rejection and atomic output protocol']
        else:
            texts = {}
            for name, source in sources.items():
                dest = out/f'{name}.keel'
                dest.write_bytes(b'sentinel\n')
                result = imp(source, dest)
                require(not result.stdout, (name, 'unexpected stdout'))
                require(imp('-', data=snapshots[source]).stdout == dest.read_bytes(), name)
                actual = re.findall(r'^(?:func|decl) @([\w.$]+)\(', dest.read_text(), re.M)
                original_names = report['modules'][name]['functions']
                require([n for n in actual if n in original_names] == original_names,
                        (name, 'lost/reordered/duplicated input function', actual))
                require(set(actual) - set(original_names) <= {'_Unwind_Resume'},
                        (name, 'unexpected synthesized function', actual))
                texts[name] = dest
            for tier in ('none', 'fast', 'full'):
                objects = {}
                for name, text in texts.items():
                    objects[name] = out/f'{name}.{tier}.o'
                    run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', objects[name]])
                for replacement in ('left', 'right', 'all'):
                    for order in (('left', 'right'), ('right', 'left')):
                        selected = [objects[n] if replacement in (n, 'all') else refs[n] for n in order]
                        execute(f'mixed.{replacement}.{tier}.{order[0]}-first',
                                [*selected, drivers['mixed']], expected['mixed'])
                for name in ('constexpr_runtime', 'virtual', 'eh'):
                    execute(f'{name}.{tier}', [drivers[name], objects[name]], expected[name])
            header = '\n'.join(line for line in sources['left'].read_text().splitlines()
                               if line.startswith(('target datalayout', 'target triple'))) + '\n'
            negatives = {
                'weak-function': 'define weak i32 @bad() { ret i32 1 }',
                'weak-global': '@bad = weak global i32 1',
                'externweak-function': 'declare extern_weak i32 @bad()',
                'externweak-global': '@bad = extern_weak global i32',
                'hidden-function': 'define linkonce_odr hidden i32 @bad() { ret i32 1 }',
                'hidden-global': '@bad = linkonce_odr hidden global i32 1',
                'protected-function': 'define linkonce_odr protected i32 @bad() { ret i32 1 }',
                'comdat-function': '$g = comdat any\ndefine linkonce_odr i32 @bad() comdat($g) { ret i32 1 }',
                'comdat-global': '$g = comdat any\n@bad = linkonce_odr global i32 1, comdat($g)',
                'tls': '@bad = linkonce_odr thread_local global i32 1',
            }
            for name, body in negatives.items():
                source = out/f'reject-{name}.ll'
                source.write_text(header + 'define i32 @good() { ret i32 7 }\n' + body + '\n')
                snapshots[source] = source.read_bytes()
                dest = out/f'reject-{name}.keel'
                dest.write_bytes(b'sentinel\n')
                for target in (dest, out/f'absent-{name}.keel', None):
                    result = imp(source, target, ok=False)
                    require(result.returncode == 1 and not result.stdout and b'llvm-import:' in result.stderr,
                            (name, report['commands'][-1]))
                    require(dest.read_bytes() == b'sentinel\n', name)
                    require(not (out/f'absent-{name}.keel').exists(), name)
                report['checks'].append(dict(kind='negative-output-preservation', case=name,
                                             diagnostic=result.stderr.decode(errors='replace')))
            for verifier in ('/usr/bin/false', str(out/'missing-keel')):
                dest = out/'verifier-preserved.keel'
                dest.write_bytes(b'sentinel\n')
                absent = out/'verifier-absent.keel'
                for target in (dest, absent, None):
                    result = imp(sources['left'], target, ok=False, keel=verifier)
                    require(result.returncode == 1 and not result.stdout, report['commands'][-1])
                    require(dest.read_bytes() == b'sentinel\n' and not absent.exists(), verifier)
                report['checks'].append(dict(kind='verifier-output-preservation', verifier=verifier))
            report['keel_correctness_validated'] = True
        unchanged()
        report['status'] = 'passed'
        report['native_runs'] = sum(check['kind'] == 'native' for check in report['checks'])
        print(json.dumps({key: report[key] for key in
                         ('status', 'mode', 'keel_correctness_validated', 'native_runs')}))
    finally:
        try:
            unchanged()
        finally:
            (out/'validation.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
