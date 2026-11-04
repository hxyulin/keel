#!/usr/bin/env python3
"""LLVM import guards and native equivalence for scalar, boolean and CFG code.

Imports Clang++ and rustc scalar code, a handwritten i1 module and handwritten
plus compiler-emitted CFG/PHI modules, and compares each at every Keel tier
with the same LLVM IR compiled natively. Also checks that unsupported modules,
targets and data layouts are rejected without output.
"""
from harness import HEADER, ROOT, main

FIXTURES = ROOT / 'suite/ir/llvm_import_fixtures'

REJECTIONS = {
    'dynamic-alloca': 'define void @f(i64 %n) { %p = alloca i32, i64 %n\nret void }',
    'loop-alloca': 'define void @f() { br label %b\nb: %p = alloca i32\nret void }',
    'zero-alloca': 'define void @f() { %p = alloca i32, i64 0\nret void }',
    'large-alloca': 'define void @f() { %p = alloca i64, i64 536870912\nret void }',
    'aligned-alloca': 'define void @f() { %p = alloca i32, align 32\nret void }',
    'opaque-alloca': '%S = type opaque\ndefine void @f() { %p = alloca %S\nret void }',
    'half-alloca': 'define void @f() { %p = alloca half\nret void }',
    'address-space': 'define ptr addrspace(1) @f(ptr addrspace(1) %p) { ret ptr addrspace(1) %p }',
    'gep-wide-index': 'define ptr @f(ptr %p, i128 %i) { %q = getelementptr i64, ptr %p, i128 %i\nret ptr %q }',
    'gep-i1': 'define ptr @f(ptr %p) { %q = getelementptr i8, ptr %p, i1 true\nret ptr %q }',
    'bad-phi': 'define i32 @f(i32 %x) { br label %b\nb: %p = phi i32 [ %x, %b ]\nret i32 %p }',
    'duplicate-phi': 'define i32 @f(i32 %x) { %c = icmp eq i32 %x, 0\nbr i1 %c, label %b, label %b\nb: %p = phi i32 [ 1, %0 ], [ 2, %0 ]\nret i32 %p }',
    'late-failure': 'define i32 @good() { ret i32 7 }\ndefine i128 @bad() { ret i128 0 }',
    'weak-global': '@g = weak global i32 0\ndefine void @f() { ret void }',
    'atomic': 'define i32 @f(ptr %p) { %v = load atomic i32, ptr %p monotonic, align 4\nret i32 %v }',
    'abi-byval': 'define void @f(ptr byval(i32) %p) { ret void }',
    'abi-sret': 'define void @f(ptr sret(i32) %p) { ret void }',
    'variadic': 'declare i32 @f(i32, ...)',
    'bool-abi': 'define i1 @f() { ret i1 true }',
    'cc': 'define fastcc i32 @f() { ret i32 1 }',
    'bundle': 'declare void @g()\ndefine void @f() { call void @g() [ "deopt"(i32 1) ]\nret void }',
    'bad-call-type': 'declare i32 @g(i32)\ndefine i64 @f(i64 %x) { %v = call i64 @g(i64 %x)\nret i64 %v }',
    'bad-ir': 'this is not LLVM IR',
    'sme-definition': 'define void @f() "aarch64_pstate_sm_enabled" { ret void }',
    'sme-declaration': 'declare void @f() "aarch64_inout_za"',
    'sme-call': 'declare void @g()\ndefine void @f() { call void @g() "aarch64_pstate_sm_compatible"\nret void }',
    'future-arm-abi': 'define void @f() "aarch64_future_abi" { ret void }',
    'call-returns-twice': 'declare i32 @g(ptr)\ndefine i32 @f(ptr %p) { %v = call i32 @g(ptr %p) returns_twice\nret i32 %v }',
    'null-valid': 'define void @f() null_pointer_is_valid { ret void }',
    'instrumentation': 'define void @f() "instrument-function-entry"="hook" { ret void }',
    'patchable': 'define void @f() "patchable-function-entry"="4" { ret void }',
    'autolink': 'define void @f() { ret void }\n!llvm.linker.options = !{!0}\n!0 = !{!"-lmissing"}',
    'comdat': '$f = comdat any\ndefine i32 @f(i32 %x) comdat { ret i32 %x }',
    'plain-i8-return': 'define i8 @f(i8 %x) { ret i8 %x }',
    'plain-i16-return': 'define i16 @f(i16 %x) { ret i16 %x }',
    'noext-i8-return': 'define noext i8 @f(i8 %x) { ret i8 %x }',
    'noext-i16-return': 'define noext i16 @f(i16 %x) { ret i16 %x }',
}
HEADERS = {
    'wrong-target': HEADER.replace('arm64-apple-macosx11.0.0', 'x86_64-unknown-linux-gnu'),
    'big-endian': HEADER.replace('e-m:o', 'E-m:o'),
    'pointer-index32': HEADER.replace('e-m:o', 'e-m:o-p:64:64:64:32'),
    'missing-layout': 'target triple = "arm64-apple-macosx11.0.0"\n',
    'missing-target': HEADER.split('target triple')[0],
}


def validate(ctx):
    run, out, fixtures = ctx.run, ctx.out, FIXTURES
    cases = dict(REJECTIONS)
    cases.update({name: 'define i32 @f() { ret i32 1 }' for name in HEADERS})
    for name, body in cases.items():
        source = ctx.module(name, body, HEADERS.get(name, HEADER))
        ctx.check(kind='rejection', case=name, diagnostic=ctx.importer.reject(source).strip())

    rust_flags = ['--edition', '2024', '--crate-type', 'lib', '-C', 'panic=abort']
    run.require([ctx.clangxx, '-O2', '-fno-exceptions', '-fno-rtti', '-S', '-emit-llvm',
                 fixtures / 'scalar.cpp', '-o', out / 'cpp.ll'])
    run.require([ctx.clangxx, '-O2', '-fno-exceptions', '-fno-rtti', '-c',
                 fixtures / 'scalar.cpp', '-o', out / 'cpp.o'])
    run.require(['rustc', *rust_flags, '-C', 'opt-level=2', '-C', 'overflow-checks=off',
                 '--emit', f'llvm-ir={out}/rust.ll,obj={out}/rust.o', fixtures / 'scalar.rs'])
    run.require([ctx.clang, '-O2', '-c', fixtures / 'driver.c', '-o', out / 'driver.o'])
    for language in ('cpp', 'rust'):
        text = out / f'{language}.keel'
        ctx.importer.accept(out / f'{language}.ll', text)
        expected = ctx.link_and_run(ctx.clang, [out / 'driver.o', out / f'{language}.o'],
                                    out / f'{language}.reference')
        ctx.tiers(language, text, [out / 'driver.o'], expected, pairs=20289, exports=8)

    text = out / 'boolean.keel'
    ctx.importer.accept(fixtures / 'boolean.ll', text)
    run.require([ctx.clang, '-O2', '-c', fixtures / 'boolean_driver.c', '-o', out / 'boolean-driver.o'])
    run.require([ctx.clang, '-c', fixtures / 'boolean.ll', '-o', out / 'boolean-reference.o'])
    expected = ctx.link_and_run(ctx.clang, [out / 'boolean-driver.o', out / 'boolean-reference.o'],
                                out / 'boolean.reference')
    ctx.tiers('boolean', text, [out / 'boolean-driver.o'], expected, inputs=65536, exports=3)

    # Handwritten hard CFG/PHI cases plus untouched compiler-emitted CFG/GEP.
    run.require([ctx.clang, '-O1', '-S', '-emit-llvm', fixtures / 'cfg.c', '-o', out / 'cfg-clang.ll'])
    run.require([ctx.clang, '-O1', '-c', fixtures / 'cfg.c', '-o', out / 'cfg-clang.reference.o'])
    run.require(['rustc', *rust_flags, '-C', 'opt-level=1',
                 '--emit', f'llvm-ir={out}/cfg-rust.ll,obj={out}/cfg-rust.reference.o', fixtures / 'cfg.rs'])
    run.require([ctx.clang, '-c', fixtures / 'cfg_memory.ll', '-o', out / 'cfg-memory.reference.o'])
    for name, source, driver, inputs in [
        ('cfg-memory', fixtures / 'cfg_memory.ll', 'cfg_memory_driver.c', 4096),
        ('cfg-clang', out / 'cfg-clang.ll', 'cfg_compiler_driver.c', 8192),
        ('cfg-rust', out / 'cfg-rust.ll', 'cfg_compiler_driver.c', 8192),
    ]:
        text = out / f'{name}.keel'
        ctx.importer.accept(source, text)
        run.require([ctx.clang, '-O2', '-c', fixtures / driver, '-o', out / f'{name}.driver.o'])
        expected = ctx.link_and_run(ctx.clang, [out / f'{name}.driver.o', out / f'{name}.reference.o'],
                                    out / f'{name}.reference')
        ctx.tiers(name, text, [out / f'{name}.driver.o'], expected, inputs=inputs)
    ctx.verifier_failures(fixtures / 'cfg_memory.ll')


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__, timeout=90))
