#!/usr/bin/env python3
"""Render the README benchmark charts from readme-benchmarks.json.

  python3 docs/assets/readme-render-charts.py                    # write SVGs
  python3 docs/assets/readme-render-charts.py --check --verify-evidence

--check fails if a committed SVG is out of date. --verify-evidence checks the
evidence file hashes and that every plotted value matches its JSON pointer.
Standard library only.
"""
import argparse
from decimal import Decimal, ROUND_HALF_UP
import hashlib
import html
import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

WIDTH, PAD, LABEL_W, VALUE_W = 720, 26, 152, 70
ROW, BAR = 36, 12
UNITS = {'ms': (1.0, 'ms'), 'us': (1000.0, 'µs')}

STYLE = """<style>
svg{--surface:#fff;--ink:#000;--ink-2:#525252;--grid:#e5e5e5;--keel:#0056d6;--other:#737373;--on-blue:#fff}
@media (prefers-color-scheme:dark){svg{--surface:#000;--ink:#fff;--ink-2:#a3a3a3;--grid:#262626;--keel:#5b9dff;--other:#737373;--on-blue:#000}}
text{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;fill:var(--ink)}
.muted{fill:var(--ink-2)}
.number{font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums}
</style>"""


def nice_step(limit, target=5):
    raw = limit / target
    base = 10 ** math.floor(math.log10(raw))
    return next(m * base for m in (1, 2, 2.5, 5, 10) if m * base >= raw)


def fmt(value):
    places = '1' if value >= 100 else '0.1' if value >= 10 else '0.01'
    return str(Decimal(str(value)).quantize(Decimal(places), rounding=ROUND_HALF_UP))


def tick(value):
    return f'{value:g}'


def render(data, chart):
    esc = html.escape
    ratio = chart.get('kind') == 'ratio'
    if ratio:
        rows = [(r['label'], r['value'], True) for r in chart['rows']]
        overall = math.exp(sum(math.log(v) for _, v, _ in rows) / len(rows))
        headline_text = f'{overall:.2f}×'
        caption = 'geometric mean'
        axis = f'{chart.get("metric", "Run time")} relative to {chart["reference"]} · lower is better'
        label_value = lambda v: f'{v:.2f}×'
        desc = '; '.join(f'{label}: {v:.2f}×' for label, v, _ in rows)
        if chart.get('summary', True):
            desc += f'. Overall {overall:.2f}× the run time of {chart["reference"]}.'
    else:
        scale, unit = UNITS[chart['unit']]
        rows = [(r['label'], r['median_ms'] * scale, r['keel']) for r in chart['rows']]
        by_label = {label: value for label, value, _ in rows}
        headline = chart['headline']
        speedup = by_label[headline['slow']] / by_label[headline['fast']]
        factor, word = (speedup, 'faster') if speedup >= 1 else (1 / speedup, 'slower')
        headline_text = f'{factor:.2f}× {word}'
        caption = f'{headline["fast"]} vs {headline["slow"]}'
        axis = f'Wall time ({unit}) · lower is better'
        label_value = lambda v: f'{fmt(v)} {unit}'
        desc = '; '.join(f'{label}: {fmt(v)} {unit}' for label, v, _ in rows)
        desc += f'. {headline["fast"]} is {factor:.1f}× {word} than {headline["slow"]}.'
    machine = chart['machine'] if 'machine' in chart else data['machine']
    statistic = chart['statistic'] if 'statistic' in chart else data['statistic']
    desc += f' {machine}, {statistic}.'

    values = [v for _, v, _ in rows]
    if ratio:
        low, high = min(1, *values), max(1, *values)
        step = nice_step(max(high - low, 0.1))
        start = max(0, step * math.floor((low - step / 3) / step))
        limit = step * math.ceil((high + step / 3) / step)
    else:
        start = 0
        step = nice_step(max(values))
        limit = step * math.ceil(max(values) / step)
    x0 = PAD + LABEL_W
    plot_w = WIDTH - x0 - PAD - VALUE_W - 12
    top = 104 if ratio else 86
    base = top + ROW * len(rows)
    height = base + 53

    def x_at(value):
        return x0 + plot_w * (value - start) / (limit - start)

    cid = chart['id']
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
           f'viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="{cid}-t {cid}-d">',
           STYLE,
           f'<title id="{cid}-t">{esc(chart["title"])} (lower is better)</title>',
           f'<desc id="{cid}-d">{esc(desc)}</desc>',
           f'<rect width="{WIDTH}" height="{height}" fill="var(--surface)"/>',
           f'<text x="{PAD}" y="44" font-size="21" font-weight="680">{esc(chart["title"])}</text>',
           f'<text class="muted" x="{PAD}" y="66" font-size="13">'
           f'{esc(machine)} · {esc(statistic)}</text>']
    if chart.get('summary', True):
        out.extend([
            f'<text class="number" x="{WIDTH - PAD}" y="43" font-size="23" font-weight="700" text-anchor="end">'
            f'{headline_text}</text>',
            f'<text class="muted" x="{WIDTH - PAD}" y="64" font-size="11" text-anchor="end">{esc(caption)}</text>',
        ])

    for i in range(round((limit - start) / step) + 1):
        value = start + i * step
        x = x_at(value)
        out.append(f'<path d="M{x:.1f} {top}V{base}" stroke="var(--grid)"/>')
        out.append(f'<text class="muted number" x="{x:.1f}" y="{base + 17}" font-size="12" '
                   f'text-anchor="middle">{tick(value)}{"×" if ratio else ""}</text>')
    out.append(f'<text class="muted" x="{x0 + plot_w / 2:.1f}" y="{base + 38}" font-size="13" '
               f'text-anchor="middle">{esc(axis)}</text>')
    if ratio:
        x = x_at(1)
        out.append(f'<path d="M{x:.1f} {top - 10}V{base}" stroke="var(--ink-2)" stroke-dasharray="3 3"/>')
        out.append(f'<text class="muted" x="{x:.1f}" y="{top - 15}" font-size="11" text-anchor="middle">'
                   f'{esc(chart["reference"])}</text>')

    for i, (label, v, keel) in enumerate(rows):
        y = top + i * ROW + ROW / 2
        x = x_at(v)
        origin = x_at(1) if ratio else x0
        weight = ' font-weight="600"' if keel else ''
        fill = 'var(--keel)' if keel else 'var(--other)'
        out.append(f'<text x="{PAD}" y="{y + 5:g}" font-size="15"{weight}>'
                   f'{esc(label)}</text>')
        out.append(f'<path d="M{origin:.1f} {y:g}H{x:.1f}" stroke="{fill}" '
                   f'stroke-width="{3 if ratio else BAR}"/>')
        if ratio:
            out.append(f'<circle cx="{x:.1f}" cy="{y:g}" r="5" fill="{fill}"/>')
        out.append(f'<text class="number" x="{WIDTH - PAD}" y="{y + 5:g}" font-size="14" '
                   f'text-anchor="end"{weight}>'
                   f'{label_value(v)}</text>')
    out.append('</svg>\n')
    return '\n'.join(out)


def render_pipeline():
    """Toolchain stages and the commands that run them."""
    esc = html.escape
    pad = 24
    stages = ['C frontend', 'Optimizer', 'Backend', 'Assembler', 'Linker']
    artifacts = ['IR', 'IR', 'asm', '.o']
    commands = [('keel cc', 0, 3), ('keel compile', 1, 3), ('keel opt', 1, 1),
                ('keel asm', 3, 3), ('keel link', 4, 4)]
    rows = [0, 1, 2, 2, 2]
    box_w, box_h, gap, x0, y0 = 92, 44, 38, 60, 30
    xs = [x0 + i * (box_w + gap) for i in range(len(stages))]
    height = y0 + box_h + 34 + 30 * (max(rows) + 1) + 6
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
           f'viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="pipeline-t pipeline-d">',
           STYLE.replace('</style>', 'rect.stage{fill:none;stroke:var(--ink-2);stroke-width:1.25}\n'
                         '.mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}\n</style>'),
           '<title id="pipeline-t">Keel toolchain stages</title>',
           '<desc id="pipeline-d">C source flows through the C frontend, optimizer, backend, assembler '
           'and linker to an executable. keel cc runs frontend to assembler; keel compile runs optimizer '
           'to assembler from Keel IR; keel opt runs the optimizer; keel asm assembles; keel link links.</desc>',
           f'<rect width="{WIDTH}" height="{height}" rx="10" fill="var(--surface)"/>',
           '<defs><marker id="arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" '
           'orient="auto"><path d="M0 0L8 4L0 8z" fill="var(--ink-2)"/></marker></defs>']

    def arrow(x1, x2, y, label):
        out.append(f'<path d="M{x1} {y}H{x2}" stroke="var(--ink-2)" stroke-width="1.25" marker-end="url(#arrow)"/>')
        out.append(f'<text class="muted mono" x="{(x1 + x2) / 2}" y="{y - 8}" font-size="10" '
                   f'text-anchor="middle">{esc(label)}</text>')

    mid = y0 + box_h / 2
    arrow(pad, x0 - 4, mid, '.c')
    for i, (x, name) in enumerate(zip(xs, stages)):
        out.append(f'<rect class="stage" x="{x}" y="{y0}" width="{box_w}" height="{box_h}" rx="6"/>')
        out.append(f'<text x="{x + box_w / 2}" y="{mid + 5}" font-size="13" font-weight="600" '
                   f'text-anchor="middle">{esc(name)}</text>')
        if i < len(artifacts):
            arrow(x + box_w + 2, xs[i + 1] - 4, mid, artifacts[i])
    arrow(xs[-1] + box_w + 2, WIDTH - pad, mid, 'exe')

    for (name, first, last), row in zip(commands, rows):
        y = y0 + box_h + 26 + row * 30
        x1, x2 = xs[first] + 4, xs[last] + box_w - 4
        out.append(f'<rect x="{x1}" y="{y}" width="{x2 - x1}" height="20" rx="4" fill="var(--keel)"/>')
        out.append(f'<text class="mono" x="{(x1 + x2) / 2}" y="{y + 14}" font-size="12" '
                   f'text-anchor="middle" style="fill:var(--on-blue)">{esc(name)}</text>')
    out.append('</svg>\n')
    return '\n'.join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--verify-evidence', action='store_true')
    args = parser.parse_args()
    data = json.loads((HERE / 'readme-benchmarks.json').read_text())
    if args.verify_evidence:
        evidence = {}
        for key, source in data['sources'].items():
            raw = (ROOT / source['path']).read_bytes()
            assert hashlib.sha256(raw).hexdigest() == source['sha256'], source['path']
            evidence[key] = json.loads(raw)
        for chart in data['charts']:
            for row in chart['rows']:
                value = evidence[row['source']]
                for part in row['pointer']:
                    value = value[part]
                assert value == row.get('median_ms', row.get('value')), row['label']
    outputs = {f'readme-{chart["id"]}.svg': render(data, chart) for chart in data['charts']}
    outputs['readme-pipeline.svg'] = render_pipeline()
    for name, output in outputs.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == output, f'Outdated chart: {path}'
        else:
            path.write_text(output)
    print('README charts are up to date' + (' and match the evidence' if args.verify_evidence else ''))


if __name__ == '__main__':
    main()
