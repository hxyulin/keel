#!/usr/bin/env python3
"""Summarize ir_lowering_probe `stages` JSON lines: per-app sum of per-file
medians for each stage, plus backend self-time breakdown.

  python3 suite/ir_lowering_summarize.py stages.jsonl [more.jsonl ...]
"""
import collections
import json
import statistics
import sys

STAGES = ['lex_ms', 'parse_ms', 'irgen_ms', 'cleanup_ms', 'frontend_ms', 'verify_ms',
          'opt_basic_ms', 'opt_full_ms', 'codegen_fast_ms', 'codegen_full_ms',
          'codegen_after_optfull_ms', 'assemble_ms', 'ir_drop_ms']


def app(path):
    parts = path.split('/')
    return parts[parts.index('pre') - 1] if 'pre' in parts else path


def summarize(path):
    rows = [json.loads(line) for line in open(path)]
    by_file = collections.defaultdict(list)
    for r in rows:
        by_file[r['path']].append(r)
    out = collections.defaultdict(lambda: collections.defaultdict(float))
    for f, rs in by_file.items():
        a = app(f)
        for s in STAGES:
            out[a][s] += statistics.median(r[s] for r in rs)
        for s in rs[0]['backend_self_ms']:
            out[a]['be:' + s] += statistics.median(r['backend_self_ms'][s] for r in rs)
    return out


def main():
    for path in sys.argv[1:]:
        out = summarize(path)
        print(f'== {path} (sum over files of per-file medians, ms)')
        apps = list(out)
        keys = list(next(iter(out.values())).keys())
        print(f'{"stage":34}' + ''.join(f'{a:>12}' for a in apps) + f'{"total":>12}')
        for k in keys:
            vals = [out[a][k] for a in apps]
            print(f'{k:34}' + ''.join(f'{v:12.2f}' for v in vals) + f'{sum(vals):12.2f}')


if __name__ == '__main__':
    main()
