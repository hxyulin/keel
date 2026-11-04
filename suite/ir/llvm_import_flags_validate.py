#!/usr/bin/env python3
"""Integer no-wrap, exact, disjoint, samesign and nneg flags.

A generated matrix wraps narrow flagged operations in i64 functions whose
expected results come from the Python oracles below; poison results select a
marker value instead. Clang output and a handwritten poison-dividend module are
imported unchanged.
"""
import re

from harness import HEADER, ROOT, TIERS, keel_functions, llvm_functions, main, require

FIXTURES = ROOT / 'suite/ir/llvm_import_flags_fixtures'
REJECTIONS = {
    'i1-arithmetic': 'define void @f() { %v = add nsw i1 true, false\nret void }',
    'i1-ordered': 'define void @f(i64 %x) { %a = trunc i64 %x to i1\n%v = icmp samesign slt i1 %a, true\nret void }',
    'pointer-samesign': 'define void @f(ptr %x) { %v = icmp samesign eq ptr %x, null\nret void }',
    'vector': 'define <2 x i64> @f(<2 x i64> %x) { %v = add nsw <2 x i64> %x, %x\nret <2 x i64> %v }',
    'fp-nneg': 'define double @f(i64 %x) { %v = uitofp nneg i64 %x to double\nret double %v }',
    'unknown-flag': 'define i64 @f(i64 %x) { %v = add imaginary i64 %x, 1\nret i64 %v }',
    'late-rejection': 'define i64 @good(i64 %x) { %r = add nsw i64 %x, 1\nret i64 %r }\ndefine i128 @bad() { ret i128 0 }',
    'unsupported-abi': 'declare void @g(ptr byval(i64))',
    'malformed': 'define i64 @f( {',
}


def signed(x, width):
    return x - (1 << width) if x >> (width - 1) else x


def binary_oracle(op, flags, x, y, w):
    """Return (bits, poison); None means immediate UB, never a test input."""
    mask, sx, sy = (1 << w) - 1, signed(x, w), signed(y, w)
    poison = False
    if op in ('add', 'sub', 'mul'):
        unsigned = {'add': x + y, 'sub': x - y, 'mul': x * y}[op]
        mathematical = {'add': sx + sy, 'sub': sx - sy, 'mul': sx * sy}[op]
        poison = ('nuw' in flags and not 0 <= unsigned <= mask) or (
            'nsw' in flags and not -(1 << (w - 1)) <= mathematical < (1 << (w - 1)))
        result = unsigned
    elif op == 'shl':
        poison = y >= w or ('nuw' in flags and (x << y) > mask) or (
            'nsw' in flags and not -(1 << (w - 1)) <= sx * (1 << y) < (1 << (w - 1)))
        result = x << y
    elif op == 'or':
        result, poison = x | y, bool(x & y)
    elif op in ('lshr', 'ashr'):
        poison = y >= w or bool(x & ((1 << y) - 1))
        result = (x if op == 'lshr' else sx) >> y
    elif op in ('sdiv', 'udiv', 'srem'):
        if not y or (op != 'udiv' and sx == -(1 << (w - 1)) and sy == -1):
            return None
        if op == 'udiv':
            result, remainder = divmod(x, y)
        else:
            quotient = (abs(sx) // abs(sy)) * (-1 if (sx < 0) != (sy < 0) else 1)
            remainder = sx - quotient * sy
            result = remainder if op == 'srem' else quotient
        poison = 'exact' in flags and remainder != 0
    else:
        raise AssertionError(op)
    return result & mask, poison


def matrix():
    """Generate ABI-wide wrappers with narrow internals and Python oracles."""
    definitions, declarations, assertions = [], [], []
    count = 0

    def add(name, body, cases):
        nonlocal count
        definitions.append(f'define i64 @{name}(i64 %x, i64 %y, i64 %take) {{\n{body}\n'
                           '  %cond = icmp ne i64 %take, 0\n'
                           '  %chosen = select i1 %cond, i64 %wide, i64 73\n'
                           '  ret i64 %chosen\n}\n')
        declarations.append(f'extern uint64_t {name}(uint64_t, uint64_t, uint64_t);')
        for x, y, take, expected in cases:
            assertions.append(f'  check({name}(UINT64_C({x}), UINT64_C({y}), {take}), UINT64_C({expected}), {count});')
            count += 1

    for w in (8, 16, 32, 64):
        mask, sign = (1 << w) - 1, 1 << (w - 1)
        values = sorted({0, 1, 2, 3, sign - 1, sign, mask - 1, mask})
        prefix = (f'  %a = trunc i64 %x to i{w}\n  %b = trunc i64 %y to i{w}\n' if w < 64 else '')
        a, b = ('%a', '%b') if w < 64 else ('%x', '%y')
        operations = [(op, flag) for op in ('add', 'sub', 'mul', 'shl') for flag in ('nsw', 'nuw', 'nuw nsw')]
        operations += [('or', 'disjoint'), ('lshr', 'exact'), ('ashr', 'exact'),
                       ('sdiv', 'exact'), ('udiv', 'exact'), ('sdiv', ''), ('srem', '')]
        for op, flags in operations:
            name = f'{op}_{flags.replace(" ", "_") or "plain"}_{w}'
            body = prefix + f'  %result = {op} {flags} i{w} {a}, {b}\n'
            body += f'  %wide = zext i{w} %result to i64' if w < 64 else '  %wide = or i64 %result, 0'
            cases = []
            ys = sorted({0, 1, 2, w - 1, w, w + 1}) if op in ('shl', 'lshr', 'ashr') else values
            for x in values:
                for y in ys:
                    oracle = binary_oracle(op, flags, x, y, w)
                    if oracle is None:
                        continue
                    result, poison = oracle
                    cases.append((x, y, 0 if poison else 1, 73 if poison else result))
            add(name, body, cases)
        # Refined poison must remain nontrapping after downstream operations.
        # The unflagged oversized shift also audits the pre-existing importer.
        for producer in ('add', 'lshr'):
            expression = f'add nsw i{w} {a}, 1' if producer == 'add' else f'lshr i{w} {a}, {w}'
            for consumer in ('sdiv', 'srem'):
                body = prefix + f'  %p = {expression}\n  %result = {consumer} i{w} %p, -1\n'
                body += f'  %wide = zext i{w} %result to i64' if w < 64 else '  %wide = or i64 %result, 0'
                # add nsw wraps MAX+1 to MIN; an oversized lshr on MIN
                # refines to MIN with Keel's masked shift count.
                x = sign - 1 if producer == 'add' else sign
                add(f'poison_{producer}_{consumer}_{w}', body, [(x, 0, 0, 73)])
        for predicate in ('eq', 'ne', 'ugt', 'uge', 'ult', 'ule', 'sgt', 'sge', 'slt', 'sle'):
            body = prefix + f'  %result = icmp samesign {predicate} i{w} {a}, {b}\n  %wide = zext i1 %result to i64'
            cases = []
            for x in values:
                for y in values:
                    xx, yy = (signed(x, w), signed(y, w)) if predicate.startswith('s') else (x, y)
                    relation = predicate.lstrip('us')
                    result = {'eq': xx == yy, 'ne': xx != yy, 'gt': xx > yy,
                              'ge': xx >= yy, 'lt': xx < yy, 'le': xx <= yy}[relation]
                    poison = (x >= sign) != (y >= sign)
                    cases.append((x, y, 0 if poison else 1, 73 if poison else int(result)))
            add(f'cmp_{predicate}_{w}', body, cases)
    for src, dst in ((src, dst) for src in (8, 16, 32, 64) for dst in (1, 8, 16, 32) if src > dst):
        for flags in ('nsw', 'nuw', 'nuw nsw'):
            body = f'  %a = trunc i64 %x to i{src}\n' if src < 64 else ''
            a = '%a' if src < 64 else '%x'
            body += f'  %result = trunc {flags} i{src} {a} to i{dst}\n  %wide = zext i{dst} %result to i64'
            cases = []
            for x in sorted({0, 1, (1 << (dst - 1)) - 1, 1 << (dst - 1), (1 << dst) - 1, 1 << dst, (1 << src) - 1}):
                result = x & ((1 << dst) - 1)
                poison = ('nuw' in flags and x != result) or ('nsw' in flags and signed(x, src) != signed(result, dst))
                cases.append((x, 0, 0 if poison else 1, 73 if poison else result))
            add(f'trunc_{src}_{dst}_{flags.replace(" ", "_")}', body, cases)
    for dst in (1, 8, 16, 32):
        body = f'  %a = trunc i64 %x to i{dst}\n  %wide = zext nneg i{dst} %a to i64'
        cases = [(x, 0, int(x < 1 << (dst - 1)), x if x < 1 << (dst - 1) else 73)
                 for x in sorted({0, 1, (1 << (dst - 1)) - 1, 1 << (dst - 1), (1 << dst) - 1})]
        add(f'zext_nneg_{dst}', body, cases)
    add('bool_or', '  %a = trunc i64 %x to i1\n  %b = trunc i64 %y to i1\n'
        '  %r = or disjoint i1 %a, %b\n  %wide = zext i1 %r to i64',
        [(0, 0, 1, 0), (1, 0, 1, 1), (0, 1, 1, 1), (1, 1, 0, 73)])
    add('bool_cmp', '  %a = trunc i64 %x to i1\n  %b = trunc i64 %y to i1\n'
        '  %r = icmp samesign eq i1 %a, %b\n  %wide = zext i1 %r to i64',
        [(0, 0, 1, 1), (1, 1, 1, 1), (0, 1, 0, 73), (1, 0, 0, 73)])
    driver = '#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n' + '\n'.join(declarations)
    driver += '\nstatic void check(uint64_t actual, uint64_t expected, int id) { if (actual != expected) { fprintf(stderr, "case %d: %llu != %llu\\n", id, (unsigned long long)actual, (unsigned long long)expected); abort(); } }\nint main(void) {\n'
    driver += '\n'.join(assertions) + f'\n puts("passed {count} independent assertions");\n}}\n'
    return HEADER + '\n'.join(definitions) + 'declare i64 @unused_declaration(i64)\n', driver, count


def validate(ctx):
    run, out = ctx.run, ctx.out
    ir, driver, count = matrix()
    (out / 'matrix.ll').write_text(ir)
    (out / 'matrix_driver.c').write_text(driver)
    compiler = out / 'signed_contract.ll'
    run.require([ctx.clang, '-x', 'c++', '-O1', '-S', '-emit-llvm', FIXTURES / 'signed_contract.cpp',
                 '-o', compiler])
    require(re.search(rb'\b(?:add|mul|shl)\s+(?:nuw\s+)?nsw\b', compiler.read_bytes()),
            'Clang fixture must exercise nsw')
    for name, source, driver in [('matrix', out / 'matrix.ll', out / 'matrix_driver.c'),
                                 ('poison-dividend', FIXTURES / 'poison_dividend.ll',
                                  FIXTURES / 'poison_dividend_driver.c'),
                                 ('signed-contract', compiler, FIXTURES / 'signed_contract_driver.c')]:
        text = out / f'{name}.keel'
        imported = ctx.importer.accept(source, text)
        require(llvm_functions(source.read_bytes()) == keel_functions(imported), (name, 'function inventory'))
        run.require([ctx.clang, '-O0', '-c', source, '-o', out / f'{name}.reference.o'])
        run.require([ctx.clang, '-O0', '-c', driver, '-o', out / f'{name}.driver.o'])
        expected = ctx.link_and_run(ctx.clang, [out / f'{name}.driver.o', out / f'{name}.reference.o'],
                                    out / f'{name}.reference')
        ctx.tiers(name, text, [out / f'{name}.driver.o'], expected)
        if name == 'poison-dividend':
            for tier in TIERS:
                trap = run.run([out / f'{name}.{tier}', 'zero'])
                require(trap.returncode != 0, f'{tier}: zero-divisor trap lost')
    ctx.reject_all(REJECTIONS)
    ctx.verifier_failures(out / 'matrix.ll')
    late = run.run(ctx.importer.command('-', '-'), (HEADER + REJECTIONS['late-rejection']).encode())
    require(late.returncode == 1 and not late.stdout and late.stderr.startswith(b'llvm-import:'),
            'late rejection through stdin')
    return dict(independent_matrix_assertions=count)


if __name__ == '__main__':
    raise SystemExit(main(validate, __doc__, timeout=180))
