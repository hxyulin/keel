"""Render cumulative preprocessor improvements and refreshed profile results."""
import html
import json
import pathlib
import statistics
import math

ROOT = pathlib.Path('suite/reports')

def read(name):
    return json.loads((ROOT / name).read_text())

def main():
    timing = read('c-preprocessor-deeper-20261015.json')
    storage = read('c-preprocessor-deeper-storage-20261015.json')
    before = read('c-preprocessor-current-profile-20261015.json')
    after = read('c-preprocessor-deeper-profile-20261015.json')
    assert timing['sources']['crates/keel-c/src/preprocess.rs'] == after['source_sha256']
    aggregate = timing['aggregate']['all']
    low, high = aggregate['ratio_ci95']
    apps = []
    for app, s in timing['summaries'].items():
        label = {'lua': 'Lua', 'quickjs': 'QuickJS', 'sqlite': 'SQLite'}[app]
        base, final = s['median_ms']['baseline'], s['median_ms']['all']
        saving = 100 * (1 - s['comparisons']['all']['median_ratio'])
        apps.append(f'<div class="app"><div><strong>{label}</strong><span class="saving">{saving:.1f}% less time</span></div><div class="bars"><div class="old" style="width:100%"></div><div class="new" style="width:{100*final/base:.1f}%"></div></div><small>{base:.1f} → {final:.1f} ms per whole application</small></div>')
    steps = []
    order = [('baseline', 'scan', 'Scanning', 'Track the next physical newline; skip whitespace spans before location work.'), ('scan', 'directives', 'Directives', 'Borrow line slices, skip inactive ordinary code, and remap locations only when required.'), ('directives', 'headers', 'Header search', 'Cache positive and negative resolutions with quoted-caller and include-next context.'), ('headers', 'all', 'Macro expansion', 'Consume reversed input without suffix shifts; recycle drained buffers up to 4,096 tokens.')]
    for left, right, title, description in order:
        ratios = []
        for app in timing['summaries']:
            rows = {(x['round'], x['variant']): x['total_ms'] for x in timing['samples'] if x['app'] == app}
            ratios.append(statistics.median(rows[i, right] / rows[i, left] for i in range(timing['rounds'])))
        gain = 100 * (1 - math.prod(ratios) ** (1/3))
        steps.append(f'<div class="step"><div><strong>{title}</strong><p>{html.escape(description)}</p></div><strong class="gain">−{gain:.1f}%</strong></div>')
    profiles = []
    memory = []
    for app in timing['summaries']:
        label = {'lua': 'Lua', 'quickjs': 'QuickJS', 'sqlite': 'SQLite'}[app]
        a, b = before['applications'][app]['stack'], after['applications'][app]['stack']
        profiles.append(f'<tr><th>{label}</th><td>{a["category_percent"]["memmove"]:.1f}% → {b["category_percent"]["memmove"]:.1f}%</td><td>{a["memmove_context_percent"]["Macro expansion"]:.1f}% → {b["memmove_context_percent"].get("Macro expansion",0):.1f}%</td></tr>')
        x = storage['applications'][app]
        calls = -100*x['allocation_call_reduction']
        memory.append(f'<tr><th>{label}</th><td>{calls:+.1f}%</td><td>{x["baseline"]["peak_rss_bytes"]/2**20:.1f} → {x["candidate"]["peak_rss_bytes"]/2**20:.1f} MiB</td></tr>')
    document = '''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
*{box-sizing:border-box}body{margin:0;color:var(--foreground,#27272c);background:transparent;font:14px/1.5 var(--font-sans,system-ui)}h2{font-size:22px;letter-spacing:-.5px;margin:0 0 5px}h3{font-size:16px;margin:22px 0 10px}p{margin:5px 0 12px}.muted,small{color:var(--muted-foreground,#777)}.hero{font-size:33px;font-weight:650;margin-top:12px;letter-spacing:-1px}.apps{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-top:18px}.app .saving{display:block;color:var(--accent,#6682ee);font-size:13px}.bars{margin:10px 0 5px}.bars>div{height:9px;border-radius:3px;margin:5px 0}.old{background:var(--muted,#ccc)}.new{background:var(--chart-1,#6473da)}.step{display:flex;gap:15px;justify-content:space-between;border-top:1px solid var(--border,#ddd);padding:10px 0}.step p{color:var(--muted-foreground,#777);margin:2px 0 0}.gain{white-space:nowrap;color:var(--accent,#6473da);font-size:18px}.columns{display:grid;grid-template-columns:1fr 1fr;gap:24px}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;border-bottom:1px solid var(--border,#ddd);padding:8px 3px;font-variant-numeric:tabular-nums}thead th{font-weight:500;color:var(--muted-foreground,#777)}.verified{padding:12px 0;border-top:1px solid var(--border,#ddd);margin-top:20px}@media(max-width:550px){.apps,.columns{grid-template-columns:1fr}.app .saving{display:inline;margin-left:12px}.columns{gap:0}.hero{font-size:28px}h2{font-size:20px}}
</style></head><body><h2>Four preprocessor hotspots improved</h2><p class="muted">Compared with the just-profiled version · same 44-file corpus · Apple M5 Max</p><div class="hero">SAVING% less preprocessing time</div><p>SPEEDUP× speedup · 95% paired bootstrap interval: LOW–HIGH% less time</p><div class="apps">APPS</div><h3>Contribution of each stage</h3>STEPS<small>Cumulative variants: each reduction compares with the preceding stage, averaged geometrically across applications. Contributions depend on order and are not additive.</small><div class="columns"><section><h3>Copying in fresh profiles</h3><table><thead><tr><th></th><th>All copying</th><th>Inside expansion</th></tr></thead><tbody>PROFILES</tbody></table><p class="muted">Percent of all stack samples, before → after. These shares overlap operation categories and are not elapsed-time ratios.</p></section><section><h3>Memory tradeoff</h3><table><thead><tr><th></th><th>Allocation calls</th><th>Peak RSS</th></tr></thead><tbody>MEMORY</tbody></table><p class="muted">Scratch reuse keeps call counts close to baseline. RSS is one full counting-process observation per variant, including warmups.</p></section></div><div class="verified"><strong>67 tests pass.</strong> All 44 outputs match byte-for-byte across six variants, generated IR verifies, and the rebuilt release driver matches all 44 CLI outputs.</div><small>11 paired rounds per application, serial warm-cache calls; raw input reads and returned-output destruction excluded. Allocator counters and six-second profiles run separately from timing. Caches reset per translation unit; included files and include directories must remain stable during a call. No cross-machine or cold-storage claim.</small></body></html>'''
    for token, value in {'SPEEDUP': f'{aggregate["speedup"]:.2f}', 'SAVING': f'{100*(1-aggregate["equal_application_geomean_ratio"]):.1f}', 'LOW': f'{100*(1-high):.1f}', 'HIGH': f'{100*(1-low):.1f}', 'APPS': ''.join(apps), 'STEPS': ''.join(steps), 'PROFILES': ''.join(profiles), 'MEMORY': ''.join(memory)}.items():
        document = document.replace(token, value)
    (ROOT / 'c-preprocessor-deeper-20261015.html').write_text(document)

if __name__ == '__main__':
    main()
