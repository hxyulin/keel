"""Summarize C frontend timings and macOS stack trees without double-counting recursion."""
import argparse
import collections
import json
import pathlib
import re
import statistics
import subprocess


def stack_summary(path):
    demangled = subprocess.check_output(['c++filt'], input=path.read_text(), text=True)
    graph = demangled.split('Call graph:\n', 1)[1].split('Total number in stack', 1)[0]
    nodes, stack = [], []
    for line in graph.splitlines():
        match = re.match(r'^([ +!:|]*)(\d+) (.+)$', line)
        if not match:
            continue
        depth, count, frame = len(match[1]), int(match[2]), match[3]
        name = frame.split('  (in ', 1)[0]
        while stack and nodes[stack[-1]]['depth'] >= depth:
            stack.pop()
        parent = stack[-1] if stack else None
        node = {'name': name, 'count': count, 'self': count, 'depth': depth, 'parent': parent}
        if parent is not None:
            nodes[parent]['self'] -= count
        nodes.append(node)
        stack.append(len(nodes) - 1)
    assert nodes and all(n['self'] >= 0 for n in nodes)
    total = sum(n['count'] for n in nodes if n['parent'] is None)
    assert sum(n['self'] for n in nodes) == total
    inclusive, leaves, categories = collections.Counter(), collections.Counter(), collections.Counter()
    groups = {
        'allocator': lambda s: any(k in s for k in ['malloc', '_xzm_', '_xzone_', 'free', 'realloc']) and 'freebsd' not in s,
        'memmove': lambda s: 'memmove' in s,
        'token_clone': lambda s: 'Token' in s and 'clone' in s,
        'ssa_predecessor_scan': lambda s: 'FunctionBuilder' in s and '::find_preds' in s,
        'ssa_seal_block': lambda s: 'FunctionBuilder' in s and '::seal_block' in s,
        'ssa_read': lambda s: 'FunctionBuilder' in s and '::read' in s,
        'ssa_builder': lambda s: 'keel_frontend::' in s,
        'pp_scan': lambda s: 'keel_c::preprocess::scan' in s,
        'pp_expand': lambda s: 'Preprocessor' in s and '::expand' in s,
        'pp_emit': lambda s: 'Preprocessor' in s and '::emit' in s,
        'pp_file': lambda s: 'Preprocessor' in s and '::file' in s,
        'lex': lambda s: 'keel_c::lex::' in s,
        'parse': lambda s: 'keel_c::parse::' in s or 'keel_c::expr::' in s,
    }
    for i, node in enumerate(nodes):
        if not node['self']:
            continue
        names = set()
        while i is not None:
            names.add(nodes[i]['name'])
            i = nodes[i]['parent']
        for name in names:
            inclusive[name] += node['self']
        leaves[node['name']] += node['self']
        for group, predicate in groups.items():
            if any(predicate(name) for name in names):
                categories[group] += node['self']
    return {'samples': total, 'category_percent': {k: 100 * v / total for k, v in categories.items()},
            'top_inclusive': [{'function': k, 'samples': v, 'percent': 100 * v / total} for k, v in inclusive.most_common(45)],
            'top_self': [{'function': k, 'samples': v, 'percent': 100 * v / total} for k, v in leaves.most_common(30)]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    out = {'git_head': report['git_head'], 'workloads': {}, 'stacks': {}}
    for app, paths in report['inputs'].items():
        rows = [r for r in report['samples'] if r['app'] == app]
        files = []
        for p in paths:
            selected = [r for r in rows if r['path'] == p['path'] and not r['debug'] and r['mode'] == 'staged']
            files.append({'name': pathlib.Path(p['path']).name, 'bytes': p['bytes'], 'tokens': selected[0]['tokens'], 'functions': selected[0]['functions'], **{k: statistics.median(r[k] for r in selected) for k in ['total_ms', 'lex_ms', 'parse_ms', 'irgen_ms', 'cleanup_ms']}})
        out['workloads'][app] = {'files': len(paths), 'bytes': sum(p['bytes'] for p in paths), 'tokens': sum(f['tokens'] for f in files), 'functions': sum(f['functions'] for f in files), 'top_files': sorted(files, key=lambda r: r['total_ms'], reverse=True)[:5]}
    for name, meta in report['stack_samples'].items():
        if meta['returncode'] != 0:
            raise RuntimeError(meta)
        out['stacks'][name] = stack_summary(pathlib.Path(meta['path']))
    args.output.write_text(json.dumps(out, indent=2) + '\n')
    for name, row in out['stacks'].items():
        print(name, row['samples'], {k: round(v, 1) for k, v in row['category_percent'].items()})
        for entry in row['top_self'][:7]:
            print(f"  {entry['percent']:.1f}% {entry['function']}")


if __name__ == '__main__':
    main()
