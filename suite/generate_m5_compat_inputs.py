#!/usr/bin/env python3
"""Repeat correctness seeds for encoding-only assembler measurements."""
import argparse
import json
import math
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--mib', type=float, default=1)
    args = parser.parse_args()
    if args.mib <= 0:
        parser.error('--mib must be positive')
    args.output_dir.mkdir(parents=True, exist_ok=True)
    tests = ROOT / 'crates/keel-asm/tests'
    seeds = {}
    for name in ('neon', 'numeric', 'sme', 'streaming'):
        lines = (tests / f'{name}-cases.txt').read_text().splitlines()
        seeds[name] = '\n'.join(s.replace('\\t', '\t').split('\t', 1)[1]
                                for s in lines if s and not s.startswith('#')) + '\n'
    source = (tests / 'm5_compat.rs').read_text()
    seeds['m5-compat'] = '\n'.join(json.loads('"' + match.group(1) + '"')
                                   for match in re.finditer(r'const (?:PAC|CRYPTO): &str = "([^"]+)";', source))
    assert seeds['m5-compat'], 'missing correctness seeds'
    seeds['m5-compat'] += '\nbti c\nmrs x0, tpidrro_el0\ndc cvau, x0\nprfm pldl1keep, [x0, #64]\ncasp x0, x1, x2, x3, [x4]\n'
    for name, seed in seeds.items():
        repeats = math.ceil(args.mib * 1024 * 1024 / len(seed.encode()))
        (args.output_dir / f'{name}.s').write_text('.text\n' + seed * repeats)


if __name__ == '__main__':
    main()
