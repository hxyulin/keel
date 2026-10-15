"""Count frontend allocations separately from latency; collect process peak RSS."""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--baseline', type=pathlib.Path, required=True)
    p.add_argument('--candidate', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--candidate-name', default='arena')
    args = p.parse_args()
    source = json.loads(args.profile.read_text())
    report = {'method': 'Identical TLS counting allocators, enabled only inside public compile; returned module destruction and verification excluded. One untimed whole-application warmup. RSS is a single full counting-process high-water mark, including warmup and resident raw preprocessed inputs; not isolated-call memory. Counters are never used for latency.',
              'script_sha256': sha(pathlib.Path(__file__)), 'binaries': {}, 'applications': {}}
    for name, binary in [('baseline', args.baseline.resolve()), (args.candidate_name, args.candidate.resolve())]:
        report['binaries'][name] = {'path': str(binary), 'sha256': sha(binary)}
        for app, inputs in source['inputs'].items():
            for debug in [False, True]:
                command = ['/usr/bin/time', '-l', str(binary), *[row['path'] for row in inputs]] + (['--debug'] if debug else [])
                result = subprocess.run(command, capture_output=True, text=True, check=True)
                rows = [json.loads(line) for line in result.stdout.splitlines()]
                peak = re.search(r'(\d+)\s+maximum resident set size', result.stderr)
                assert peak and len(rows) == len(inputs)
                item = {'command': command, 'files': rows, 'peak_rss_bytes': int(peak[1]),
                        'totals': {k: sum(x[k] for x in rows) for k in ['alloc','realloc','dealloc','requested_bytes','functions']}}
                report['applications'].setdefault(f'{app}:{debug}', {})[name] = item
                print(app, debug, name, 'alloc calls', item['totals']['alloc']+item['totals']['realloc'], 'RSS MiB', item['peak_rss_bytes']/2**20, flush=True)
    for data in report['applications'].values():
        a,b = data['baseline'],data[args.candidate_name]
        assert a['totals']['functions'] == b['totals']['functions']
        data['allocation_call_reduction'] = 1-(b['totals']['alloc']+b['totals']['realloc'])/(a['totals']['alloc']+a['totals']['realloc'])
        data['requested_bytes_reduction'] = 1-b['totals']['requested_bytes']/a['totals']['requested_bytes']
    args.output.write_text(json.dumps(report, indent=2)+'\n')


if __name__ == '__main__':
    main()
