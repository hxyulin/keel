#!/usr/bin/env python3
"""Integer flag checks; invoke through suite/fast_opt_locked.py (no lock here).

Use a new private --out directory. Original LLVM and Clang output is consumed
unchanged. See llvm_import_flags_fixtures/README.md for the refinement domain.
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
FIXTURES = ROOT / 'suite/llvm_import_flags_fixtures'
HEADER = '''target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "arm64-apple-macosx11.0.0"
'''


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

    def run(command, ok=True, stdin=None):
        command = list(map(str, command))
        commands.append(command)
        result = subprocess.run(command, input=stdin, capture_output=True, timeout=180)
        if ok:
            assert result.returncode == 0, (command, result.returncode, result.stderr.decode(errors='replace'))
        return result

    def importer(source, dest, keel=None, stdin=None):
        return run([sys.executable, TOOL, '--libllvm', args.libllvm, '--keel', keel or args.keel,
                    source, '-o', dest], ok=False, stdin=stdin)

    try:
        ir, driver, count = matrix()
        (out/'matrix.ll').write_text(ir)
        (out/'matrix_driver.c').write_text(driver)
        compiler = out/'signed_contract.ll'
        run([args.clang, '-x', 'c++', '-O1', '-S', '-emit-llvm', FIXTURES/'signed_contract.cpp', '-o', compiler])
        assert re.search(rb'\b(?:add|mul|shl)\s+(?:nuw\s+)?nsw\b', compiler.read_bytes()), 'Clang fixture must exercise nsw'
        for name, source, driver in [('matrix', out/'matrix.ll', out/'matrix_driver.c'),
                ('poison-dividend', FIXTURES/'poison_dividend.ll', FIXTURES/'poison_dividend_driver.c'),
                ('signed-contract', compiler, FIXTURES/'signed_contract_driver.c')]:
            original = source.read_bytes()
            dest = out/f'{name}.keel'
            dest.write_bytes(b'sentinel\n')
            result = importer(source, dest)
            assert result.returncode == 0 and not result.stdout, result.stderr.decode()
            assert source.read_bytes() == original and dest.read_bytes() != b'sentinel\n'
            source_names = re.findall(rb'^(?:define|declare)\b[^@\n]*@([\w.$]+)\(', original, re.M)
            output_names = re.findall(rb'^(?:func|decl) @([\w.$]+)\(', dest.read_bytes(), re.M)
            assert source_names == output_names, (source_names, output_names)
            result = importer('-', '-', stdin=original)
            assert result.returncode == 0 and result.stdout == dest.read_bytes(), result.stderr.decode()
            run([args.clang, '-O0', '-c', source, '-o', out/f'{name}.reference.o'])
            run([args.clang, '-O0', '-c', driver, '-o', out/f'{name}.driver.o'])
            run([args.clang, out/f'{name}.driver.o', out/f'{name}.reference.o', '-o', out/f'{name}.reference'])
            expected = run([out/f'{name}.reference']).stdout
            for tier in ('none', 'fast', 'full'):
                obj, exe = out/f'{name}.{tier}.o', out/f'{name}.{tier}'
                run([args.keel, 'compile', '--opt-tier', tier, '--emit-object', dest, '-o', obj])
                run([args.clang, out/f'{name}.driver.o', obj, '-o', exe])
                assert run([exe]).stdout == expected, (name, tier)
                if name == 'poison-dividend':
                    assert run([exe, 'zero'], ok=False).returncode != 0, 'zero-divisor trap lost'
                checks.append(dict(kind='native-independent-oracle', case=name, tier=tier))
            assert source.read_bytes() == original
            checks.append(dict(kind='source-and-all-functions-preserved', case=name,
                               sha256=hashlib.sha256(original).hexdigest()))
        cases = {
            'poison': 'define i64 @f() { ret i64 poison }',
            'undef': 'define i64 @f() { ret i64 undef }',
            'freeze': 'define i64 @f(i64 %x) { %v = freeze i64 %x\nret i64 %v }',
            'i1-arithmetic': 'define void @f() { %v = add nsw i1 true, false\nret void }',
            'i1-ordered': 'define void @f(i64 %x) { %a = trunc i64 %x to i1\n%v = icmp samesign slt i1 %a, true\nret void }',
            'pointer-samesign': 'define void @f(ptr %x) { %v = icmp samesign eq ptr %x, null\nret void }',
            'vector': 'define <2 x i64> @f(<2 x i64> %x) { %v = add nsw <2 x i64> %x, %x\nret <2 x i64> %v }',
            'fp-nneg': 'define double @f(i64 %x) { %v = uitofp nneg i64 %x to double\nret double %v }',
            'unknown-flag': 'define i64 @f(i64 %x) { %v = add imaginary i64 %x, 1\nret i64 %v }',
            'late-rejection': 'define i64 @good(i64 %x) { %r = add nsw i64 %x, 1\nret i64 %r }\ndefine i64 @bad() { ret i64 poison }',
            'unsupported-abi': 'declare void @g(ptr byval(i64))',
            'malformed': 'define i64 @f( {',
        }
        for name, body in cases.items():
            source = out/f'reject-{name}.ll'
            original = (HEADER + body + '\n').encode()
            source.write_bytes(original)
            for destination_exists in (True, False):
                dest = out/f'reject-{name}-{destination_exists}.keel'
                if destination_exists:
                    dest.write_bytes(b'sentinel\n')
                result = importer(source, dest)
                assert result.returncode == 1 and not result.stdout and b'llvm-import:' in result.stderr, (name, result)
                assert source.read_bytes() == original
                assert dest.read_bytes() == b'sentinel\n' if destination_exists else not dest.exists()
            checks.append(dict(kind='rejection-preservation', case=name))
        for verifier in ('/usr/bin/false', out/'missing-keel'):
            for destination_exists in (True, False):
                dest = out/f'verifier-preserved-{destination_exists}.keel'
                if destination_exists:
                    dest.write_bytes(b'sentinel\n')
                result = importer(out/'matrix.ll', dest, verifier)
                assert result.returncode == 1 and not result.stdout and b'llvm-import:' in result.stderr
                assert dest.read_bytes() == b'sentinel\n' if destination_exists else not dest.exists()
        result = importer('-', '-', stdin=(HEADER + cases['late-rejection']).encode())
        assert result.returncode == 1 and not result.stdout and b'llvm-import:' in result.stderr
        checks.append(dict(kind='verifier-failure-preservation'))
        print(json.dumps(dict(native_builds=9, independent_matrix_assertions=count,
                              rejections=len(cases), checks=len(checks))))
    finally:
        (out/'commands.json').write_text(json.dumps(commands, indent=2))
        (out/'checks.json').write_text(json.dumps(checks, indent=2))


if __name__ == '__main__':
    main()
