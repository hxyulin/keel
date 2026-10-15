"""Capture frontend stacks separately from latency, including optional SSA outlining."""
import argparse
import hashlib
import json
import pathlib
import subprocess

from summarize_c_frontend import stack_summary
from analyze_frontend_stages import attribution


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--binary', type=pathlib.Path, required=True)
    p.add_argument('--outlined', type=pathlib.Path)
    p.add_argument('--output', type=pathlib.Path, required=True)
    args = p.parse_args()
    source = json.loads(args.profile.read_text())
    jobs = [(app, False, 'production', args.binary.resolve()) for app in source['inputs']]
    jobs.append(('sqlite', True, 'production', args.binary.resolve()))
    if args.outlined:
        jobs += [(app, False, 'ssa-outline', args.outlined.resolve()) for app in ['quickjs','sqlite']]
    report = {'method': 'Serial six-second macOS samples at requested 1ms interval, after IR parity validation/warmup; loop includes compile and returned-module destruction. Separate from timing. The optional ssa-outline binary only adds inline(never) to find_preds for attribution; it is diagnostic, never a production speedup comparison.',
              'binary_sha256': sha(args.binary), 'sources': {str(p): sha(p) for p in pathlib.Path('crates/keel-c/src').glob('*.rs')}, 'stacks': {}}
    for app, debug, variant, binary in jobs:
        name = f'{app}-{variant}' + ('-debug' if debug else '')
        path = args.output.with_name(args.output.stem + '-' + name + '.sample.txt')
        command = [str(binary), 'sample', '100000', *[r['path'] for r in source['inputs'][app]]] + (['--debug'] if debug else [])
        proc = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        try:
            while True:
                line = proc.stderr.readline()
                if line.startswith('ready:'):
                    break
                if not line:
                    raise RuntimeError(f'early exit: {proc.poll()}')
            sampler = ['/usr/bin/sample', str(proc.pid),'6','1','-fullPaths','-file',str(path.resolve())]
            subprocess.run(sampler, check=True, capture_output=True, text=True)
        finally:
            proc.terminate(); proc.wait()
        details = stack_summary(path)
        details['attribution'] = attribution(path)
        report['stacks'][name] = {'command': command, 'sample_command': sampler,
            'binary_sha256': sha(binary), 'sample_path': str(path), 'sample_sha256': sha(path), **details}
        args.output.write_text(json.dumps(report, indent=2)+'\n')
        print(name, {k: round(v,1) for k,v in details['category_percent'].items()}, flush=True)


if __name__ == '__main__':
    main()
