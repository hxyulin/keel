#!/usr/bin/env python3
"""Native C correctness only; invoke under suite/fast_opt_locked.py.

Uses a fresh output directory, at most two compile jobs, no timed campaigns.
All command output and coverage failures are preserved in report.json.
"""
import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import platform
import re
import subprocess
import sys
import tarfile
import urllib.request

from corpus import generate

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(argv):
    argv = [str(a) for a in argv]
    try:
        p = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=600)
        return dict(argv=argv, returncode=p.returncode, stdout=p.stdout, stderr=p.stderr)
    except subprocess.TimeoutExpired as e:
        def decoded(s):
            return s.decode(errors='replace') if isinstance(s, bytes) else (s or '')
        return dict(argv=argv, returncode=None, timeout_seconds=600,
                    stdout=decoded(e.stdout), stderr=decoded(e.stderr))
    except OSError as e:
        return dict(argv=argv, returncode=None, failure='launch_error', stdout='', stderr=str(e))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    parser.add_argument('--keel', type=pathlib.Path, required=True)
    parser.add_argument('--archive', type=pathlib.Path,
                        help='Optional existing archive, still checksum verified')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    keel = args.keel.resolve()
    # Read only this workload's pinned fields; schema validation belongs to
    # keel-suite. Keep this helper usable with the host's Python 3.9 too.
    manifest = (HERE / 'suite.toml').read_text()
    fetch = {key: re.search(r'^' + key + r' = "([^"]+)"$', manifest, re.M).group(1)
             for key in ['url', 'sha256']}
    archive = out / '0.13.0.tar.gz'
    if args.archive:
        archive.write_bytes(args.archive.read_bytes())
    else:
        with urllib.request.urlopen(fetch['url']) as response:
            archive.write_bytes(response.read())
    if sha(archive) != fetch['sha256']:
        raise SystemExit('archive SHA-256 mismatch; refusing extraction')
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            dest = (out / member.name).resolve()
            if out not in dest.parents or not (member.isdir() or member.isfile()):
                raise SystemExit('refusing unsafe archive member: ' + member.name)
        tar.extractall(out)
    source = out / re.search(r'^dir = "([^"]+)"$', manifest, re.M).group(1)
    corpus = out / 'corpus'
    expected = generate(corpus)
    report = dict(route='native C: Keel preprocessing + frontend + native object emitter',
                  host=platform.platform(), source_revision=command(['git', 'rev-parse', 'HEAD']),
                  upstream_url=fetch['url'], archive_sha256=sha(archive),
                  keel_sha256=sha(keel), upstream_defines=[], jobs=2,
                  clang_version=command(['clang', '--version']), corpus=expected,
                  inputs={str(p): sha(p) for p in [source / 'src/yyjson.c',
                          source / 'src/yyjson.h', HERE / 'driver.c', HERE / 'corpus.py',
                          HERE / 'check.py', HERE / 'validate.py', HERE / 'suite.toml']},
                  configurations=[])
    report_path = out / 'report.json'

    def save():
        report_path.write_text(json.dumps(report, indent=2) + '\n')

    save()
    configs = [('clang-O0', ['clang', '-std=c11', '-O0', '-c']),
               ('clang-O2', ['clang', '-std=c11', '-O2', '-c'])]
    for tier in ['none', 'fast', 'full']:
        configs.append((f'keel-{tier}', [str(keel), 'cc', '--emit-object',
                        '--opt-tier', tier, '--isel-tier',
                        'baseline' if tier == 'none' else 'full', '--regalloc', 'full']))
    reference = None
    for name, prefix in configs:
        build = out / name
        build.mkdir()
        units = [source / 'src/yyjson.c', HERE / 'driver.c']
        objects = [build / f'{p.stem}.o' for p in units]
        commands = [prefix + ['-I', str(source / 'src'), str(p), '-o', str(obj)]
                    for p, obj in zip(units, objects)]
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(command, commands))
        row = dict(name=name, compile=results, status='compile_failed')
        report['configurations'].append(row)
        save()
        if any(r['returncode'] != 0 for r in results):
            print(f'{name}: compile_failed', flush=True)
            continue
        binary = build / 'yyjson-workload'
        row['link'] = command(['clang', *objects, '-lm', '-o', binary])
        row['status'] = 'link_failed'
        if row['link']['returncode'] == 0:
            row['binary_sha256'] = sha(binary)
            row['test'] = command([sys.executable, HERE / 'check.py', binary, corpus])
            row['status'] = 'test_failed'
            if row['test']['returncode'] == 0:
                if reference is None:
                    reference = row['test']['stdout']
                row['status'] = 'passed' if row['test']['stdout'] == reference else 'output_mismatch'
                # Exercise the actual benchmark commands once, without timing.
                row['workloads'] = []
                for case, rounds in [('api', 40), ('config', 120), ('telemetry', 40)]:
                    result = command([binary, corpus / f'{case}.json', str(rounds)])
                    want = {k: expected[case][k] for k in ['hash', 'nodes', 'string_bytes']}
                    want['rounds'] = rounds
                    try:
                        result['matches_oracle'] = (result['returncode'] == 0 and
                                                    json.loads(result['stdout']) == want)
                    except ValueError:
                        result['matches_oracle'] = False
                    if not result['matches_oracle']:
                        row['status'] = 'workload_failed'
                    row['workloads'].append(result)
        save()
        print(f'{name}: {row["status"]}', flush=True)
    report['status'] = ('passed' if all(r['status'] == 'passed'
                                      for r in report['configurations']) else 'coverage_blocked')
    save()
    print(report_path, flush=True)
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
