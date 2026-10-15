"""Measure arena/cache counters, allocations and counting-harness peak RSS.

This is separate from latency timing; allocator instrumentation is never timed.
Both counting binaries must use the identical allocation-counting example.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=pathlib.Path, required=True)
    parser.add_argument('--baseline', type=pathlib.Path, required=True)
    parser.add_argument('--candidate', type=pathlib.Path, required=True)
    parser.add_argument('--profiler', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    profile = json.loads(args.profile.read_text())
    report = {'method': 'Allocation counters are enabled only across preprocess, with one warmup per file. Identical TLS counting allocator in both binaries; neither binary is used for timing. macOS /usr/bin/time -l peak RSS covers the full counting-harness process (including warmups and raw input reading); a single run per application/configuration. Requested bytes include realloc sizes and exclude file mappings; peak RSS includes resident mapped pages. Candidate file/arena counters use preprocess_with_stats, which is a separate monomorphization; ordinary timings compile these counters away. Caches reset on every preprocess call, including between translation units.',
              'script_sha256': sha(pathlib.Path(__file__)), 'binaries': {}, 'applications': {}}
    for name, binary in [('baseline', args.baseline), ('candidate', args.candidate)]:
        report['binaries'][name] = {'path': str(binary.resolve()), 'sha256': sha(binary)}
        for app, row in profile['preprocessing'].items():
            base = row['prepare_command']
            command = ['/usr/bin/time', '-l', str(binary.resolve()), base[3], base[4], *base[6:]]
            result = subprocess.run(command, capture_output=True, text=True, check=True)
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            peak = re.search(r'(\d+)\s+maximum resident set size', result.stderr)
            if not peak:
                raise RuntimeError(result.stderr)
            report['applications'].setdefault(app, {})[name] = {
                'command': command, 'files': rows, 'time_stderr': result.stderr,
                'peak_rss_bytes': int(peak[1]),
                'totals': {key: sum(r[key] for r in rows) for key in ['alloc', 'realloc', 'dealloc', 'requested_bytes', 'output_bytes']}}
            print(app, name, 'calls', sum(r['alloc'] + r['realloc'] for r in rows), 'peak RSS MiB', round(int(peak[1]) / 2**20, 2), flush=True)
    report['binaries']['stats_profiler'] = {'path': str(args.profiler.resolve()), 'sha256': sha(args.profiler)}
    for app, row in profile['preprocessing'].items():
        command = [str(args.profiler.resolve()), 'stats', '1', *row['prepare_command'][3:]]
        result = subprocess.run(command, capture_output=True, text=True, check=True)
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        data = report['applications'][app]
        data['cache_stats'] = {'command': command, 'files': rows,
                               'totals': {k: sum(r[k] for r in rows) for k in rows[0] if k != 'path'},
                               'max_tu_arena_bytes': max(r['arena_bytes'] for r in rows),
                               'max_tu_token_bytes': max(r['token_bytes'] for r in rows)}
        a, b = data['baseline']['totals'], data['candidate']['totals']
        assert a['output_bytes'] == b['output_bytes'] == sum(r['output_bytes'] for r in rows)
        data['allocation_call_reduction'] = 1 - (b['alloc'] + b['realloc']) / (a['alloc'] + a['realloc'])
        data['requested_bytes_reduction'] = 1 - b['requested_bytes'] / a['requested_bytes']
        data['peak_rss_reduction'] = 1 - data['candidate']['peak_rss_bytes'] / data['baseline']['peak_rss_bytes']
        print(app, data['cache_stats']['totals'], flush=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
