#!/usr/bin/env python3
"""Linux GCC/Clang/Keel runtime: fixed-backing configs and normal-launch timing.

`pagecfg` writes a config for the unchanged pattern-lab/page-backing/run.py
from linux_baseline_build.py's link and correctness records. That runner has
three arms (control A, control B, optimized), so each family pairs two
byte-identical, independently linked copies of one reference compiler's build
(A/B) with one compared build (N): its analysis gives N/sqrt(A*B) directly.

`launch` is ordinary, unprivileged process timing: wall time from spawn to
reap, normal ASLR and page backing, one pinned CPU, rotated arm order per
round, every output checked. On the x86-64 Linux host it is known to be confounded by
executable page backing (docs/executable-page-backing.md); keep it separate
from controlled results.
"""
import argparse
import hashlib
import json
import math
import os
import pathlib
import random
import resource
import statistics
import subprocess
import time

ENV = dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C', TZ='UTC')


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def load(build):
    build = pathlib.Path(build).resolve()
    links = json.loads((build / 'link.json').read_text())
    check = json.loads((build / 'correctness.json').read_text())
    if not check['passed']:
        raise SystemExit('correctness did not pass')
    paths = {}
    for r in links:
        p = build / 'bin' / r['config'] / r['copy'] / pathlib.Path(r['argv'][r['argv'].index('-o') + 1]).name
        if sha(p) != r['sha256']:
            raise SystemExit(f'binary changed: {p}')
        paths[(r['config'], r['app'], r['copy'])] = dict(path=str(p), sha256=r['sha256'])
    return build, paths, check['workloads']


def workload_files(spec, root):
    inputs = [dict(path=a, sha256=sha(a)) for a in spec['argv'][1:] if a.startswith('/') and pathlib.Path(a).is_file()]
    stdin = dict(path=spec['stdin'], sha256=sha(spec['stdin'])) if spec['stdin'] else None
    if stdin:
        inputs.append(stdin)
    return inputs, stdin


def cmd_pagecfg(args):
    build, paths, workloads = load(args.build)
    root = pathlib.Path(args.root).resolve()
    out = []
    for name in args.workloads.split(','):
        spec = workloads[name]
        app = spec['app']
        binaries = dict(A=paths[(args.control, app, 'A')], B=paths[(args.control, app, 'B')],
                        N=paths[(args.compared, app, 'A')])
        if binaries['A']['sha256'] != binaries['B']['sha256'] or binaries['A']['path'] == binaries['B']['path']:
            raise SystemExit('controls must be distinct byte-identical files')
        inputs, stdin = workload_files(spec, root)
        out.append(dict(id=f'{args.compared}-vs-{args.control}/{name}', binaries=binaries,
                        argv=[binaries['A']['path'], *spec['argv'][1:]], cwd=str(root),
                        environment=dict(ENV, HOME=args.home, USER=args.user, LOGNAME=args.user),
                        inputs=inputs, expected=dict(path=spec['expected'], sha256=spec['expected_sha256']),
                        stdin=stdin))
    config = dict(uid=args.uid, cpu=args.cpu, controller_cpu=args.controller_cpu, expected_base=args.expected_base,
                  clang=args.clang, pool_directory=args.pool_directory, groups=args.groups, blocks=args.blocks,
                  repetitions=1, seed=args.seed, arms=['A', 'B', 'N'], budget_seconds=args.budget, workloads=out)
    pathlib.Path(args.output).write_text(json.dumps(config, indent=2) + '\n')
    print('wrote', args.output, len(out), 'workloads')


def cmd_launch(args):
    build, paths, workloads = load(args.build)
    root = pathlib.Path(args.root).resolve()
    arms = args.configs.split(',')
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw = (out / 'launch-raw.jsonl').open('x')
    os.sched_setaffinity(0, {args.controller_cpu})
    rng = random.Random(args.seed)
    names = args.workloads.split(',')
    raw.write(json.dumps(dict(kind='meta', host=os.uname().nodename, cpu=args.cpu, controller_cpu=args.controller_cpu,
                              arms=arms, binaries={f'{c}/{a}': paths[(c, a, 'A')] for c in arms for a in ['lua', 'quickjs', 'sqlite']},
                              warmups=args.warmups, reps=args.reps, seed=args.seed, env=ENV,
                              aslr=pathlib.Path('/proc/sys/kernel/randomize_va_space').read_text().strip(),
                              estimand='spawn-to-reap wall time of an ordinary launch (includes exec, dynamic loading, startup, workload, output)')) + '\n')
    for name in names:
        spec = workloads[name]
        expected = pathlib.Path(spec['expected']).read_bytes()
        stdin = pathlib.Path(spec['stdin']).read_bytes() if spec['stdin'] else None
        for round_ in range(args.warmups + args.reps):
            order = arms[:]
            rng.shuffle(order)
            for arm in order:
                exe = paths[(arm, spec['app'], 'A')]['path']
                argv = [exe, *spec['argv'][1:]]
                before = resource.getrusage(resource.RUSAGE_CHILDREN)
                start = time.perf_counter_ns()
                p = subprocess.Popen(argv, cwd=root, env=ENV, stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     preexec_fn=lambda: os.sched_setaffinity(0, {args.cpu}))
                so, se = p.communicate(stdin, timeout=120)
                wall = time.perf_counter_ns() - start
                after = resource.getrusage(resource.RUSAGE_CHILDREN)
                row = dict(kind='result', workload=name, arm=arm, round=round_,
                           phase='warmup' if round_ < args.warmups else 'measured', wall_ns=wall,
                           user_s=after.ru_utime - before.ru_utime, sys_s=after.ru_stime - before.ru_stime,
                           rc=p.returncode, stdout_ok=so == expected, stderr_bytes=len(se))
                raw.write(json.dumps(row) + '\n')
                raw.flush()
                if p.returncode or so != expected or se:
                    raise SystemExit(f'output mismatch {name} {arm}: rc={p.returncode} {se[-300:]!r}')
        print('done', name, flush=True)
    raw.close()
    summary = summarize_launch(out / 'launch-raw.jsonl', args.seed)
    (out / 'launch-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


def percentile(values, q):
    values = sorted(values)
    i = (len(values) - 1) * q
    lo = int(i)
    hi = min(lo + 1, len(values) - 1)
    return values[lo] + (values[hi] - values[lo]) * (i - lo)


def summarize_launch(raw, seed):
    rows = [json.loads(x) for x in pathlib.Path(raw).read_text().splitlines()]
    meta = rows[0]
    measured = [r for r in rows if r['kind'] == 'result' and r['phase'] == 'measured']
    arms = meta['arms']
    result = {}
    for name in dict.fromkeys(r['workload'] for r in measured):
        rounds = {}
        for r in measured:
            if r['workload'] == name:
                rounds.setdefault(r['round'], {})[r['arm']] = r['wall_ns'] / 1e6
        per = {a: [rounds[k][a] for k in sorted(rounds)] for a in arms}
        stats = {a: dict(median_ms=statistics.median(v), mean_ms=statistics.mean(v), min_ms=min(v), max_ms=max(v),
                         stdev_ms=statistics.stdev(v) if len(v) > 1 else 0.0, n=len(v)) for a, v in per.items()}
        ratios = {}
        rng = random.Random(seed)
        for num in arms:
            for den in arms:
                if num == den:
                    continue
                logs = [math.log(rounds[k][num] / rounds[k][den]) for k in sorted(rounds)]
                draws = []
                for _ in range(5000):
                    sample = [logs[rng.randrange(len(logs))] for _ in logs]
                    draws.append(math.exp(statistics.mean(sample)))
                ratios[f'{num}/{den}'] = dict(paired_geomean=math.exp(statistics.mean(logs)),
                                              round_bootstrap_95ci=[percentile(draws, .025), percentile(draws, .975)],
                                              median_ratio=stats[num]['median_ms'] / stats[den]['median_ms'])
        result[name] = dict(stats=stats, ratios=ratios)
    return dict(host=meta['host'], estimand=meta['estimand'], arms=arms, workloads=result,
                limits='ordinary launch; normal ASLR and page-cache backing; rounds are the only resampling unit')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='cmd', required=True)
    c = sub.add_parser('pagecfg')
    c.add_argument('--build', required=True)
    c.add_argument('--root', required=True)
    c.add_argument('--output', required=True)
    c.add_argument('--control', required=True)
    c.add_argument('--compared', required=True)
    c.add_argument('--workloads', required=True)
    c.add_argument('--uid', type=int, required=True)
    c.add_argument('--user', required=True)
    c.add_argument('--home', required=True)
    c.add_argument('--cpu', type=int, default=0)
    c.add_argument('--controller-cpu', type=int, default=15)
    c.add_argument('--expected-base', type=int, default=0x555555554000)
    c.add_argument('--clang', default='/usr/bin/clang')
    c.add_argument('--pool-directory', default='/tmp')
    c.add_argument('--groups', type=int, default=8)
    c.add_argument('--blocks', type=int, default=6)
    c.add_argument('--seed', type=int, required=True)
    c.add_argument('--budget', type=int, default=3600)
    l = sub.add_parser('launch')
    l.add_argument('--build', required=True)
    l.add_argument('--root', required=True)
    l.add_argument('--out', required=True)
    l.add_argument('--configs', default='gcc-O2,clang-O2,keel-O')
    l.add_argument('--workloads', required=True)
    l.add_argument('--cpu', type=int, required=True)
    l.add_argument('--controller-cpu', type=int, required=True)
    l.add_argument('--warmups', type=int, default=2)
    l.add_argument('--reps', type=int, default=15)
    l.add_argument('--seed', type=int, default=20261019)
    s = sub.add_parser('summarize-launch')
    s.add_argument('raw')
    s.add_argument('--seed', type=int, default=20261019)
    args = p.parse_args()
    if args.cmd == 'summarize-launch':
        print(json.dumps(summarize_launch(args.raw, args.seed), indent=2))
    else:
        dict(pagecfg=cmd_pagecfg, launch=cmd_launch)[args.cmd](args)


if __name__ == '__main__':
    main()
