#!/usr/bin/env python3
"""Exported ODR and RTTI symbols, never localized.

Clang++ output for two translation units that share weak_odr and linkonce_odr
functions, data and RTTI is imported unchanged. Each Keel object replaces its
Clang counterpart in both link orders, and the constexpr, virtual and EH C++
coverage cases run with their own drivers. Unsupported linkage, visibility,
COMDAT and TLS forms are rejected without output.
"""
import re

from harness import ROOT, TIERS, keel_functions, llvm_functions, main, require

FIX = ROOT / 'suite/ir/llvm_import_odr_fixtures'
ORIGINALS = ('constexpr_runtime', 'virtual', 'eh')
REJECTIONS = {
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


def validate(ctx):
    run, out, clang = ctx.run, ctx.out, ctx.clangxx
    run.require([clang, '--version'])
    flags = ['-std=c++17', '-O1', '-fno-inline', '-fwrapv', '-ffp-contract=off',
             '-Xclang', '-disable-lifetime-markers']
    paths = dict(left=FIX / 'left.cpp', right=FIX / 'right.cpp')
    paths.update({name: ROOT / f'suite/ir/cpp_coverage_fixtures/{name}.cpp' for name in ORIGINALS})
    sources, refs, drivers, modules = {}, {}, {}, {}
    for name, path in paths.items():
        ll = out / f'{name}.ll'
        run.require([clang, *flags, '-S', '-emit-llvm', path, '-o', ll])
        sources[name] = ll
        text = ll.read_text()
        # Keep an exact inventory, including dead definitions and declarations.
        functions = llvm_functions(text)
        require(functions, f'empty module: {name}')
        modules[name] = dict(
            functions=functions,
            odr_lines=[line for line in text.splitlines()
                       if re.search(r'\b(?:weak_odr|linkonce_odr)\b', line)],
            rtti_lines=[line for line in text.splitlines()
                        if line.startswith('@_ZTI') or line.startswith('@_ZTV')])
        refs[name] = out / f'{name}.reference.o'
        # The very same, untouched LLVM module is the reference compiler input.
        run.require([clang, '-O1', '-c', ll, '-o', refs[name]])
    for name in ('left', 'right'):
        module = modules[name]
        require(any('define weak_odr ' in line for line in module['odr_lines']), module)
        require(any('define linkonce_odr ' in line for line in module['odr_lines']), module)
        require(any(line.startswith('@function_slot = linkonce_odr') and 'ptr @_Z13inline_helperi' in line
                    for line in module['odr_lines']), module)
        require(any(line.startswith('@_ZTI7Derived =') for line in module['rtti_lines']), module)

    def execute(name, objects, expected):
        require(ctx.link_and_run(clang, objects, out / name) == expected, name)
        ctx.check(kind='native', executable=name)

    drivers['mixed'] = out / 'mixed.driver.o'
    run.require([clang, *flags, '-c', FIX / 'driver.cpp', '-o', drivers['mixed']])
    expected = dict(mixed=b'ODR: identity, storage, dispatch; 27 EH paths independently asserted\n')
    for order in (('left', 'right'), ('right', 'left')):
        execute(f'mixed.reference.{order[0]}-first', [refs[n] for n in order] + [drivers['mixed']],
                expected['mixed'])
    for index, name in enumerate(ORIGINALS):
        drivers[name] = out / f'{name}.driver.o'
        run.require([clang, *flags, f'-DCASE={index}', '-c', FIX / 'original_driver.cpp', '-o', drivers[name]])
        expected[name] = b'original: 6 independently asserted inputs\n'
        execute(f'{name}.reference', [drivers[name], refs[name]], expected[name])

    texts = {}
    for name, source in sources.items():
        texts[name] = out / f'{name}.keel'
        actual = keel_functions(ctx.importer.accept(source, texts[name]))
        original = modules[name]['functions']
        require([n for n in actual if n in original] == original,
                (name, 'lost, reordered or duplicated input function', actual))
        require(set(actual) - set(original) <= {'_Unwind_Resume'},
                (name, 'unexpected synthesized function', actual))
    for tier in TIERS:
        objects = {name: ctx.compile(text, out / f'{name}.{tier}.o', tier) for name, text in texts.items()}
        for replacement in ('left', 'right', 'all'):
            for order in (('left', 'right'), ('right', 'left')):
                selected = [objects[n] if replacement in (n, 'all') else refs[n] for n in order]
                execute(f'mixed.{replacement}.{tier}.{order[0]}-first', [*selected, drivers['mixed']],
                        expected['mixed'])
        for name in ORIGINALS:
            execute(f'{name}.{tier}', [drivers[name], objects[name]], expected[name])
    header = ''.join(line + '\n' for line in sources['left'].read_text().splitlines()
                     if line.startswith(('target datalayout', 'target triple')))
    ctx.reject_all(REJECTIONS, header + 'define i32 @good() { ret i32 7 }\n')
    ctx.verifier_failures(sources['left'])
    return dict(modules=modules)


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__, timeout=180))
