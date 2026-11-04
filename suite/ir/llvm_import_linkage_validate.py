#!/usr/bin/env python3
"""Bounded ODR local copies with --localize-odr-functions.

The caller asserts that no outside translation unit references the candidate
symbols. Compiler-produced LLVM is imported and reference-compiled without
byte edits. Inputs whose ODR functions are address-observable, share storage
or can be interposed are rejected, and native two-TU links check that the
originals really depend on those properties.
"""
import re

from harness import ROOT, TIERS, keel_functions, llvm_functions, main, require

FIX = ROOT / 'suite/ir/llvm_import_linkage_fixtures'
CANDIDATES = rb'^define (?:linkonce_odr|available_externally)\b[^@\n]*@([\w.$]+)\('


def rejections():
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
    return bodies


def validate(ctx):
    run, out, clang = ctx.run, ctx.out, ctx.clang

    def reject(name, source, localize=True):
        diagnostic = ctx.importer.reject(source, localize=localize)
        ctx.check(kind='rejection', case=name, diagnostic=diagnostic.strip())

    def accept(name, source, localize=True):
        dest = out / f'{name}.keel'
        text = ctx.importer.accept(source, dest, localize=localize)
        original = source.read_bytes()
        require(llvm_functions(original) == keel_functions(text), (name, 'function inventory'))
        for symbol in re.findall(CANDIDATES, original, re.M):
            line = next(line for line in text.splitlines()
                        if line.startswith('func @' + symbol.decode() + '('))
            require(' cc(c)' in line and line.endswith(' local {' if localize else ' weak_odr {'), line)
        ctx.check(kind='retained-all-functions-local-copies', case=name)
        return dest

    def llvm(name, source):
        dest = out / f'{name}.ll'
        run.require([clang, '-x', 'c++', '-O0', '-fno-exceptions', '-fno-rtti',
                     '-S', '-emit-llvm', source, '-o', dest])
        return dest

    def compile_ref(name, source):
        obj = out / f'{name}.reference.o'
        run.require([clang, '-O0', '-c', source, '-o', obj])
        return obj

    def driver(name, source):
        obj = out / f'{name}.o'
        run.require([clang, '-O0', '-c', source, '-o', obj])
        return obj

    def executable(name, objects, main_object):
        return ctx.link_and_run(clang, [main_object, *objects], out / name)

    sources = {name: llvm(name, FIX / f'{name}.cpp') for name in
               ['left', 'right', 'identity_left', 'identity_right', 'storage', 'storage_right']}
    sources['constexpr'] = llvm('constexpr', ROOT / 'suite/ir/cpp_coverage_fixtures/constexpr_runtime.cpp')
    sources['virtual'] = llvm('virtual', ROOT / 'suite/ir/cpp_coverage_fixtures/virtual.cpp')
    # These are actual unmodified compiler outputs, including vtables and
    # constructor/function address uses that must not be internalized.
    for name in ['identity_left', 'identity_right', 'storage', 'storage_right', 'virtual']:
        reject(name, sources[name])
    reject('interposition', FIX / 'interposition.ll')
    for name in ['left', 'right', 'constexpr']:
        accept(name + '-default-odr', sources[name], localize=False)
    texts = {name: accept(name, sources[name]) for name in ['left', 'right', 'constexpr']}
    texts['available'] = accept('available', FIX / 'available.ll')
    reject('available-default-strict', FIX / 'available.ll', localize=False)
    header = (FIX / 'available.ll').read_text().split('define', 1)[0]
    ctx.reject_all(rejections(), header, localize=True)

    refs = {name: compile_ref(name, source) for name, source in sources.items() if name != 'virtual'}
    main_driver = driver('driver', FIX / 'driver.c')
    expected = executable('reference', [refs['left'], refs['right']], main_driver)
    # LLVM and Clang really coalesce the address-observable copies. Both inputs
    # are rejected above, so the importer never creates unequal local copies.
    executable('identity-reference', [refs['identity_left'], refs['identity_right']],
               driver('identity-driver', FIX / 'identity_driver.c'))
    ctx.check(kind='original-two-TU-address-equality')
    executable('storage-reference', [refs['storage'], refs['storage_right']],
               driver('storage-driver', FIX / 'storage_driver.c'))
    ctx.check(kind='original-two-TU-shared-static-storage')
    executable('interposition-reference', [compile_ref('interposition', FIX / 'interposition.ll')],
               driver('interposition-driver', FIX / 'interposition_driver.c'))
    ctx.check(kind='original-weak-strong-interposition')

    constexpr_driver = out / 'constexpr-driver.c'
    constexpr_driver.write_text('#include <assert.h>\n#include <stdio.h>\ntypedef unsigned long long U;\nU cpp_case(U,U,volatile U*);\nint main(void) { U xs[]={0,1,17,~0ULL}; for(unsigned i=0;i<4;i++) { U x=xs[i]; assert(cpp_case(x,13,0)==((x*32)^13)); printf("%llu\\n",cpp_case(x,13,0)); }}\n')
    constdriver = driver('constexpr-driver', constexpr_driver)
    constexpr_expected = executable('constexpr.reference', [refs['constexpr']], constdriver)
    available_ref = compile_ref('available', FIX / 'available.ll')
    provider = compile_ref('provider', FIX / 'available_provider.c')
    avdriver = driver('available-driver', FIX / 'available_driver.c')
    available_expected = executable('available.reference', [available_ref, provider], avdriver)
    outside = out / 'outside.c'
    outside.write_text('extern unsigned long long _Z14linkage_factorj(unsigned);\n'
                       'int main(void) { return _Z14linkage_factorj(3)!=8; }\n')

    for tier in TIERS:
        objects = {name: ctx.compile(text, out / f'{name}.{tier}.o', tier) for name, text in texts.items()}
        for label, pair in [('two-local', [objects['left'], objects['right']]),
                            ('mixed-left', [objects['left'], refs['right']]),
                            ('mixed-right', [refs['left'], objects['right']])]:
            require(executable(f'{label}.{tier}', pair, main_driver) == expected, (label, tier))
            ctx.check(kind='native', case=label, tier=tier)
        require(executable(f'constexpr.{tier}', [objects['constexpr']], constdriver) == constexpr_expected,
                ('constexpr', tier))
        require(executable(f'available.{tier}', [objects['available'], provider], avdriver)
                == available_expected, ('available', tier))
        ctx.check(kind='native', case='constexpr+available', tier=tier)
        # The contract deliberately supplies no externally callable helper.
        # No public ODR symbol may have been silently strengthened.
        for name in ['left', 'right', 'constexpr', 'available']:
            exported = run.require(['nm', '-g', objects[name]]).stdout
            original = (FIX / 'available.ll' if name == 'available' else sources[name]).read_bytes()
            for symbol in re.findall(CANDIDATES, original, re.M):
                require(b'_' + symbol not in exported, (name, tier, exported))
        failed = run.run([clang, '-O0', outside, objects['left'], objects['right'], '-o', out / f'outside.{tier}'])
        require(failed.returncode != 0 and b'linkage_factor' in failed.stderr
                and b'Undefined symbols' in failed.stderr, failed.stderr)
        ctx.check(kind='outside-reference-requires-real-provider', tier=tier)
    ctx.verifier_failures(sources['left'])


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__, timeout=180))
