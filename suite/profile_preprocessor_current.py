"""Profile the current preprocessor on the previously recorded raw C corpus.

Build a release profiling example with CARGO_PROFILE_RELEASE_DEBUG=1 first.
Timings and macOS stack sampling are separate, serial passes after warmup.
"""
import argparse
import collections
import re
import statistics
import hashlib
import json
import pathlib
import platform
import subprocess

from summarize_c_frontend import stack_summary


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stack_details(path):
    graph = subprocess.check_output(['c++filt'], input=path.read_text(), text=True)
    graph = graph.split('Call graph:\n', 1)[1].split('Total number in stack', 1)[0]
    nodes, parents = [], []
    for line in graph.splitlines():
        m = re.match(r'^([ +!:|]*)(\d+) (.+)$', line)
        if not m:
            continue
        depth, count, name = len(m[1]), int(m[2]), m[3].split('  (in ', 1)[0]
        while parents and nodes[parents[-1]]['depth'] >= depth:
            parents.pop()
        parent = parents[-1] if parents else None
        if parent is not None:
            nodes[parent]['self'] -= count
        nodes.append(dict(depth=depth, self=count, name=name, parent=parent))
        parents.append(len(nodes) - 1)
    total = sum(n['self'] for n in nodes)
    phases, copies = collections.Counter(), collections.Counter()
    utf8 = 0
    for i, n in enumerate(nodes):
        if not n['self']:
            continue
        assert n['self'] > 0
        ancestry = []
        while i is not None:
            ancestry.append(nodes[i]['name'])
            i = nodes[i]['parent']
        phase = 'Setup / other'
        for frame in ancestry:
            if 'preprocess::scan' in frame or '::scan_file' in frame:
                phase = 'Token scanning'
            elif '::expand' in frame or '::paste' in frame or 'preprocess::arguments' in frame:
                phase = 'Macro expansion'
            elif '::emit' in frame or '::flush' in frame:
                phase = 'Output / flush overhead'
            elif '::find_header' in frame:
                phase = 'Header search'
            elif '::canonical_path' in frame:
                phase = 'Path canonicalization'
            elif 'preprocess::FileSource' in frame:
                phase = 'Source loading'
            elif '::file' in frame and 'Preprocessor' in frame:
                phase = 'File / directive processing'
            else:
                continue
            break
        phases[phase] += n['self']
        if any('memmove' in frame for frame in ancestry):
            copies[phase] += n['self']
        if any('from_utf8' in frame for frame in ancestry):
            utf8 += n['self']
    assert sum(phases.values()) == total
    return {'exclusive_phase_percent': {k: 100*v/total for k, v in phases.most_common()},
            'memmove_context_percent': {k: 100*v/total for k, v in copies.most_common()},
            'utf8_validation_percent': 100*utf8/total}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=pathlib.Path, required=True)
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    source = json.loads(args.profile.read_text())
    binary = args.binary.resolve()
    report = {
        'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'host': platform.platform(),
        'binary': {'path': str(binary), 'sha256': sha(binary)},
        'source_sha256': sha(pathlib.Path('crates/keel-c/src/preprocess.rs')),
        'script_sha256': sha(pathlib.Path(__file__)),
        'corpus_sha256': sha(args.profile),
        'method': 'Serial warm-cache preprocessor library calls, exact original raw corpus and options. Seven timing rounds after one whole-application warmup. Input reads and returned output destruction are outside latency. Separate six-second macOS /usr/bin/sample captures at 1ms requested interval after warmup, with no concurrent benchmark/build. Inclusive stack percentages overlap and are sample shares, not isolated phase timings. Symbols include optimized inlining effects. Caches reset per translation unit. Prior pre-fast-path survey, where available, was three seconds and is directional only.',
        'phase_attribution': 'Exclusive categories assign each leaf sample to its closest recognizable preprocessor operation in the stack. Optimized inline frames may be absent; output is grouped with flush overhead. Memory-copy and UTF-8 costs are inclusive sample shares and overlap with operation categories.',
        'applications': {},
    }
    for app, config in source['preprocessing'].items():
        for entry in config['sources']:
            assert sha(pathlib.Path(entry['path'])) == entry['sha256']
        base = config['prepare_command']
        command = [str(binary), 'time', '7', *base[3:]]
        timed = subprocess.run(command, capture_output=True, text=True, check=True)
        rows = [json.loads(line) for line in timed.stdout.splitlines()]
        sample_command = [str(binary), 'sample', '100000', *base[3:]]
        proc = subprocess.Popen(sample_command, stdout=subprocess.DEVNULL,
                                stderr=subprocess.PIPE, text=True)
        path = args.output.with_name(args.output.stem + '-' + app + '.sample.txt')
        try:
            while True:
                line = proc.stderr.readline()
                if 'ready:' in line:
                    break
                if not line:
                    raise RuntimeError('profiling process exited before warmup completed')
            sampler = ['/usr/bin/sample', str(proc.pid), '6', '1', '-fullPaths', '-file', str(path.resolve())]
            subprocess.run(sampler, capture_output=True, text=True, check=True)
        finally:
            proc.terminate()
            proc.wait()
        stats = stack_summary(path)
        stats.update(stack_details(path))
        previous = pathlib.Path(f'target/frontend-profile/arena-baseline/survey-{app}.sample.txt')
        report['applications'][app] = {'timing_command': command, 'timings': rows,
            'sample_command': sample_command, 'sampler_command': sampler,
            'sample_path': str(path), 'sample_sha256': sha(path), 'stack': stats,
            'prior_survey': stack_summary(previous) if previous.exists() else None}
        totals = [sum(row['preprocess_ms'] for row in rows if row['round'] == i) for i in range(7)]
        report['applications'][app]['timing_summary'] = {
            'round_totals_ms': totals, 'median_ms': statistics.median(totals), 'files': len(rows) // 7}
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(app, stats['samples'], {k: round(v, 1) for k, v in stats['category_percent'].items()}, flush=True)
        for frame in stats['top_self'][:9]:
            print(round(frame['percent'], 1), frame['function'], flush=True)
    assert sha(binary) == report['binary']['sha256']


if __name__ == '__main__':
    main()
