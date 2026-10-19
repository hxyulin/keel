#!/usr/bin/env python3
"""Inclusive/self sample attribution from a macOS `sample` call graph.

Recursion-safe: a node contributes to a symbol's inclusive count only when no
ancestor has the same symbol. Symbols are shortened Rust v0 names.

  python3 suite/ir_lowering_sample_tree.py SAMPLE.txt [--root SUBSTR] [--top N]
     [--match SUBSTR ...]

--root restricts attribution to samples under the first-matching ancestor
(e.g. `irgen3Gen3run` for C->IR generation only); percentages are of that root.
"""
import argparse
import re
from collections import Counter

LINE = re.compile(r'^(?P<prefix>[\s+!:|]*)(?P<count>\d+)\s+(?P<sym>.+?)\s+\(in ')


def short(sym):
    """Minimal Rust v0 path demangling: identifiers only, crate hashes dropped."""
    if not sym.startswith('_R'):
        return sym[:80]
    out, i, n = [], 2, len(sym)
    while i < n:
        c = sym[i]
        if c.isdigit():
            j = i
            while j < n and sym[j].isdigit():
                j += 1
            length = int(sym[i:j])
            if j < n and sym[j] == '_':
                j += 1
            out.append(sym[j:j + length])
            i = j + length
            continue
        m = re.match(r'(?:Cs|B|s)[0-9A-Za-z]*_', sym[i:])
        if m and c in 'CBs':
            i += m.end()
            continue
        i += 1
    out = [o for o in out if o and o not in ('core', 'alloc', 'std')]
    return '::'.join(out[-3:]) if out else sym[:80]


def parse(path):
    nodes = []  # (depth, count, sym)
    in_graph = False
    for line in open(path, errors='replace'):
        if line.startswith('Call graph:'):
            in_graph = True
            continue
        if in_graph and (line.startswith('Total number in stack') or line.startswith('Sort by top')):
            break
        if not in_graph:
            continue
        m = LINE.match(line)
        if not m:
            continue
        nodes.append((len(m.group('prefix')), int(m.group('count')), m.group('sym')))
    return nodes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('sample')
    ap.add_argument('--root')
    ap.add_argument('--top', type=int, default=40)
    ap.add_argument('--match', nargs='*', default=[])
    args = ap.parse_args()
    nodes = parse(args.sample)
    inclusive, selfc = Counter(), Counter()
    stack = []  # (depth, sym, count)
    root_total = 0
    for i, (depth, count, sym) in enumerate(nodes):
        while stack and stack[-1][0] >= depth:
            stack.pop()
        ancestors = [s for _, s, _ in stack]
        if args.root:
            under = any(args.root in s for s in ancestors + [sym])
            if not under:
                stack.append((depth, sym, count))
                continue
            if args.root in sym and not any(args.root in s for s in ancestors):
                root_total += count
        name = short(sym)
        if name not in {short(s) for s in ancestors}:
            inclusive[name] += count
        child = sum(c for d, _, c in nodes[i + 1:i + 1 + 0])  # placeholder
        stack.append((depth, sym, count))
    # self = count minus direct children counts
    for i, (depth, count, sym) in enumerate(nodes):
        children = 0
        j = i + 1
        child_depth = None
        while j < len(nodes) and nodes[j][0] > depth:
            if child_depth is None:
                child_depth = nodes[j][0]
            if nodes[j][0] == child_depth:
                children += nodes[j][1]
            j += 1
        selfc[short(sym)] += count - children
    total = root_total or max((c for d, c, s in nodes if d == min(n[0] for n in nodes)), default=1)
    print(f'root={args.root} total_samples={total}')
    print('-- inclusive')
    for name, c in inclusive.most_common(args.top):
        print(f'{c:7d} {100 * c / total:6.2f}%  {name}')
    for m in args.match:
        hits = [(n, c) for n, c in inclusive.items() if m in n]
        for n, c in sorted(hits, key=lambda x: -x[1])[:10]:
            print(f'match {m}: {c:7d} {100 * c / total:6.2f}%  {n}')


if __name__ == '__main__':
    main()
