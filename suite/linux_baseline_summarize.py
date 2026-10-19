#!/usr/bin/env python3
"""Summarize linux_baseline_* evidence into Markdown tables (no new estimators).

Compile: per configuration, mode and application, each repetition's serial
per-TU wall/CPU times are summed; the table gives the median over measured
repetitions with min/max. Fixed-backing results are copied from the unchanged
analyze.py summary.json; launch results from linux_baseline_runtime.py.
"""
import argparse
import json
import pathlib
import statistics


def compile_table(raw):
    rows = [json.loads(x) for x in pathlib.Path(raw).read_text().splitlines()]
    meta = rows[0]
    sums = {}
    for r in rows[1:]:
        if r.get('phase') != 'measured':
            continue
        stage = r['mode'] if r['kind'] == 'compile' else 'asm'
        key = (r['config'], stage, r['app'], r['rep'])
        w, c = sums.get(key, (0.0, 0.0))
        sums[key] = (w + r['wall_ns'] / 1e9, c + r['user_s'] + r['sys_s'])
    configs = list(dict.fromkeys(k[0] for k in sums))
    apps = list(dict.fromkeys(k[2] for k in sums))
    out = {}
    for config in configs:
        for stage in ['S', 'asm', 'obj']:
            for app in apps:
                walls = [v[0] for k, v in sums.items() if k[:3] == (config, stage, app)]
                cpus = [v[1] for k, v in sums.items() if k[:3] == (config, stage, app)]
                if walls:
                    out[(config, stage, app)] = dict(median_s=statistics.median(walls), min_s=min(walls), max_s=max(walls),
                                                     cpu_median_s=statistics.median(cpus), n=len(walls))
    lines = [f"Compile host {meta['host']}, CPU {meta['cpu']}, {meta['warmups']} warmup + {meta['reps']} measured reps, serial.",
             '', '| config | app | codegen to .s (s) | shared assembler (s) | .s+asm total (s) | direct to .o (s) |',
             '|---|---|---|---|---|---|']
    for config in configs:
        for app in apps:
            s = out.get((config, 'S', app))
            a = out.get((config, 'asm', app))
            o = out.get((config, 'obj', app))
            fmt = lambda d: f"{d['median_s']:.3f} [{d['min_s']:.3f}–{d['max_s']:.3f}]" if d else 'n/a'
            total = f"{s['median_s'] + a['median_s']:.3f}" if s and a else 'n/a'
            lines.append(f'| {config} | {app} | {fmt(s)} | {fmt(a)} | {total} | {fmt(o)} |')
    return meta, out, '\n'.join(lines)


def page_geomeans(summary):
    """Descriptive per-arm geometric means of measured post-barrier wall time (ms)."""
    import math
    raw = pathlib.Path(summary).with_name('raw.jsonl')
    logs = {}
    for line in raw.read_text().splitlines():
        r = json.loads(line)
        if r.get('kind') == 'result' and r['phase'] == 'measured':
            logs.setdefault((r['workload'], r['arm']), []).append(math.log(r['wall_ns'] / 1e6))
    return {k: math.exp(statistics.mean(v)) for k, v in logs.items()}


def page_table(summary, label):
    s = json.loads(pathlib.Path(summary).read_text())
    g = page_geomeans(summary)
    lines = [f"**{label}** — status `{s['status']}`, {s['calls']} calls ({s['measured_calls']} measured), {s['physical_sets']} physical sets.",
             '', '| workload | A ms | B ms | N ms | N/sqrt(A·B) | 95% CI | control B/A | control CI | controls ok | placement range |', '|---|---|---|---|---|---|---|---|---|---|']
    for name, w in s['workloads'].items():
        e, c = w['estimates']['optimized'], w['estimates']['control']
        per = [v[layout]['optimized'] for v in w['per_set_and_layout'] for layout in ['sorted', 'shuffled']]
        lines.append(f"| {name} | {g[(name, 'A')]:.1f} | {g[(name, 'B')]:.1f} | {g[(name, 'N')]:.1f} | {e['ratio']:.3f} | [{e['conditional_95ci'][0]:.3f}, {e['conditional_95ci'][1]:.3f}] | "
                     f"{c['ratio']:.4f} | [{c['conditional_95ci'][0]:.4f}, {c['conditional_95ci'][1]:.4f}] | {w['controls_adequate']} | "
                     f"{min(per):.3f}–{max(per):.3f} |")
    return '\n'.join(lines)


def launch_table(summary):
    s = json.loads(pathlib.Path(summary).read_text())
    arms = s['arms']
    head = '| workload | ' + ' | '.join(f'{a} median ms' for a in arms) + ' | ' + ' | '.join(
        f'{arms[-1]}/{a}' for a in arms[:-1]) + ' |'
    lines = [f"Ordinary-launch timing on {s['host']}: {s['estimand']}.", '', head, '|' + '---|' * (1 + 2 * len(arms) - 1)]
    for name, w in s['workloads'].items():
        cells = [f"{w['stats'][a]['median_ms']:.1f}" for a in arms]
        ratios = [w['ratios'][f'{arms[-1]}/{a}'] for a in arms[:-1]]
        rcells = [f"{r['paired_geomean']:.3f} [{r['round_bootstrap_95ci'][0]:.3f}, {r['round_bootstrap_95ci'][1]:.3f}]" for r in ratios]
        lines.append(f'| {name} | ' + ' | '.join(cells + rcells) + ' |')
    return '\n'.join(lines)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compile', action='append', default=[])
    p.add_argument('--page', action='append', default=[], help='LABEL=summary.json')
    p.add_argument('--launch', action='append', default=[])
    args = p.parse_args()
    for raw in args.compile:
        print(compile_table(raw)[2], '\n')
    for spec in args.page:
        label, path = spec.rsplit('=', 1)
        print(page_table(path, label), '\n')
    for path in args.launch:
        print(launch_table(path), '\n')


if __name__ == '__main__':
    main()
