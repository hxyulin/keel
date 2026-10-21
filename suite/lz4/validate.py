#!/usr/bin/env python3
"""Local correctness only; invoke through suite/fast_opt_locked.py.

Uses two compile jobs and a private output/cache directory. Retains pristine
archive, extracted sources, preprocessed inputs, objects, binaries and JSON
command/results evidence. Benchmark modes are left to the parent runner.
"""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--keel', required=True, type=Path)
    parser.add_argument('--keel-source', default='unknown')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('-j', '--jobs', type=int, choices=[1, 2], default=2)
    args = parser.parse_args()
    suite = Path(__file__).resolve().parent
    manifest = tomllib.loads((suite / 'suite.toml').read_text())
    work = args.output.resolve()
    work.mkdir(parents=True, exist_ok=True)
    report = {'complete': False, 'jobs': args.jobs, 'commands': [], 'builds': {},
              'errors': [], 'keel_source': args.keel_source,
              'scope': 'correctness only; no benchmark or performance loops'}

    def save():
        (work / 'validation.json').write_text(json.dumps(report, indent=2) + '\n')

    def run(argv, cwd=None):
        argv = [str(a) for a in argv]
        result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=600)
        row = {'argv': argv, 'cwd': str(cwd) if cwd else None,
               'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
        report['commands'].append(row)
        if result.returncode:
            raise RuntimeError(json.dumps(row))
        return result.stdout

    try:
        clang = Path(shutil.which('clang')).resolve()
        keel = args.keel.resolve()
        report['compilers'] = {'clang': {'path': str(clang), 'sha256': sha(clang),
                                       'version': run([clang, '--version'])},
                               'keel': {'path': str(keel), 'sha256': sha(keel)}}
        expected = manifest['fetch'][0]['sha256']
        if sha(args.archive) != expected:
            raise RuntimeError('archive SHA-256 mismatch')
        archive = work / 'v1.10.0.tar.gz'
        if args.archive.resolve() != archive:
            shutil.copy2(args.archive, archive)
        report['archive'] = {'path': str(archive), 'sha256': sha(archive),
                             'url': manifest['fetch'][0]['url']}
        # The pinned official archive is verified before extracting it.
        with tarfile.open(archive) as tar:
            tar.extractall(work, filter='data')
        lib = work / manifest['build']['dir']
        report['upstream'] = {name: sha(lib / name) for name in ['lz4.c', 'lz4.h']}
        report['inputs'] = {name: sha(suite / name)
                            for name in ['suite.toml', 'driver.c', 'validate.py']}
        pre = work / 'pre'
        pre.mkdir(exist_ok=True)
        for source in [lib / 'lz4.c', suite / 'driver.c']:
            run([clang, '-E', '-std=c99', '-I', lib, source,
                 '-o', pre / (source.stem + '.i')])
        report['preprocessed'] = {p.name: sha(p) for p in sorted(pre.glob('*.i'))}
        configs = {'clang-O2': (clang, ['-O2']),
                   'keel-none': (keel, []),
                   'keel-fast': (keel, ['--opt-tier', 'fast', '--isel-tier', 'full',
                                         '--regalloc', 'full']),
                   'keel-full': (keel, ['-O'])}
        reference = None
        for name, (compiler, flags) in configs.items():
            dest = work / name
            dest.mkdir(exist_ok=True)
            commands = []
            for source in sorted(pre.glob('*.i')):
                obj = dest / (source.stem + '.o')
                if name == 'clang-O2':
                    argv = [compiler, '-c', *flags, source, '-o', obj]
                else:
                    argv = [compiler, 'cc', *flags, '--emit-object', source, '-o', obj]
                commands.append((argv, obj))

            def compile_one(command):
                argv, obj = command
                key = hashlib.sha256(json.dumps([list(map(str, argv)), sha(compiler),
                                                report['preprocessed']]).encode()).hexdigest()
                stamp = obj.with_suffix('.cache.json')
                if obj.exists() and stamp.exists():
                    cached = json.loads(stamp.read_text())
                    if cached == {'key': key, 'sha256': sha(obj)}:
                        return
                run(argv)
                stamp.write_text(json.dumps({'key': key, 'sha256': sha(obj)}) + '\n')

            try:
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                    # Consume every result, including independent compile failures.
                    futures = [pool.submit(compile_one, command) for command in commands]
                    failures = []
                    for future in futures:
                        try:
                            future.result()
                        except Exception as error:
                            failures.append(str(error))
                    if failures:
                        raise RuntimeError('\n'.join(failures))
                binary = dest / 'lz4-workload'
                run([clang, *[obj for _, obj in commands], '-o', binary])
                output = run([binary, 'test'])
                if reference is None:
                    reference = output
                if output != reference:
                    raise RuntimeError('correctness output differs from clang-O2: ' + output)
                report['builds'][name] = {'status': 'passed', 'output': output,
                                         'binary': str(binary), 'sha256': sha(binary)}
            except Exception as error:
                report['builds'][name] = {'status': 'failed', 'error': str(error)}
                report['errors'].append(name + ': ' + str(error))
            save()
        report['complete'] = len(report['builds']) == 4 and not report['errors']
    except Exception as error:
        report['errors'].append(str(error))
    save()
    print(json.dumps({'report': str(work / 'validation.json'),
                      'complete': report['complete'], 'builds': report['builds'],
                      'errors': report['errors']}, indent=2))
    return 0 if report['complete'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
