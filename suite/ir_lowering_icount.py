#!/usr/bin/env python3
"""Retired-instruction A/B for probe binaries, under the shared perf lock.

  python3 suite/ir_lowering_icount.py --inputs LIST.txt --output OUT.json \\
      [--reps 3] [--rounds 2] [--debug] [--opt] NAME=PROBE NAME=PROBE ...

Each PROBE is an ir_lowering_probe build (see the IR-lowering reports under
target/ir-lowering-*/scripts/ir_lowering_probe.rs) supporting the modes
`parseonly`, `front` (lex+parse+irgen+verify+IR drop), `pipe` (front +
codegen fast + codegen full + assemble) and optionally `opt` (front + opt
basic and opt full on clones). `/usr/bin/time -l` reports retired
instructions, cycles and max RSS; derived parts are
  irgen+verify = front - parseonly, backend = pipe - front,
  opt basic+full = opt - front.
Instruction counts barely move with machine load, unlike wall or cycles,
which makes them the primary A/B signal on the shared machine. Report
cycles beside them, and do not claim time savings from instructions alone.
Holds an exclusive fcntl lock on /tmp/keel-perf-20261019.lock throughout.
"""
import argparse, fcntl, json, os, re, statistics, subprocess

LOCK = '/tmp/keel-perf-20261019.lock'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--inputs', required=True, help='file listing .i paths')
    ap.add_argument('--output', required=True)
    ap.add_argument('--reps', type=int, default=3)
    ap.add_argument('--rounds', default='2')
    ap.add_argument('--debug', action='store_true')
    ap.add_argument('--opt', action='store_true')
    ap.add_argument('probes', nargs='+', metavar='NAME=PROBE')
    a = ap.parse_args()
    probes = dict(p.split('=', 1) for p in a.probes)
    inputs = open(a.inputs).read().split()
    modes = ['parseonly', 'front', 'pipe'] + (['opt'] if a.opt else [])

    def run(probe, mode):
        cmd = ['/usr/bin/time', '-l', probe, mode, a.rounds] + (['--debug'] if a.debug else []) + inputs
        p = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, check=True)
        g = lambda k: int(re.search(r'(\d+)\s+' + k, p.stderr).group(1))
        return dict(instructions=g('instructions retired'), cycles=g('cycles elapsed'),
                    maxrss=g('maximum resident set size'))

    res = {n: {m: [] for m in modes} for n in probes}
    with open(LOCK, 'a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        load = [os.getloadavg()]
        for _ in range(a.reps):
            for mode in modes:
                for name, probe in probes.items():  # interleaved per mode
                    res[name][mode].append(run(probe, mode))
        load.append(os.getloadavg())
    summary = {}
    for name in probes:
        m = {mode: {k: statistics.median(r[k] for r in rs) for k in rs[0]} for mode, rs in res[name].items()}
        m['irgen+verify'] = {k: m['front'][k] - m['parseonly'][k] for k in ('instructions', 'cycles')}
        m['backend'] = {k: m['pipe'][k] - m['front'][k] for k in ('instructions', 'cycles')}
        if a.opt:
            m['opt basic+full'] = {k: m['opt'][k] - m['front'][k] for k in ('instructions', 'cycles')}
        summary[name] = m
    json.dump(dict(raw=res, summary=summary, args=vars(a), loadavg=load), open(a.output, 'w'), indent=1)
    names = list(probes)
    base = names[0]
    print(f'rounds={a.rounds} (+1 warm) debug={a.debug} reps={a.reps} load={load}; millions')
    for part in ['parseonly', 'irgen+verify', 'backend', 'pipe'] + (['opt basic+full'] if a.opt else []):
        for k in ('instructions', 'cycles'):
            row = []
            for n in names:
                v, b = summary[n][part][k], summary[base][part][k]
                row.append(f'{v / 1e6:10.1f}' + ('' if n == base else f' ({100 * (v - b) / b:+.2f}%)'))
            print(f'{part:14s} {k:12s} ' + '  '.join(row))


if __name__ == '__main__':
    main()
