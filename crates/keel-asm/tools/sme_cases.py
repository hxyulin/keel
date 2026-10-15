#!/usr/bin/env python3
"""Generate one named Clang comparison per selected pinned SME/SME2 record."""
import argparse
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]


def case(name):
    op = name.split('_')[0]
    if name == 'zero_zt_i_': return 'zero {zt0}'
    if name in ('ldr_zt_br_', 'str_zt_br_'): return f'{op} zt0, [sp]'
    if name.startswith(('mova_za2_z_', 'mova_za4_z_', 'mova_mz2_za_', 'mova_mz4_za_')):
        insert = name.startswith('mova_za')
        count = int(name.split('_')[1][-1])
        size = name[-2]; suffix = 's' if size == 'w' else size
        tile = f'za0v.{suffix}[w15, 0:{count-1}]'
        group = '{' + f'z{32-count}.{suffix} - z31.{suffix}' + '}'
        return f'mova {tile}, {group}' if insert else f'mova {group}, {tile}'
    if name.startswith(('mova_za_mz', 'mova_mz_za')):
        insert = name.startswith('mova_za_')
        count = int(name.split('_')[2][-1])
        group = '{' + f'z{32-count}.d - z31.d' + '}'
        array = f'za.d[w11, 7, vgx{count}]'
        return f'mova {array}, {group}' if insert else f'mova {group}, {array}'
    if name.startswith(('fmla_za_', 'fmls_za_', 'add_za_', 'sub_za_')):
        parts = name.split('_'); kind = parts[2]; tag = parts[3]
        count = int(tag[1] if kind == 'zzi' else tag[0])
        suffix = tag[0] if kind == 'zzi' else 'h' if name.endswith('_16') else 's'
        group = '{' + f'z{32-count}.{suffix} - z31.{suffix}' + '}'
        source = f'{op} za.{suffix}[w11, 7, vgx{count}], {group}'
        if kind == 'zzw': return source + ', {' + f'z0.{suffix} - z{count-1}.{suffix}' + '}'
        if kind == 'zzv': return source + f', z15.{suffix}'
        if kind == 'zw': return source
        if kind == 'zzi': return source + f', z15.{suffix}[1]'
        raise ValueError(name)
    if op == 'addsvl': return 'addsvl sp, x30, #-32'
    if op == 'rdsvl': return 'rdsvl x30, #31'
    if name in ('ldr_za_ri_', 'str_za_ri_'): return f'{op} za[w15, 15], [sp, #15, mul vl]'
    if name.startswith(('mova_za_p_rz_', 'mova_z_p_rza_')):
        size = name[-1]; suffix = 's' if size == 'w' else size
        tile = f'za{(1 << "bhwdq".index(size))-1}v.{suffix}[w15, 0]'
        return f'mova {tile}, p7/m, z31.{suffix}' if name.startswith('mova_za_') else f'mova z31.{suffix}, p7/m, {tile}'
    if '_za_p_rrr_' in name:
        size = op[-1]; suffix = 's' if size == 'w' else size
        shift = 'bhwdq'.index(size)
        pred = 'p7/z' if op.startswith('ld') else 'p7'
        return f'{op} {{za{(1<<shift)-1}v.{suffix}[w15, 0]}}, {pred}, [sp, x30, lsl #{shift}]'
    if op in ('addha', 'addva'):
        suffix = 's' if name.endswith('32') else 'd'
        return f'{op} za0.{suffix}, p0/m, p7/m, z31.{suffix}'
    if '_pp_zz_' in name:
        if op.startswith('bf'):
            dst, src = ('s' if 'za32' in name else 'h'), 'h'
        elif name.endswith('64'):
            dst, src = 'd', ('d' if op.startswith('f') else 'h')
        elif name.endswith('16'):
            dst, src = ('s' if 'za32' in name else 'h'), 'h'
        else:
            dst, src = 's', ('s' if op.startswith('f') else 'b')
        return f'{op} za0.{dst}, p0/m, p7/m, z31.{src}, z0.{src}'
    raise ValueError(name)


def output():
    names = [n for n in (ROOT / 'isa/sme.txt').read_text().splitlines() if n and not n.startswith('#')]
    header = ('# Named SME/SME2 cases corresponding to isa/sme.txt.\n'
              '# Encoding-name inventory derived from Arm AARCHMRS 2025-12.\n'
              '# Copyright (c) 2010-2025 Arm Limited or its affiliates. All rights reserved.\n'
              '# Arm-derived data: BSD-3-Clause; see ../isa/LICENSE-ARM.\n')
    return header + ''.join(f'{n}\t{case(n)}\n' for n in names)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    target = ROOT / 'tests/sme-cases.txt'
    result = output()
    if args.check:
        assert target.read_text() == result, 'SME cases are stale'
    else:
        target.write_text(result)
