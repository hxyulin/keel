#!/usr/bin/env python3
"""Scalar floating-point import.

Compiler output is consumed unchanged. FP-to-int comparisons use only inputs
LLVM defines (the truncated value is in range). Arithmetic NaNs compare by
classification; bitcasts, constants and fneg compare bits. Fast-math flags,
strict FP, unsupported types and FP attributes are rejected without output.
"""
from harness import ROOT, keel_functions, llvm_functions, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_fp_fixtures'


def rejections():
    cases = {
        'frem': 'define double @f(double %x) { %r = frem double %x, 2.0\nret double %r }',
        'half': 'define half @f(half %x) { ret half %x }',
        'bfloat': 'define bfloat @f(bfloat %x) { ret bfloat %x }',
        'x87': 'define x86_fp80 @f(x86_fp80 %x) { ret x86_fp80 %x }',
        'fp128': 'define fp128 @f(fp128 %x) { ret fp128 %x }',
        'vector': 'define <2 x float> @f(<2 x float> %x) { ret <2 x float> %x }',
        'vector-layout': 'define void @f() { %p = alloca <2 x float>\nret void }',
        'strict': 'define double @f(double %x) strictfp { ret double %x }',
        'strict-decl': 'declare double @f(double) strictfp',
        'strict-call': 'declare double @g(double)\ndefine double @f(double %x) { %v = call double @g(double %x) strictfp\nret double %v }',
        'constrained': 'declare double @llvm.experimental.constrained.fadd.f64(double, double, metadata, metadata)\ndefine double @f(double %x) { %v = call double @llvm.experimental.constrained.fadd.f64(double %x, double %x, metadata !"round.downward", metadata !"fpexcept.strict")\nret double %v }',
        'nneg': 'define double @f(i32 %x) { %v = uitofp nneg i32 %x to double\nret double %v }',
        'indirect-nofpclass': 'define double @f(ptr %p, double %x) { %v = call nofpclass(nan) double %p(double %x)\nret double %v }',
        'half-global': '@g = global half 1.0\ndefine double @f() { ret double 1.0 }',
        'nofpclass-return': 'define nofpclass(nan) double @f(double %x) { ret double %x }',
        'nofpclass-param': 'declare double @f(double nofpclass(inf))',
        'nofpclass-call': 'declare double @g(double)\ndefine double @f(double %x) { %v = call nofpclass(nan) double @g(double %x)\nret double %v }',
        'nofpclass-call-param': 'declare double @g(double)\ndefine double @f(double %x) { %v = call double @g(double nofpclass(nan) %x)\nret double %v }',
        'late-failure': 'define double @good() { ret double 1.0 }\ndefine double @bad() { %p = fadd fast double 1.0, 2.0\nret double %p }',
        # Unsupported ABI contracts stay rejected.
        'plain-i8-return': 'define i8 @f(i8 %x) { ret i8 %x }',
        'extension-mismatch': 'declare signext i8 @g(i8)\ndefine signext i8 @f(i8 %x) { %v = call i8 @g(i8 %x)\nret i8 %v }',
        'byval': 'declare double @g(ptr byval(double))',
        'sret': 'declare void @g(ptr sret(double))',
    }
    for flag in ['fast', 'nnan', 'ninf', 'nsz', 'arcp', 'contract', 'afn', 'reassoc']:
        cases['flag-' + flag] = f'define double @f(double %x) {{ %v = fadd {flag} double %x, 1.0\nret double %v }}'
    for op,body in [('fcmp', '%v = fcmp nnan olt double %x, 1.0\n%z = zext i1 %v to i32\nret i32 %z'),
                    ('fneg', '%v = fneg nsz double %x\nret double %v'),
                    ('select', '%v = select nnan i1 true, double %x, double 0.0\nret double %v'),
                    ('call', '%v = call nnan double @g(double %x)\nret double %v')]:
        ret = 'i32' if op == 'fcmp' else 'double'
        cases['flag-' + op] = 'declare double @g(double)\n' + f'define {ret} @f(double %x) {{ {body} }}'
    for width in [1, 8, 16]:
        for op in ['fptosi', 'fptoui']:
            cases[f'{op}-i{width}'] = f'define void @f(double %x) {{ %v = {op} double %x to i{width}\nret void }}'
    cases['flag-fptrunc'] = 'define float @f(double %x) { %v = fptrunc nnan double %x to float\nret float %v }'
    for attr in ['denormal-fp-math', 'denormal-fp-math-f32', 'rounding-mode',
                 'unsafe-fp-math', 'no-infs-fp-math', 'no-nans-fp-math',
                 'no-signed-zeros-fp-math', 'approx-func-fp-math']:
        cases['attribute-' + attr] = f'define double @f(double %x) "{attr}"="true" {{ ret double %x }}'
    return cases


def validate(ctx):
    run, out = ctx.run, ctx.out
    ctx.reject_all(rejections())
    compiler = out / 'compiler.ll'
    run.require([ctx.clang, '-O1', '-ffp-contract=off', '-S', '-emit-llvm', FIXTURES / 'compiler.c', '-o', compiler])
    compiler_o0 = out / 'compiler-o0.ll'
    run.require([ctx.clang, '-O0', '-ffp-contract=off', '-S', '-emit-llvm', FIXTURES / 'compiler.c',
                 '-o', compiler_o0])
    for name, source, driver in [('scalar', FIXTURES / 'scalar.ll', FIXTURES / 'driver.c'),
                                 ('compiler', compiler, FIXTURES / 'compiler_driver.c'),
                                 ('compiler-o0', compiler_o0, FIXTURES / 'compiler_driver.c')]:
        text = out / f'{name}.keel'
        imported = ctx.importer.accept(source, text)
        # Every definition and declaration survives, including unused helpers.
        require(llvm_functions(source.read_bytes()) == keel_functions(imported), (name, 'function inventory'))
        if name == 'scalar':
            for constant in ['0x7ff0000000000001', '0x8000000000000000', '0x0000000000000001',
                             '0x7f800001', '0x00000001']:
                require(constant in imported, f'scalar: constant {constant} lost')
        # Compile the same immutable LLVM input, not a fresh C compilation.
        run.require([ctx.clang, '-O0', '-c', source, '-o', out / f'{name}.reference.o'])
        run.require([ctx.clang, '-O0', '-ffp-contract=off', '-c', driver, '-o', out / f'{name}.driver.o'])
        expected = ctx.link_and_run(ctx.clang, [out / f'{name}.driver.o', out / f'{name}.reference.o'],
                                    out / f'{name}.reference')
        ctx.tiers(name, text, [out / f'{name}.driver.o'], expected)
    ctx.verifier_failures(FIXTURES / 'scalar.ll')


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__))
