"""Count allocations separately from timings on the exact preprocessing corpus."""
import argparse
import hashlib
import json
import pathlib
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=pathlib.Path, required=True)
    parser.add_argument('--baseline', type=pathlib.Path, required=True)
    parser.add_argument('--candidate', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    profile = json.loads(args.profile.read_text())
    report = {'method': 'Single-thread TLS counting allocator forwards to System. One untimed warmup per file. Counters enabled only across preprocess, including transitive header IO and internal cleanup; returned output remains live. Instrumented binaries are never timed. requested_bytes sums requested allocation sizes and full new reallocation sizes; it is not peak live memory or RSS.',
              'script_sha256': sha(pathlib.Path(__file__)),
              'example_sha256': sha(pathlib.Path('crates/keel-c/examples/profile_preprocess_allocations.rs')),
              'binaries': {}, 'applications': {}}
    for name, binary in [('baseline', args.baseline), ('candidate', args.candidate)]:
        report['binaries'][name] = {'path': str(binary.resolve()), 'sha256': sha(binary)}
        for app, row in profile['preprocessing'].items():
            base = row['prepare_command']
            command = [str(binary.resolve()), base[3], base[4], *base[6:]]
            result = subprocess.run(command, capture_output=True, text=True, check=True)
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            report['applications'].setdefault(app, {})[name] = {
                'command': command, 'files': rows,
                'totals': {key: sum(r[key] for r in rows) for key in ['alloc', 'realloc', 'dealloc', 'requested_bytes', 'output_bytes']}}
            print(app, name, report['applications'][app][name]['totals'], flush=True)
    for app, row in report['applications'].items():
        a, b = row['baseline']['totals'], row['candidate']['totals']
        assert a['output_bytes'] == b['output_bytes']
        row['allocation_call_reduction'] = 1 - (b['alloc'] + b['realloc']) / (a['alloc'] + a['realloc'])
        row['requested_bytes_reduction'] = 1 - b['requested_bytes'] / a['requested_bytes']
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
