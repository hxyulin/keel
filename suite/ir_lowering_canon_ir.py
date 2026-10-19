#!/usr/bin/env python3
"""Compare Display-format IR modulo value numbering.

  python3 suite/ir_lowering_canon_ir.py A_DIR B_DIR [--ext txt,g.txt]

Compares every A_DIR/*.EXT with B_DIR/<same name> (Display text, e.g. from
`format!("{module}")`).

Value names (vN) are renamed per function in order of first textual
appearance (block parameters and results are printed at their definitions,
operands resolved through aliases), so two modules whose instructions,
layout, block parameter order and operands agree compare equal even if
dead/alias value numbers differ. Everything else (instructions, blocks,
inst numbering is not printed, locations, types, attributes) must match
byte for byte.
"""
import re, sys, pathlib
# Preserve quoted contents and global names: only SSA values may be renamed.
V = re.compile(r'"(?:\\.|[^"\\])*"|@[A-Za-z0-9_.$-]+|\bv(\d+)\b')

def canon(text):
    out = []
    names = {}
    for line in text.splitlines():
        if line.startswith('func @'):
            names = {}
        def sub(m):
            k = m.group(1)
            if k is None:
                return m.group(0)
            if k not in names:
                names[k] = len(names)
            return 'v%d' % names[k]
        out.append(V.sub(sub, line))
    return '\n'.join(out)

a, b = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
exts = sys.argv[sys.argv.index('--ext') + 1].split(',') if '--ext' in sys.argv else ['txt', 'g.txt']
bad = same_raw = n = 0
for ext in exts:
    for fa in sorted(a.glob('*.' + ext)):
        if ext == 'txt' and fa.name.endswith('.g.txt'):
            continue
        fb = b / fa.name
        ta, tb = fa.read_text(), fb.read_text()
        n += 1
        if ta == tb:
            same_raw += 1
            continue
        if canon(ta) != canon(tb):
            bad += 1
            ca, cb = canon(ta).splitlines(), canon(tb).splitlines()
            for i, (x, y) in enumerate(zip(ca, cb)):
                if x != y:
                    print(f'DIFF {fa.name}:{i+1}\n  - {x}\n  + {y}')
                    break
            else:
                print(f'DIFF {fa.name}: length {len(ca)} vs {len(cb)}')
print(f'files={n} raw-identical={same_raw} canon-different={bad}')
sys.exit(1 if bad else 0)
