#!/usr/bin/env python3
"""Generate one named Clang comparison per selected pinned SME/SME2 record."""
import argparse
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]


def case(name):
    op = name.split('_')[0]
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
