"""Add ownership of comparison, allocation, and clone samples to stage profiles."""
import argparse
import collections
import json
import pathlib
import re
import subprocess


def attribution(path):
    graph = subprocess.check_output(['c++filt'], input=path.read_text(), text=True)
    graph = graph.split('Call graph:\n', 1)[1].split('Total number in stack', 1)[0]
    nodes, parents = [], []
    for line in graph.splitlines():
        match = re.match(r'^([ +!:|]*)(\d+) (.+)$', line)
        if not match:
            continue
        depth, count, name = len(match[1]), int(match[2]), match[3].split('  (in ', 1)[0]
        while parents and nodes[parents[-1]]['depth'] >= depth:
            parents.pop()
        parent = parents[-1] if parents else None
        if parent is not None:
            nodes[parent]['self'] -= count
        nodes.append(dict(depth=depth, self=count, name=name, frame=match[3], parent=parent))
        parents.append(len(nodes) - 1)
    total = sum(n['self'] for n in nodes)
    phase_totals = collections.Counter()
    comparison_sites = collections.Counter()
    costs = {key: collections.Counter() for key in ['comparisons', 'allocator', 'string_clones', 'token_clones']}
    for i, node in enumerate(nodes):
        if not node['self']:
            continue
        assert node['self'] > 0
        names, frames = [], []
        while i is not None:
            names.append(nodes[i]['name'])
            frames.append(nodes[i]['frame'])
            i = nodes[i]['parent']
        # Use the pipeline entry frames, not helper namespaces. Tok::clone
        # belongs to the caller's phase, even though Tok lives in lex.rs.
        if any('keel_c::lex::tokenize' in s for s in names):
            phase = 'Lex'
        elif any('Parser' in s and '::translation_unit' in s for s in names):
            phase = 'Parse / types'
        elif any('irgen::Gen' in s and '::run' in s for s in names):
            phase = 'IR generation'
        elif any('drop_glue::<keel_c::parse::Parser>' in s for s in names):
            phase = 'Parser / token / AST cleanup'
        elif any('drop_glue::<keel_ir::module::Module>' in s for s in names):
            phase = 'Module destruction'
        else:
            phase = 'Other / inlined boundary'
        phase_totals[phase] += node['self']
        checks = {
            'comparisons': any('memcmp' in s for s in names),
            'allocator': any(any(k in s for k in ['malloc', '_xzm_', '_xzone_', 'free', 'realloc']) and 'freebsd' not in s for s in names),
            'string_clones': any('alloc::string::String' in s and '::clone' in s for s in names),
            'token_clones': any('keel_c::lex::Tok' in s and '::clone' in s for s in names),
        }
        if checks['comparisons'] and phase == 'Lex':
            caller = next(frame for frame in frames if 'keel_c::lex::tokenize' in frame)
            offset = re.search(r'\+ (\d+)\s+\[', caller)
            comparison_sites[int(offset[1]) if offset else 'unresolved'] += node['self']
        for key, yes in checks.items():
            if yes:
                costs[key][phase] += node['self']
    assert sum(phase_totals.values()) == total
    return {'samples': total,
            'lexer_comparison_sites': [{'tokenize_symbol_offset': k, 'samples': v, 'percent': 100*v/total} for k, v in comparison_sites.most_common()],
            'phase_sample_percent': {k: 100*v/total for k, v in phase_totals.most_common()},
            'cost_sample_percent_by_phase': {key: {k: 100*v/total for k, v in rows.most_common()} for key, rows in costs.items()}}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--analysis', type=pathlib.Path, required=True)
    args = p.parse_args()
    profile = json.loads(args.profile.read_text())
    analysis = json.loads(args.analysis.read_text())
    analysis['attribution_method'] = 'Each leaf sample is assigned by pipeline entry frame (tokenize, translation_unit, Gen::run, parser drop), preventing parser token cloning from being misclassified as lexing. Percentages use the whole compile-plus-module-drop sample denominator. Cost groups overlap. Optimized inlining can hide frames; unclassified samples stay Other.'
    for app, meta in profile['stack_samples'].items():
        details = attribution(pathlib.Path(meta['path']))
        assert details['samples'] == analysis['stacks'][app]['samples']
        analysis['stacks'][app]['attribution'] = details
        print(app, details)
    args.analysis.write_text(json.dumps(analysis, indent=2) + '\n')


if __name__ == '__main__':
    main()
