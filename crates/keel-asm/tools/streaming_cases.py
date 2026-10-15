#!/usr/bin/env python3
"""Generate named cases for the contiguous streaming-memory increment."""
import argparse
from pathlib import Path
from generate import STREAMING_NAMES
ROOT=Path(__file__).resolve().parents[1]

def case(name):
    op=name.split('_')[0]
    if op.startswith('while'): return f'{op} p15.d, x30, xzr'
    if op=='addvl': return 'addvl sp, x30, #-32'
    if op=='dup': return 'dup z31.d, sp'
    load=op.startswith('ld')
    size={'8':'b','16':'h','32':'s','64':'d'}.get(name.split('_')[-1][1:]) if load else 'd'
    shift='bhwd'.index(op[-1])
    addr='[sp, #-8, mul vl]' if '_bi_' in name else f'[sp, x30, lsl #{shift}]'
    return f'{op} {{z31.{size}}}, p7'+('/z' if load else '')+f', {addr}'

def output():
    return ('# Named cases from Arm AARCHMRS 2025-12, schema 2.7.4.\n'
            '# Copyright (c) 2010-2025 Arm Limited or its affiliates. All rights reserved.\n'
            '# Arm-derived data: BSD-3-Clause; see ../isa/LICENSE-ARM.\n'+
            ''.join(f'{n}\t{case(n)}\n' for n in sorted(STREAMING_NAMES)))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--check',action='store_true');a=p.parse_args()
    target=ROOT/'tests/streaming-cases.txt';s=output()
    if a.check: assert target.read_text()==s, 'streaming cases are stale'
    else: target.write_text(s)
