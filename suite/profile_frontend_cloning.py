"""Linux-only, serial baseline refresh and diagnostic Expr clone attribution.

Instrument only an isolated disposable source copy; never time that binary.
Build/freeze profile, profile_allocations, and clone_counts before measure.
"""
import argparse
import collections
import datetime
import hashlib
import json
import pathlib
import random
import re
import statistics
import subprocess
import time

from measure_preprocessor import interval


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv):
    p = subprocess.run(list(map(str, argv)), text=True, capture_output=True, check=True)
    return {'command': list(map(str, argv)), 'stdout': p.stdout, 'stderr': p.stderr}


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def load():
    return {'time': now(), 'uptime': run(['uptime']),
            'processes': run(['ps', '-eo', 'pid,ppid,pcpu,pmem,args', '--sort=-pcpu']),
            'vmstat': run(['vmstat', '1', '3'])}


def instrument(source):
    path = source / 'crates/keel-c/src/ast.rs'
    text = path.read_text()
    old = '#[derive(Clone, Debug)]\npub struct Expr'
    assert text.count(old) == 1
    text = text.replace(old, '#[derive(Debug)]\npub struct Expr', 1)
    text += r'''
// Diagnostic only: TLS counters attribute recursive nodes to each root caller.
// No counter binary is used for latency or allocation measurements.
type CloneKey = (&'static str, u32, u32);
#[derive(Default)]
struct CloneStats(std::collections::BTreeMap<CloneKey, (u64, u64)>);
impl Drop for CloneStats {
    fn drop(&mut self) {
        for ((file, line, col), (roots, nodes)) in &self.0 {
            eprintln!("CLONES {file}:{line}:{col} {roots} {nodes}");
        }
    }
}
thread_local! {
    static CLONE_STATS: std::cell::RefCell<CloneStats> = Default::default();
    static CLONE_ROOT: std::cell::Cell<Option<CloneKey>> = const { std::cell::Cell::new(None) };
}
impl Clone for Expr<'_> {
    #[track_caller]
    fn clone(&self) -> Self {
        let loc = std::panic::Location::caller();
        let previous = CLONE_ROOT.with(|r| r.get());
        let key = previous.unwrap_or((loc.file(), loc.line(), loc.column()));
        CLONE_ROOT.with(|r| r.set(Some(key)));
        CLONE_STATS.with(|s| {
            let mut s = s.borrow_mut();
            let n = s.0.entry(key).or_default();
            n.0 += u64::from(previous.is_none());
            n.1 += 1;
        });
        let result = Self {
            kind: self.kind.clone(), sugar: self.sugar.clone(),
            ty: self.ty.clone(), loc: self.loc,
        };
        CLONE_ROOT.with(|r| r.set(previous));
        result
    }
}
'''
    path.write_text(text)


def decode(data):
    # Disable inline expansion to retain qualified Rust physical symbols.
    # Count failed DWARF unwinds explicitly rather than discarding those events.
    decoded = run(['sudo', '-n', 'perf', 'script', '--no-inline', '-i', data])
    folded = collections.Counter()
    for sample in decoded['stdout'].strip().split('\n\n'):
        frames = [line.strip() for line in sample.splitlines()[1:] if line.strip()]
        folded[';'.join(frames) if frames else '[no unwind frames]'] += 1
    leaves = run(['sudo', '-n', 'perf', 'script', '--no-inline', '-G', '-F', 'ip,sym,dso', '-i', data])
    leaf_counts = collections.Counter(line.strip().split(' ', 1)[1] for line in leaves['stdout'].splitlines() if line.strip())
    assert sum(leaf_counts.values()) == sum(folded.values())
    return {'decode_command': decoded['command'], 'decode_stderr': decoded['stderr'],
            'samples': sum(folded.values()),
            'leaf_command': leaves['command'], 'leaf_counts': leaf_counts.most_common(),
            'no_unwind_samples': folded['[no unwind frames]'],
            'folded_stacks': sorted(folded.items(), key=lambda kv: -kv[1])}


def redecode(root):
    output = root / 'evidence/profile.json'
    report = json.loads(output.read_text())
    report['decode_script_sha256'] = sha(pathlib.Path(__file__))
    for key, result in report['stacks'].items():
        app, debug = key.split(':')
        result.update(decode(root / f'evidence/{app}-{debug}.perf.data'))
        expected = re.search(r'\((\d+) samples\)', result['record']['stderr'])
        assert result['samples'] == int(expected[1])
    output.write_text(json.dumps(report, indent=2) + '\n')


def measure(root, rounds, seconds, after_timing=False):
    evidence = root / 'evidence'
    evidence.mkdir(exist_ok=True)
    bins = {n: root / 'bin' / n for n in ['profile', 'profile_allocations', 'clone_counts']}
    inputs = {app: sorted((root / 'inputs' / app).glob('*.i')) for app in ['lua', 'quickjs', 'sqlite']}
    assert [len(v) for v in inputs.values()] == [33, 9, 2]
    report = {'base': '14a639b75f700b167ef3a4816443e57eba5be20e',
              'script_sha256': sha(pathlib.Path(__file__)), 'rounds': rounds,
              'binaries': {n: {'path': str(p), 'sha256': sha(p)} for n, p in bins.items()},
              'inputs': {a: [{'path': str(p), 'sha256': sha(p)} for p in ps] for a, ps in inputs.items()},
              'cargo_lock_sha256': sha(root / 'source/Cargo.lock'),
              'toolchain': run([pathlib.Path.home() / '.cargo/bin/rustc', '-Vv']),
              'validation': {}, 'samples': [], 'allocations': {}, 'clones': {}, 'stacks': {}}
    output = evidence / 'profile.json'
    if after_timing:
        previous = json.loads(output.read_text())
        for key in ['base', 'rounds', 'binaries', 'inputs', 'cargo_lock_sha256']:
            assert previous[key] == report[key], key
        assert len(previous['samples']) == rounds * 12
        report = previous
        report['postprocessing_script_sha256'] = sha(pathlib.Path(__file__))

    def save():
        output.write_text(json.dumps(report, indent=2) + '\n')

    # Byte-for-byte normal/debug snapshots compare diagnostic and production
    # binaries. Each helper also checks public/staged parity and verifies both.
    for app, paths in ([] if after_timing else inputs.items()):
        for debug in [False, True]:
            suffix = ['--debug'] if debug else []
            key = f'{app}:{debug}'
            snapshots = {}
            for name in ['profile', 'clone_counts']:
                out = evidence / 'ir' / name / key
                cmd = [bins[name], 'prepare', '1', out, *paths, *suffix]
                result = run(cmd)
                snapshots[name] = {p.name: sha(p) for p in sorted(out.glob('*.ir'))}
                report['validation'].setdefault(key, {})[name] = result
            assert snapshots['profile'] == snapshots['clone_counts']
            assert len(snapshots['profile']) == len(paths)
            report['validation'][key]['ir_sha256'] = snapshots['profile']
    if not after_timing:
        report['timing_start'] = load()
    save()
    if not after_timing:
        print('TIMING START', now(), flush=True)
    rng = random.Random(20261016)
    for index in range(0 if after_timing else rounds):
        jobs = [(a, d) for a in inputs for d in [False, True]]
        rng.shuffle(jobs)
        for app, debug in jobs:
            paths = inputs[app]
            shift = index % len(paths)
            paths = paths[shift:] + paths[:shift]
            # Alternate API ordering in each same-round pair.
            modes = ['baseline', 'staged'] if index % 2 == 0 else ['staged', 'baseline']
            for mode in modes:
                result = run([bins['profile'], mode, '1', *paths, *(['--debug'] if debug else [])])
                rows = [json.loads(s) for s in result.pop('stdout').splitlines()]
                report['samples'].append({'round': index, 'app': app, 'debug': debug,
                                          'mode': mode, 'total_ms': sum(r['total_ms'] for r in rows),
                                          'files': rows, **result})
        save()
        print('round', index + 1, flush=True)
    if not after_timing:
        report['timing_end'] = load()
        print('TIMING END', now(), flush=True)
    report['summaries'] = {}
    for app in inputs:
        for debug in [False, True]:
            groups = {m: [r for r in report['samples'] if r['app'] == app and r['debug'] == debug and r['mode'] == m]
                      for m in ['baseline', 'staged']}
            times = {m: [r['total_ms'] for r in rs] for m, rs in groups.items()}
            ratios = [a / b for a, b in zip(times['staged'], times['baseline'])]
            report['summaries'][f'{app}:{debug}'] = {
                'public_median_ms': statistics.median(times['baseline']),
                'public_median_ci95_ms': interval(times['baseline'], rng),
                'staged_public_paired_ratios': ratios,
                'staged_public_ratio_ci95': interval(ratios, rng),
                'stage_medians_ms': {k: statistics.median(sum(f[k] for f in r['files']) for r in groups['staged'])
                                     for k in ['lex_ms', 'parse_ms', 'irgen_ms', 'cleanup_ms']}}
    save()
    # Instrumentation and peak RSS are separate from latency.
    for app, paths in inputs.items():
        for debug in [False, True]:
            suffix = ['--debug'] if debug else []
            key = f'{app}:{debug}'
            # A fresh wrapper makes RUSAGE_CHILDREN specific to this one binary.
            wrapper = ('import resource,subprocess,sys; '
                       'p=subprocess.run(sys.argv[1:]); '
                       'print("PEAK_RSS_KIB",resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,file=sys.stderr); '
                       'sys.exit(p.returncode)')
            result = run(['python3', '-c', wrapper, bins['profile_allocations'], *paths, *suffix])
            rows = [json.loads(s) for s in result.pop('stdout').splitlines()]
            rss = re.search(r'PEAK_RSS_KIB (\d+)', result['stderr'])
            report['allocations'][key] = {'files': rows, 'peak_rss_bytes': int(rss[1]) * 1024,
                'totals': {k: sum(r[k] for r in rows) for k in ['alloc', 'realloc', 'dealloc', 'requested_bytes', 'functions']}, **result}
            result = run([bins['clone_counts'], 'baseline', '1', *paths, *suffix])
            # profile warms ordinary + staged once, then measures ordinary once:
            # all diagnostic totals cover THREE compilations of each input.
            counts = re.findall(r'^CLONES (.+) (\d+) (\d+)$', result['stderr'], re.M)
            report['clones'][key] = {'compile_passes': 3, 'callers': [
                {'caller': c, 'roots': int(r), 'nodes': int(n)} for c, r, n in counts], **result}
            save()
    # Attach only after helper warmup/IR verification. Production binary, with
    # returned-module destruction inside sample loop; no timing claims here.
    for app, paths in inputs.items():
        for debug in [False, True]:
            key = f'{app}:{debug}'
            cmd = [str(bins['profile']), 'sample', '100000', *map(str, paths), *(['--debug'] if debug else [])]
            proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            try:
                assert proc.stderr.readline().startswith('ready:')
                data = evidence / f'{app}-{debug}.perf.data'
                perf = ['sudo', '-n', 'perf', 'record', '-e', 'cpu-clock', '-F', '499',
                        '--call-graph', 'dwarf,16384', '-p', str(proc.pid), '-o', str(data), '--', 'sleep', str(seconds)]
                record = run(perf)
                report['stacks'][key] = {'command': cmd, 'record': record, **decode(data)}
            finally:
                proc.terminate()
                proc.wait()
            save()
    report['finished'] = load()
    for name, binary in bins.items():
        assert sha(binary) == report['binaries'][name]['sha256']
    save()


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['instrument', 'measure', 'redecode'])
    p.add_argument('--root', type=pathlib.Path, required=True)
    p.add_argument('--rounds', type=int, default=13)
    p.add_argument('--seconds', type=int, default=8)
    p.add_argument('--after-timing', action='store_true', help='Resume saved timings, rerun only untimed collection')
    a = p.parse_args()
    if a.action == 'instrument':
        instrument(a.root)
    elif a.action == 'redecode':
        redecode(a.root)
    else:
        assert a.rounds >= 5 and a.seconds >= 1
        measure(a.root.resolve(), a.rounds, a.seconds, a.after_timing)
