#!/usr/bin/env python3
"""Time paired captured-input kernel compilation, excluding preprocessing and host tools."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import resource
import shlex
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs(manifest):
    rows = [row for row in manifest['inputs'] if row['scope'] == 'kernel64']
    generated = {str(Path(row['path']).with_suffix('.s')) for row in rows if row['kind'] == 'c'}
    return [row for row in rows if row['kind'] == 'c' or row['path'] not in generated]


def gcc_command(row, source, build, output, level):
    original = shlex.split(row['command'].split(';', 1)[0].replace('<source>', str(source)))
    result = ['gcc']
    index = 1
    while index < len(original):
        token = original[index]
        if token == '-o':
            index += 2
            continue
        if token == '-c' or token.startswith(('-O', '-save-temps', '-Wp,-MMD')):
            index += 1
            continue
        if token.endswith(('.c', '.S')) and not token.startswith('-'):
            index += 1
            continue
        result.append(token)
        index += 1
    return result + [level, '-c', str(build / row['path']), '-o', str(output)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['manifest', 'source', 'build', 'keel', 'out']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    if args.jobs < 1 or args.repeats < 3:
        parser.error('Positive jobs and at least three measured repeats are required')
    for name in ['manifest', 'source', 'build', 'keel', 'out']:
        setattr(args, name, getattr(args, name).resolve())
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(args.manifest.read_text())
    rows = inputs(manifest)
    if sha(args.build / '.config') != manifest['config_sha256']:
        raise SystemExit('Changed kernel configuration')
    for row in rows:
        if sha(args.build / row['path']) != row['sha256']:
            raise SystemExit('Changed captured input: ' + row['path'])
    policy_path = Path(__file__).with_name('kernel-audit-policy.json')
    policy = json.loads(policy_path.read_text())
    report = {'schema': 1, 'scope': 'captured preprocessed C plus handwritten assembly to objects; excludes preprocessing, host tools, objtool, archives, linking and packaging',
              'config_sha256': manifest['config_sha256'], 'manifest_sha256': sha(args.manifest),
              'keel_sha256': sha(args.keel), 'policy_sha256': sha(policy_path),
              'gcc': subprocess.check_output(['gcc', '--version'], text=True).splitlines()[0],
              'jobs': args.jobs, 'repeats': args.repeats, 'warmups_per_configuration': 1,
              'input_count': len(rows), 'c_inputs': sum(row['kind'] == 'c' for row in rows),
              'load_start': os.getloadavg(), 'samples': []}
    configs = [(compiler, level) for level in ['-O2', '-Os'] for compiler in ['gcc', 'keel']]

    def save():
        (args.out / 'results.json').write_text(json.dumps(report, indent=2) + '\n')

    digests = {}
    for repeat in range(-1, args.repeats):
        order = configs if repeat % 2 == 0 else list(reversed(configs))
        for compiler, level in order:
            label = compiler + '-' + level[1:]
            directory = args.out / 'objects' / label
            directory.mkdir(parents=True, exist_ok=True)
            sample = {'compiler': compiler, 'level': level, 'repeat': repeat, 'phases': {}}
            commands = []

            def compile_one(row):
                output = directory / Path(row['path']).with_suffix('.o')
                output.parent.mkdir(parents=True, exist_ok=True)
                output.unlink(missing_ok=True)
                if compiler == 'gcc':
                    command = gcc_command(row, args.source, args.build, output, level)
                else:
                    command = [str(args.keel), 'cc' if row['kind'] == 'c' else 'asm', '--target', 'x86_64-unknown-linux-gnu']
                    if row['kind'] == 'c':
                        command += [level, '--emit-object', '--cpu-level', 'v1', '--code-model', 'kernel', '--link-intent', 'executable', '-fno-asynchronous-unwind-tables', '-fno-unwind-tables']
                    else:
                        command += ['--x86-syntax', 'att', '--cpu-level', 'v3']
                    features = policy[row['kind'] + '_target_features']
                    if features:
                        command += ['--target-features', ','.join('+' + feature for feature in features)]
                    command += [str(args.build / row['path']), '-o', str(output)]
                result = subprocess.run(command, cwd=args.build, capture_output=True, timeout=180)
                return {'path': row['path'], 'command': command, 'returncode': result.returncode,
                        'diagnostic': result.stderr.decode(errors='replace')}

            for kind in ['c', 'asm']:
                subset = [row for row in rows if row['kind'] == kind]
                cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
                start = time.perf_counter()
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                    results = list(pool.map(compile_one, subset))
                seconds = time.perf_counter() - start
                cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)
                sample['phases'][kind] = {'wall_seconds': seconds, 'cpu_seconds': cpu_after.ru_utime + cpu_after.ru_stime - cpu_before.ru_utime - cpu_before.ru_stime}
                commands.extend(results)
                failures = [result for result in results if result['returncode'] != 0]
                if failures:
                    report['failure'] = {'compiler': compiler, 'level': level, 'repeat': repeat, 'inputs': failures}
                    save()
                    raise SystemExit(json.dumps(report['failure'], indent=2))
            digest = hashlib.sha256()
            for row in rows:
                output = directory / Path(row['path']).with_suffix('.o')
                digest.update(row['path'].encode() + b'\0' + bytes.fromhex(sha(output)))
            sample['objects_sha256'] = digest.hexdigest()
            if label in digests and digests[label] != sample['objects_sha256']:
                raise RuntimeError('Nondeterministic objects: ' + label)
            digests[label] = sample['objects_sha256']
            sample['wall_seconds'] = sum(phase['wall_seconds'] for phase in sample['phases'].values())
            (args.out / (label + '-commands.json')).write_text(json.dumps(commands, indent=2) + '\n')
            report['samples'].append(sample)
            save()
            print(label, repeat, round(sample['wall_seconds'], 3), flush=True)
    report['load_end'] = os.getloadavg()
    save()


if __name__ == '__main__':
    main()
