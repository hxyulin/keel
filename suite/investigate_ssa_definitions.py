"""Measure definition-table sparsity in the pre-optimization SSA builder.

Diagnostic builds are never used for latency. Sources are restored in finally;
rebuild the ordinary examples afterward. --summarize-only reuses captured logs.
"""
import argparse
import ast
import collections
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess


INSTRUMENTATION = '''        let mut var_counts = vec![0usize; self.var_types.len()];
        let mut var_span = vec![0usize; self.var_types.len()];
        let mut block_counts = vec![0usize; self.func.blocks().map(|b| b.index() + 1).max().unwrap_or(0)];
        let mut block_span = block_counts.clone();
        for &(var, block) in self.defs.keys() {
            var_counts[var.index()] += 1;
            var_span[var.index()] = var_span[var.index()].max(block.index() + 1);
            block_counts[block.index()] += 1;
            block_span[block.index()] = block_span[block.index()].max(var.index() + 1);
        }
        eprintln!("defs-shape: {} {} {} {} {} {} {:?} {:?}", self.var_types.len(), block_counts.len(), self.defs.len(), var_span.iter().sum::<usize>(), block_span.iter().sum::<usize>(), self.func.name, var_counts, block_counts);
'''


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--base', default='f39567e')
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--work-dir', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--summarize-only', action='store_true')
    args = p.parse_args()
    source = pathlib.Path('crates/keel-frontend/src/lib.rs')
    baseline = subprocess.check_output(['git', 'show', f'{args.base}:{source}'], text=True)
    needle = '    pub fn finalize(mut self) {\n'
    assert baseline.count(needle) == 1
    diagnostic = baseline.replace(needle, needle + INSTRUMENTATION)
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    binary = work / 'profile-shape'
    inputs = json.loads(args.profile.read_text())['inputs']
    if not args.summarize_only:
        original = source.read_bytes()
        try:
            source.write_text(diagnostic)
            subprocess.run(['cargo', 'build', '--release', '-p', 'keel-c', '--features', 'profile',
                            '--example', 'profile'], env={**os.environ, 'CARGO_PROFILE_RELEASE_DEBUG': '1'}, check=True)
            shutil.copy2('target/release/examples/profile', binary)
        finally:
            source.write_bytes(original)
        for app, rows in inputs.items():
            with (work / f'{app}-shape.txt').open('w') as log:
                subprocess.run([str(binary), 'baseline', '1', *[r['path'] for r in rows]],
                               stdout=subprocess.DEVNULL, stderr=log, check=True)
    report = dict(base_commit=args.base, diagnostic_source_sha256=hashlib.sha256(diagnostic.encode()).hexdigest(),
                  binary_sha256=sha(binary), instrumentation=INSTRUMENTATION, applications={},
                  method='Untimed diagnostic prints final sparse definition counts, per-variable and per-block occupancy, and hypothetical dense spans. The profile baseline harness compiles each input three times (two API warmups plus one public timed region, whose timing is ignored). Histograms and totals are divided by three. Counts are before final trivial-parameter cleanup. Production sources contain no counters.')
    for app in inputs:
        log = work / f'{app}-shape.txt'
        totals, variables, blocks = collections.Counter(), collections.Counter(), collections.Counter()
        functions = collections.Counter()
        for line in log.read_text().splitlines():
            if not line.startswith('defs-shape: '):
                continue
            m = re.fullmatch(r'defs-shape: (\d+) (\d+) (\d+) (\d+) (\d+) (.*?) (\[[^]]*\]) (\[[^]]*\])', line)
            assert m
            v, b, n, vs, bs = map(int, m.groups()[:5])
            vc, bc = ast.literal_eval(m[7]), ast.literal_eval(m[8])
            assert len(vc) == v and len(bc) == b and sum(vc) == sum(bc) == n
            totals.update(variables=v, blocks=b, definitions=n, variable_dense_span=vs,
                          block_dense_span=bs, full_dense_cells=v*b)
            variables.update(vc)
            blocks.update(bc)
            functions[(m[6], v, b, n)] += 1
        assert all(n % 3 == 0 for group in [totals, variables, blocks, functions] for n in group.values())
        report['applications'][app] = dict(totals={k: n//3 for k, n in totals.items()},
            variable_definition_count_histogram={k: n//3 for k, n in sorted(variables.items())},
            block_definition_count_histogram={k: n//3 for k, n in sorted(blocks.items())},
            largest_functions=[dict(name=f[0], variables=f[1], blocks=f[2], definitions=f[3], occurrences=n//3)
                               for f, n in sorted(functions.items(), key=lambda row: row[0][3], reverse=True)[:12]],
            raw_log_sha256=sha(log))
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
