#!/usr/bin/env python3
"""Generate a named Clang-oracle case for each pinned baseline NEON record.

This is an encoding-form inventory, not a complete operand-space test. The
handwritten differential/native tests cover arrangements and boundary operands.
No expected opcode bits are embedded here: Arm masks and Clang are the oracles.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def case(record):
    name = record['name']
    op, cls, *tags = name.split('_')
    op = op.lower()
    tag = '_'.join(tags)
    fields = {f['name'] for f in record['encoding']['values'] if 'x' in f['value']['value']}
    scalar = cls.startswith('asisd') and not cls.startswith('asisdl')
    if cls.startswith('asisdl'):
        # Full structures, lane structures, and replicates, with separate
        # no-writeback/register-post-index/immediate-post-index records.
        n = int(op[2])
        replicate = op.endswith('r')
        full = cls in ('asisdlse', 'asisdlsep')
        if full:
            length = int(tags[0][-1])
            items = ','.join(f'v{i}.16b' for i in range(length))
            operand = '{' + items + '}'
            increment = 16 * length
        elif replicate:
            operand = '{' + ','.join(f'v{i}.4s' for i in range(n)) + '}'
            increment = 4 * n
        else:
            element = tags[0][0].lower()
            operand = '{' + ','.join(f'v{i}.{element}' for i in range(n)) + '}[0]'
            increment = (1 << 'bhsd'.index(element)) * n
        post = ''
        if cls in ('asisdlsep', 'asisdlsop'):
            post = ', x2' if 'Rm' in fields else f', #{increment}'
        return f'{op} {operand}, [x0]{post}'
    if cls == 'asimdperm':
        return f'{op} v0.4s, v1.4s, v2.4s'
    if cls == 'asimdext':
        return 'ext v0.16b, v1.16b, v2.16b, #15'
    if cls == 'asimdtbl':
        length = int(tags[0][1:])
        return f'{op} v0.16b, ' + '{' + ','.join(f'v{i}.16b' for i in range(length)) + '}, v5.16b'
    if cls == 'asisdone':
        return 'dup d0, v1.d[1]'
    if cls == 'asimdins':
        return {
            ('dup', 'DV_v'): 'dup v0.4s, v1.s[3]',
            ('dup', 'DR_r'): 'dup v0.4s, w1',
            ('ins', 'IR_r'): 'ins v0.s[3], w1',
            ('ins', 'IV_v'): 'ins v0.s[3], v1.s[2]',
            ('smov', 'W_w'): 'smov w0, v1.h[7]',
            ('smov', 'X_x'): 'smov x0, v1.s[3]',
            ('umov', 'W_w'): 'umov w0, v1.s[3]',
            ('umov', 'X_x'): 'umov x0, v1.d[1]',
        }[(op, tag)]
    if cls == 'asimdimm':
        arr = {'L_sl': '4s', 'L_hl': '8h', 'M_sm': '4s', 'N_b': '16b', 'D_ds': None, 'D2_d': '2d', 'S_s': '4s'}[tag]
        d = 'd0' if arr is None else f'v0.{arr}'
        value = '#1.0' if op == 'fmov' else '#0xff' if tag not in ('D_ds', 'D2_d') else '#0xff00ff00ff00ff00'
        extra = ', msl #8' if tag == 'M_sm' else ''
        return f'{op} {d}, {value}{extra}'
    if cls in ('asimdall', 'asisdpair'):
        if cls == 'asisdpair':
            return f'{op} ' + ('s0, v1.2s' if op.startswith('f') else 'd0, v1.2d')
        if op.startswith('f'):
            return f'{op} s0, v1.4s'
        return f'{op} ' + ('h0, v1.16b' if op in ('saddlv', 'uaddlv') else 'b0, v1.16b')
    if cls in ('asimddiff', 'asisddiff'):
        if scalar:
            return f'{op} s0, h1, h2'
        if tag == 'N':
            return f'{op} v0.8b, v1.8h, v2.8h'
        narrow = '8b' if op == 'pmull' else '4h'
        wide = '8h' if op == 'pmull' else '4s'
        return f'{op} v0.{wide}, v1.{wide if tag == "W" else narrow}, v2.{narrow}'
    if cls in ('asimdelem', 'asisdelem'):
        fp = op.startswith('f')
        e = 's' if fp else 'h'
        if scalar:
            d = 's0' if tag == 'L' or fp else 'h0'
            n = e + '1'
        else:
            d = 'v0.4s' if tag == 'L' or fp else 'v0.8h'
            n = 'v1.4s' if fp else 'v1.4h' if tag == 'L' else 'v1.8h'
        return f'{op} {d}, {n}, v2.{e}[1]'
    if cls in ('asimdshf', 'asisdshf'):
        if tag == 'N':
            d, n = ('b0', 'h1') if scalar else ('v0.8b', 'v1.8h')
        elif tag == 'L':
            d, n = 'v0.8h', 'v1.8b'
        elif scalar:
            d, n = 'd0', 'd1'
        else:
            d, n = ('v0.4s', 'v1.4s') if tag == 'C' else ('v0.8h', 'v1.8h')
        return f'{op} {d}, {n}, #1'
    if cls in ('asimdsame', 'asisdsame', 'asimdmisc', 'asisdmisc'):
        if tag == 'N':
            if op == 'fcvtxn':
                d, n = ('s0', 'd1') if scalar else ('v0.2s', 'v1.2d')
            elif op == 'fcvtn':
                d, n = 'v0.4h', 'v1.4s'
            else:
                d, n = ('b0', 'h1') if scalar else ('v0.8b', 'v1.8h')
            return f'{op} {d}, {n}'
        if tag == 'L':
            return f'{op} v0.4s, v1.4h'
        if tag == 'P':
            return f'{op} v0.4s, v1.8h'
        if tag == 'S':
            return 'shll v0.8h, v1.8b, #8'
        if 'sz' in fields:
            arr = 's' if scalar else '4s'
        elif 'size' in fields:
            arr = 'h' if scalar else '8h'
            if op in ('rev16', 'cnt', 'pmul'):
                arr = '16b'
        else:
            arr = 'd' if scalar else '16b'
        d, n, m = [f'{arr}{i}' if scalar else f'v{i}.{arr}' for i in range(3)]
        if tag in ('Z', 'FZ'):
            return f'{op} {d}, {n}, #' + ('0.0' if tag == 'FZ' else '0')
        return f'{op} {d}, {n}' + (f', {m}' if 'Rm' in fields else '')
    raise AssertionError(f'Unhandled NEON class: {name}')


def generate():
    data = json.loads((ROOT / 'isa/a64.json').read_text())
    records = {r['name']: r for r in data['instructions']}
    names = [line for line in (ROOT / 'isa/neon.txt').read_text().splitlines()
             if line and not line.startswith('#')]
    return ('# Generated by tools/neon_cases.py; names from pinned Arm AARCHMRS.\n'
            '# Arm-derived names: BSD-3-Clause; see ../isa/LICENSE-ARM.\n'
            + ''.join(f'{name}\t{case(records[name])}\n' for name in names))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = ROOT / 'tests/neon-cases.txt'
    output = generate()
    if args.check:
        assert target.read_text() == output, 'neon-cases.txt is stale'
    else:
        target.write_text(output)
