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

WIDTH, PAD, LABEL_W, VALUE_W = 720, 24, 150, 84
ROW, BAR = 34, 20
UNITS = {'ms': (1.0, 'ms'), 'us': (1000.0, 'µs')}

STYLE = """<style>
svg{--surface:#fcfcfb;--ink:#0b0b0b;--ink-2:#52514e;--grid:#e4e3df;--keel:#2a78d6;--other:#a3a29b}
@media (prefers-color-scheme:dark){svg{--surface:#1a1a19;--ink:#f0efec;--ink-2:#c3c2b7;--grid:#3a3935;--keel:#3987e5;--other:#6f6e68}}
text{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;fill:var(--ink)}
.muted{fill:var(--ink-2)}
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


def bar(x, y, w, h, r=4):
    r = min(r, w)
    return (f'<path d="M{x} {y}h{w - r:.2f}a{r} {r} 0 0 1 {r} {r}v{h - 2 * r}'
            f'a{r} {r} 0 0 1 -{r} {r}h-{w - r:.2f}z"')


def render(data, chart):
    esc = html.escape
    ratio = chart.get('kind') == 'ratio'
    if ratio:
        rows = [(r['label'], r['value'], True) for r in chart['rows']]
        overall = math.exp(sum(math.log(v) for _, v, _ in rows) / len(rows))
        unit, headline_text = '×', f'{overall:.1f}× slower'
        caption = f'geometric mean vs {chart["reference"]}'
        axis = f'Run time relative to {chart["reference"]} · lower is better'
        label_value = lambda v: f'{v:.2f}×'
        desc = '; '.join(f'{label}: {v:.2f}×' for label, v, _ in rows)
        desc += f'. Overall {overall:.2f}× the run time of {chart["reference"]}.'
    else:
        scale, unit = UNITS[chart['unit']]
        rows = [(r['label'], r['median_ms'] * scale, r['keel']) for r in chart['rows']]
        by_label = {label: value for label, value, _ in rows}
        headline = chart['headline']
        speedup = by_label[headline['slow']] / by_label[headline['fast']]
        headline_text = f'{speedup:.1f}× faster'
        caption = f'{headline["fast"]} vs {headline["slow"]}'
        axis = f'Wall time ({unit}) · lower is better'
        label_value = lambda v: f'{fmt(v)} {unit}'
        desc = '; '.join(f'{label}: {fmt(v)} {unit}' for label, v, _ in rows)
        desc += f'. {headline["fast"]} is {speedup:.1f}× faster than {headline["slow"]}.'
    statistic = chart.get('statistic', data['statistic'])
    desc += f' {data["machine"]}, {statistic}.'

    step = 0.5 if ratio else nice_step(max(v for _, v, _ in rows))
    limit = step * math.ceil(max(v for _, v, _ in rows) / step)
    x0 = PAD + LABEL_W
    plot_w = WIDTH - x0 - PAD - VALUE_W
    top = 84
    base = top + ROW * len(rows)
    height = base + 62

    cid = chart['id']
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
           f'viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="{cid}-t {cid}-d">',
           STYLE,
           f'<title id="{cid}-t">{esc(chart["title"])} (lower is better)</title>',
           f'<desc id="{cid}-d">{esc(desc)}</desc>',
           f'<rect width="{WIDTH}" height="{height}" rx="10" fill="var(--surface)"/>',
           f'<text x="{PAD}" y="38" font-size="18" font-weight="650">{esc(chart["title"])}</text>',
           f'<text class="muted" x="{PAD}" y="60" font-size="12">'
           f'{esc(data["machine"])} · {esc(statistic)}</text>',
           f'<text x="{WIDTH - PAD}" y="40" font-size="26" font-weight="700" text-anchor="end">'
           f'{headline_text}</text>',
           f'<text class="muted" x="{WIDTH - PAD}" y="60" font-size="12" text-anchor="end">'
           f'{esc(caption)}</text>']

    value = 0.0
    while value <= limit + 1e-9:
        x = x0 + plot_w * value / limit
        out.append(f'<path d="M{x:.1f} {top - 6}V{base}" stroke="var(--grid)"/>')
        out.append(f'<text class="muted" x="{x:.1f}" y="{base + 16}" font-size="11" '
                   f'text-anchor="middle">{tick(value)}</text>')
        value += step
    out.append(f'<text class="muted" x="{x0 + plot_w / 2:.1f}" y="{base + 40}" font-size="12" '
               f'text-anchor="middle">{esc(axis)}</text>')

    for i, (label, v, keel) in enumerate(rows):
        y = top + i * ROW + (ROW - BAR) / 2 - 4
        w = max(plot_w * v / limit, 1)
        weight = ' font-weight="600"' if keel and not ratio else ''
        fill = 'var(--keel)' if keel else 'var(--other)'
        out.append(f'<text x="{x0 - 12}" y="{y + 14}" font-size="13" text-anchor="end"{weight}>'
                   f'{esc(label)}</text>')
        out.append(bar(x0, y, round(w, 2), BAR) + f' fill="{fill}"/>')
        out.append(f'<text x="{x0 + w + 8:.2f}" y="{y + 14}" font-size="12"{weight}>'
                   f'{label_value(v)}</text>')
    if ratio:
        x = x0 + plot_w / limit
        out.append(f'<path d="M{x:.1f} {top - 10}V{base}" stroke="var(--ink)" stroke-width="1.5"/>')
        out.append(f'<text x="{x:.1f}" y="{top - 14}" font-size="11" text-anchor="middle">'
                   f'{esc(chart["reference"])}</text>')
    out.append('</svg>\n')
    return '\n'.join(out)


def render_pipeline():
    """Toolchain stages and the commands that run them."""
    esc = html.escape
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
    arrow(PAD, x0 - 4, mid, '.c')
    for i, (x, name) in enumerate(zip(xs, stages)):
        out.append(f'<rect class="stage" x="{x}" y="{y0}" width="{box_w}" height="{box_h}" rx="6"/>')
        out.append(f'<text x="{x + box_w / 2}" y="{mid + 5}" font-size="13" font-weight="600" '
                   f'text-anchor="middle">{esc(name)}</text>')
        if i < len(artifacts):
            arrow(x + box_w + 2, xs[i + 1] - 4, mid, artifacts[i])
    arrow(xs[-1] + box_w + 2, WIDTH - PAD, mid, 'exe')

    for (name, first, last), row in zip(commands, rows):
        y = y0 + box_h + 26 + row * 30
        x1, x2 = xs[first] + 4, xs[last] + box_w - 4
        out.append(f'<rect x="{x1}" y="{y}" width="{x2 - x1}" height="20" rx="4" fill="var(--keel)"/>')
        out.append(f'<text class="mono" x="{(x1 + x2) / 2}" y="{y + 14}" font-size="12" '
                   f'text-anchor="middle" style="fill:#fff">{esc(name)}</text>')
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
