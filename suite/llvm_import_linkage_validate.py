#!/usr/bin/env python3
"""Bounded ODR local-copy validation; run through suite/fast_opt_locked.py.

Takes no host lock itself. --out must be a fresh private artifact directory.
Compiler-produced LLVM is imported and reference-compiled without byte edits.
The explicit local-copy contract forbids outside-TU references to candidates.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'suite/llvm_import.py'
FIX = ROOT / 'suite/llvm_import_linkage_fixtures'
HEADER = (FIX/'available.ll').read_text().split('define', 1)[0]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True)
    p.add_argument('--keel', required=True)
    p.add_argument('--clang', default='clang')
    p.add_argument('--out', required=True)
    args = p.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    commands, checks = [], []

    def run(command, ok=True, data=None):
        command = list(map(str, command))
        commands.append(command)
        r = subprocess.run(command, capture_output=True, input=data, timeout=180)
        if ok:
            assert r.returncode == 0, (command, r.returncode, r.stderr.decode(errors='replace'))
        return r

    def imp(source, dest, opt=True, keel=None, data=None):
        return run([sys.executable, TOOL, '--libllvm', args.libllvm,
                    '--keel', keel or args.keel,
                    *(['--localize-odr-functions'] if opt else []), source,
                    '-o', dest], ok=False, data=data)

    def reject(name, source, opt=True):
        original = source.read_bytes()
        dest = out/'preserved.keel'
        dest.write_bytes(b'sentinel\n')
        r = imp(source, dest, opt)
        assert r.returncode == 1 and not r.stdout and b'llvm-import:' in r.stderr, (name, r)
        assert dest.read_bytes() == b'sentinel\n' and source.read_bytes() == original
        absent = out/'absent.keel'
        r2 = imp(source, absent, opt)
        assert r2.returncode == 1 and not r2.stdout and not absent.exists(), name
        checks.append(dict(kind='rejection-preservation', case=name, diagnostic=r.stderr.decode()))

    def accept(name, source, opt=True):
        original = source.read_bytes()
        dest = out/f'{name}.keel'
        dest.write_bytes(b'replace-on-success\n')
        r = imp(source, dest, opt=opt)
        assert r.returncode == 0 and not r.stdout, r.stderr.decode()
        assert original == source.read_bytes()
        names = re.findall(rb'^(?:define|declare)\b[^@\n]*@([\w.$]+)\(', original, re.M)
        output_names = re.findall(r'^(?:func|decl) @([\w.$]+)\(', dest.read_text(), re.M)
        assert [s.decode() for s in names] == output_names, (name, names, output_names)
        candidates = re.findall(rb'^define (?:linkonce_odr|available_externally)\b[^@\n]*@([\w.$]+)\(', original, re.M)
        for symbol in candidates:
            line = next(line for line in dest.read_text().splitlines() if line.startswith('func @' + symbol.decode() + '('))
            assert ' cc(c)' in line and line.endswith(' local {' if opt else ' weak_odr {'), line
        checks.append(dict(kind='retained-all-functions-local-copies', case=name,
                           sha256=hashlib.sha256(original).hexdigest()))
        return dest

    def llvm(name, source):
        dest = out/f'{name}.ll'
        run([args.clang, '-x', 'c++', '-O0', '-fno-exceptions', '-fno-rtti',
             '-S', '-emit-llvm', source, '-o', dest])
        return dest

    def compile_ref(name, source):
        obj = out/f'{name}.reference.o'
        run([args.clang, '-O0', '-c', source, '-o', obj])
        return obj

    def compile_keel(name, text, tier):
        obj = out/f'{name}.{tier}.o'
        run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
        return obj

    def executable(name, objects, driver):
        exe = out/name
        run([args.clang, driver, *objects, '-o', exe])
        return run([exe]).stdout

    try:
        sources = {name: llvm(name, FIX/f'{name}.cpp') for name in
                   ['left', 'right', 'identity_left', 'identity_right', 'storage', 'storage_right']}
        sources['constexpr'] = llvm('constexpr', ROOT/'suite/cpp_coverage_fixtures/constexpr_runtime.cpp')
        sources['virtual'] = llvm('virtual', ROOT/'suite/cpp_coverage_fixtures/virtual.cpp')
        # These are actual unmodified compiler outputs, including vtables and
        # constructor/function address uses that must not be internalized.
        for name in ['identity_left', 'identity_right', 'storage', 'storage_right', 'virtual']:
            reject(name, sources[name])
        reject('interposition', FIX/'interposition.ll')
        for name in ['left', 'right', 'constexpr']:
            accept(name + '-default-odr', sources[name], opt=False)
        texts = {name: accept(name, sources[name]) for name in ['left', 'right', 'constexpr']}
        texts['available'] = accept('available', FIX/'available.ll')
        reject('available-default-strict', FIX/'available.ll', opt=False)

        bodies = {
            'weak-any': 'define weak i64 @h(i64 %x) { ret i64 %x }',
            'weak-odr-hidden': 'define weak_odr hidden i64 @h(i64 %x) { ret i64 %x }',
            'linkonce-any': 'define linkonce i64 @h(i64 %x) { ret i64 %x }',
            'external-weak': 'declare extern_weak i64 @h(i64)',
            'address-return': 'define linkonce_odr i64 @h(i64 %x) { ret i64 %x }\ndefine ptr @escape() { ret ptr @h }',
            'address-global': '@p = global ptr @h\ndefine linkonce_odr i64 @h(i64 %x) { ret i64 %x }',
            'address-compare': 'define linkonce_odr i64 @h(i64 %x) { ret i64 %x }\ndefine i1 @cmp(ptr %p) { %r = icmp eq ptr %p, @h\nret i1 %r }',
            'callee-also-argument': 'define linkonce_odr void @h(ptr %p) { ret void }\ndefine void @f() { call void @h(ptr @h)\nret void }',
            'constant-expression-escape': 'define linkonce_odr i64 @h(i64 %x) { ret i64 %x }\ndefine i64 @f() { ret i64 ptrtoint(ptr @h to i64) }',
            'weak-data': '@g = linkonce_odr global i64 3\ndefine linkonce_odr i64 @h() { %v = load i64, ptr @g\nret i64 %v }',
            'private-data': '@g = private global i64 3\ndefine linkonce_odr i64 @h() { %v = load i64, ptr @g\nret i64 %v }',
            'transitive-data': '@g = internal global i64 3\ndefine internal i64 @local() { %v = load i64, ptr @g\nret i64 %v }\ndefine linkonce_odr i64 @h() { %v = call i64 @local()\nret i64 %v }',
            'constant-gep-storage': '@g = private global [2 x i64] zeroinitializer\ndefine linkonce_odr ptr @h() { ret ptr getelementptr ([2 x i64], ptr @g, i64 0, i64 1) }',
            'external-call': 'declare i64 @external(i64)\ndefine linkonce_odr i64 @h(i64 %x) { %v = call i64 @external(i64 %x)\nret i64 %v }',
            'indirect-call': 'define linkonce_odr i64 @h(ptr %p) { %v = call i64 %p()\nret i64 %v }',
            'hidden': 'define linkonce_odr hidden i64 @h(i64 %x) { ret i64 %x }',
            'late-semantics': 'define linkonce_odr i64 @h(i64 %x) { %v = freeze i128 poison\nret i64 %x }',
            'malformed': 'define linkonce_odr i64 @h( {',
        }
        for selection in ['any', 'exactmatch', 'largest', 'noduplicates', 'samesize']:
            bodies['comdat-' + selection] = f'$g = comdat {selection}\ndefine linkonce_odr i64 @h(i64 %x) comdat($g) {{ ret i64 %x }}'
        bodies['comdat-group'] = '$g = comdat any\n@g = linkonce_odr global i64 0, comdat($g)\ndefine linkonce_odr i64 @h(i64 %x) comdat($g) { ret i64 %x }'
        for name, body in bodies.items():
            source = out/f'reject-{name}.ll'
            source.write_text(HEADER + body + '\n')
            reject(name, source)

        refs = {name: compile_ref(name, source) for name, source in sources.items()
                if name in ['left', 'right', 'constexpr', 'identity_left', 'identity_right', 'storage', 'storage_right']}
        driver = out/'driver.o'
        run([args.clang, '-O0', '-c', FIX/'driver.c', '-o', driver])
        expected = executable('reference', [refs['left'], refs['right']], driver)
        # LLVM/Clang really coalesce the address-observable copies. Both inputs
        # are rejected above, so the importer never creates unequal local copies.
        identity = out/'identity-driver.o'
        run([args.clang, '-O0', '-c', FIX/'identity_driver.c', '-o', identity])
        executable('identity-reference', [refs['identity_left'], refs['identity_right']], identity)
        checks.append(dict(kind='original-two-TU-address-equality'))
        storage_driver = out/'storage-driver.o'
        run([args.clang, '-O0', '-c', FIX/'storage_driver.c', '-o', storage_driver])
        executable('storage-reference', [refs['storage'], refs['storage_right']], storage_driver)
        checks.append(dict(kind='original-two-TU-shared-static-storage'))
        interposition_driver = out/'interposition-driver.o'
        run([args.clang, '-O0', '-c', FIX/'interposition_driver.c', '-o', interposition_driver])
        executable('interposition-reference', [compile_ref('interposition', FIX/'interposition.ll')], interposition_driver)
        checks.append(dict(kind='original-weak-strong-interposition'))

        constexpr_driver = out/'constexpr-driver.c'
        constexpr_driver.write_text('#include <assert.h>\n#include <stdio.h>\ntypedef unsigned long long U;\nU cpp_case(U,U,volatile U*);\nint main(void) { U xs[]={0,1,17,~0ULL}; for(unsigned i=0;i<4;i++) { U x=xs[i]; assert(cpp_case(x,13,0)==((x*32)^13)); printf("%llu\\n",cpp_case(x,13,0)); }}\n')
        constdriver = out/'constexpr-driver.o'
        run([args.clang, '-O0', '-c', constexpr_driver, '-o', constdriver])
        constexpr_expected = executable('constexpr.reference', [refs['constexpr']], constdriver)
        available_ref = compile_ref('available', FIX/'available.ll')
        provider = compile_ref('provider', FIX/'available_provider.c')
        avdriver = out/'available-driver.o'
        run([args.clang, '-O0', '-c', FIX/'available_driver.c', '-o', avdriver])
        available_expected = executable('available.reference', [available_ref, provider], avdriver)

        for tier in ['none', 'fast', 'full']:
            objects = {name: compile_keel(name, text, tier) for name, text in texts.items()}
            for label, pair in [('two-local', [objects['left'], objects['right']]),
                                ('mixed-left', [objects['left'], refs['right']]),
                                ('mixed-right', [refs['left'], objects['right']])]:
                assert executable(f'{label}.{tier}', pair, driver) == expected
                checks.append(dict(kind='native-independent-assertions', case=label, tier=tier))
            assert executable(f'constexpr.{tier}', [objects['constexpr']], constdriver) == constexpr_expected
            assert executable(f'available.{tier}', [objects['available'], provider], avdriver) == available_expected
            checks.append(dict(kind='native-independent-assertions', case='constexpr+available', tier=tier))
            # The contract deliberately supplies no externally callable helper.
            # No public ODR symbol may have been silently strengthened.
            for name in ['left', 'right', 'constexpr', 'available']:
                exported = run(['nm', '-g', objects[name]]).stdout
                for symbol in re.findall(rb'^define (?:linkonce_odr|available_externally)\b[^@\n]*@([\w.$]+)\(',
                                         (FIX/'available.ll').read_bytes() if name == 'available' else sources[name].read_bytes(), re.M):
                    assert b'_' + symbol not in exported, (name, tier, exported)
            missing = out/'outside.c'
            missing.write_text('extern unsigned long long _Z14linkage_factorj(unsigned);\nint main(void) { return _Z14linkage_factorj(3)!=8; }\n')
            failed = run([args.clang, '-O0', missing, objects['left'], objects['right'], '-o', out/f'outside.{tier}'], ok=False)
            assert failed.returncode != 0 and b'linkage_factor' in failed.stderr and b'Undefined symbols' in failed.stderr, failed
            checks.append(dict(kind='outside-reference-requires-real-provider', tier=tier))

        # API/CLI output protocol: stdout only after success, stdin is byte-exact,
        # downstream verification failure cannot replace an existing destination.
        original = sources['left'].read_bytes()
        stdout = imp('-', '-', data=original)
        assert stdout.returncode == 0 and stdout.stdout == texts['left'].read_bytes(), stdout.stderr
        dest = out/'verify-preserved.keel'
        dest.write_bytes(b'sentinel\n')
        failure = imp(sources['left'], dest, keel='/usr/bin/false')
        assert failure.returncode == 1 and not failure.stdout and b'Keel verification failed' in failure.stderr
        assert dest.read_bytes() == b'sentinel\n' and sources['left'].read_bytes() == original
        checks.append(dict(kind='stdin-stdout-downstream-verifier-protocol'))
        print(json.dumps(dict(checks=len(checks),
                             rejections=sum(c['kind'] == 'rejection-preservation' for c in checks),
                             native_tiers=['none', 'fast', 'full'])))
    finally:
        (out/'commands.json').write_text(json.dumps(commands, indent=2))
        (out/'checks.json').write_text(json.dumps(checks, indent=2))


if __name__ == '__main__':
    main()
