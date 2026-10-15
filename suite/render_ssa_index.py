"""Render paired SSA builder optimizations as standalone HTML."""
import argparse
import json
import pathlib


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=pathlib.Path, required=True)
    p.add_argument('--storage', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--kind', choices=['index', 'reads'], default='index')
    args = p.parse_args()
    profile = json.loads(args.profile.read_text())
    storage = json.loads(args.storage.read_text())
    variant = 'compact' if args.kind == 'reads' else 'indexed'
    # Normalize the display schema; the saved measurements retain variant names.
    data = dict(aggregate={k.replace(variant + ':', 'indexed:'): v for k, v in profile['aggregate'].items()},
                summaries={}, memory={})
    for key, row in profile['summaries'].items():
        data['summaries'][key] = dict(variants={'baseline': row['variants']['baseline'],
            'indexed': row['variants'][variant]}, comparisons={'indexed': row['comparisons'][variant]})
    for key, row in storage['applications'].items():
        data['memory'][key] = {
            name: dict(calls=row[source]['totals']['alloc'] + row[source]['totals']['realloc'],
                       bytes=row[source]['totals']['requested_bytes'], rss=row[source]['peak_rss_bytes'])
            for name, source in [('baseline', 'baseline'), ('indexed', variant)]}
    html = '''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>
*{box-sizing:border-box}body{margin:0;font:14px/1.5 var(--font-sans,system-ui);color:var(--foreground,#ddd)}
h2{font-size:22px;margin:0 0 8px}h3{font-size:16px;margin:26px 0 10px}p{margin:10px 0}.muted,small{color:var(--muted-foreground,#999)}
select{font:inherit;color:inherit;background:var(--background,#111);border:1px solid var(--border,#444);border-radius:6px;padding:7px 12px;margin:6px 0 16px}
.cards{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}.card{padding:16px;border:1px solid var(--border,#333);border-radius:var(--radius,8px)}.big{font-size:25px;font-weight:700;line-height:1.2}
table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:right;border-bottom:1px solid var(--border,#333);padding:10px 4px}th{color:var(--muted-foreground,#999);font-weight:400}th:first-child,td:first-child{text-align:left;padding-left:0}
.chart{margin:16px 0}.label{display:flex;justify-content:space-between;margin:0 0 5px}.track{height:13px;background:var(--muted,#222);margin:4px 0;border-radius:2px;overflow:hidden}.bar{height:100%}.before{background:var(--chart-1,#488dca)}.after{background:var(--chart-2,#48a98c)}
.legend{display:flex;gap:16px;font-size:12px}.dot{display:inline-block;width:10px;height:10px;margin-right:5px}.foot{font-size:11px;border-top:1px solid var(--border,#333);margin-top:24px;padding-top:12px}
@media(max-width:460px){.cards{gap:6px}.card{padding:16px}.big{font-size:21px}th,td{font-size:11px;padding:8px 2px}}
</style></head><body>
<h2>SSA: index incoming edges as control flow grows</h2>
<p class="muted">44 translation units · Lua / QuickJS / SQLite · Apple M5 Max</p>
<select id="mode" aria-label="Output mode"><option value="False">Normal mode</option><option value="True">Debug info</option></select>
<div class="cards"><div class="card"><div class="big" id="headline"></div><small>less frontend time</small></div><div class="card"><div class="big" id="ir"></div><small>less IR generation time</small></div><div class="card"><div class="big">81</div><small>tests passing</small></div></div>
<p class="muted" id="interval"></p>
<h3>IR generation, milliseconds per application</h3>
<div class="legend"><span><i class="dot before"></i>Previous commit</span><span><i class="dot after"></i>Incoming-edge index</span></div>
<div id="charts"></div>
<h3>Whole frontend compilation</h3>
<table><thead><tr><th>Application</th><th>Before → after, ms</th><th>Less time</th><th>95% paired interval</th></tr></thead><tbody id="times"></tbody></table>
<h3>What changed</h3>
<p>Each source block's outgoing edges are observed after its instruction builder finishes. Sealing a destination takes its incoming-edge bucket instead of scanning every block. Edges are sorted in source layout order to preserve deterministic IR.</p>
<p>Raw CFG edits through <code>function_mut()</code> invalidate the index; it rebuilds before the next seal. Shared access uses <code>function()</code>. Stack slots, imports, and block parameters have helpers that preserve the index. This replaces the builder's public <code>func</code> field.</p>
<h3>Allocation and memory checks</h3>
<table><thead><tr><th>Application</th><th>Allocation calls</th><th>Requested bytes</th><th>Peak RSS, MiB</th></tr></thead><tbody id="memory"></tbody></table>
<p class="muted">Changes are relative to the previous commit. Peak RSS is one full counting-process observation per variant, including resident inputs and warmup; it is not an isolated-call peak.</p>
<p><strong>IR is byte-identical:</strong> all 44 normal and 44 debug-info outputs match the baseline (176 snapshots), including verification and public-versus-staged equality. Regression coverage includes loops, duplicate destinations, raw destination rewrites, removal/movement, and source visitation order. Executable C fixtures pass.</p>
<p class="foot muted">Baseline: 7528fb6, which already includes the lexer and arena/interner work. Nine paired serial rounds per application and output mode; application order shuffled and variant order rotated. Headline is the equal-application geometric mean of paired public-compile ratios, with a paired bootstrap interval. IR-generation reduction compares staged medians. Inputs are preprocessed and resident. Preprocessing, input reads, optimizer, backend, IR verification, process startup, and returned-module destruction are excluded from timings. Allocation instrumentation runs separately. Results apply to this host and corpus.</p>
<script>const D=__DATA__;
const apps=['lua','quickjs','sqlite'],labels={lua:'Lua',quickjs:'QuickJS',sqlite:'SQLite'};
const f=x=>x.toFixed(1),pct=x=>f(100*x)+'%',delta=x=>(x>=0?'+':'')+pct(x);
function render(){const mode=document.getElementById('mode').value,a=D.aggregate['indexed:'+mode];
document.getElementById('headline').textContent=pct(1-a.equal_application_geomean_ratio);
document.getElementById('interval').textContent=f(a.speedup)+'× speedup · 95% interval: '+pct(1-a.ratio_ci95[1])+'–'+pct(1-a.ratio_ci95[0])+' less frontend time';
const rows=apps.map(app=>({app,s:D.summaries[app+':'+mode],m:D.memory[app+':'+mode]}));
const irRatio=Math.exp(rows.reduce((sum,r)=>sum+Math.log(r.s.variants.indexed.stages_median_ms.irgen_ms/r.s.variants.baseline.stages_median_ms.irgen_ms),0)/rows.length);
document.getElementById('ir').textContent=pct(1-irRatio);
const max=Math.max(...rows.map(r=>r.s.variants.baseline.stages_median_ms.irgen_ms));
document.getElementById('charts').innerHTML=rows.map(r=>{const b=r.s.variants.baseline.stages_median_ms.irgen_ms,c=r.s.variants.indexed.stages_median_ms.irgen_ms;return `<div class="chart"><div class="label"><strong>${labels[r.app]}</strong><span>${f(b)} → ${f(c)} ms</span></div><div class="track"><div class="bar before" style="width:${100*b/max}%"></div></div><div class="track"><div class="bar after" style="width:${100*c/max}%"></div></div></div>`}).join('');
document.getElementById('times').innerHTML=rows.map(r=>{const b=r.s.variants.baseline.compile_median_ms,c=r.s.variants.indexed.compile_median_ms,p=r.s.comparisons.indexed;return `<tr><td><strong>${labels[r.app]}</strong></td><td>${f(b)} → ${f(c)}</td><td>${pct(1-p.median_ratio)}</td><td>${pct(1-p.ratio_ci95[1])}–${pct(1-p.ratio_ci95[0])}</td></tr>`}).join('');
document.getElementById('memory').innerHTML=rows.map(r=>{const b=r.m.baseline,c=r.m.indexed;return `<tr><td><strong>${labels[r.app]}</strong></td><td>${delta(c.calls/b.calls-1)}</td><td>${delta(c.bytes/b.bytes-1)}</td><td>${f(b.rss/2**20)} → ${f(c.rss/2**20)}</td></tr>`}).join('');
}document.getElementById('mode').addEventListener('change',render);render();console.log('Two variants; six application/mode groups; controls ready.');</script></body></html>'''
    if args.kind == 'reads':
        html = html.replace('SSA: index incoming edges as control flow grows', 'SSA: compact variable definitions')
        html = html.replace('Incoming-edge index', 'Compact definition tables').replace('>81</div>', '>83</div>')
        start = html.index("<p>Each source block's outgoing edges")
        end = html.index('<h3>Allocation and memory checks</h3>', start)
        html = html[:start] + '''<p>Block-indexed state replaces hash maps for sealed flags, pending parameters, and predecessor lists. Each block keeps four variable definitions inline. Sorted lists grow as variables are used; above 32 entries, a sparse hash map bounds insertion cost.</p>
<p>SSA parameter construction reads fixed predecessor edges one at a time, removing a vector clone per parameter. Alias resolution, loop sealing, and parameter elimination retain their behavior. Storage stays proportional to actual definitions rather than allocating a block-by-variable matrix.</p>
<p class="muted">Debug-info generation keeps locals in stack slots rather than SSA variables, so this path benefits less from the definition-table changes.</p>
''' + html[end:]
        html = html.replace('Regression coverage includes loops, duplicate destinations, raw destination rewrites, removal/movement, and source visitation order.',
                            'Regression coverage includes sparse variable IDs, repeated updates, inline growth, wide-block promotion, raw block gaps, loops, and trivial-parameter elimination.')
        html = html.replace('Baseline: 7528fb6, which already includes the lexer and arena/interner work.',
                            'Baseline: f39567e, which already includes incoming-edge indexing, the lexer rewrite, and arena/interner work.')
    html = html.replace('Nine paired serial rounds', str(profile['rounds']) + ' paired serial rounds')
    if profile.get('extension_note'):
        html = html.replace('Inputs are preprocessed and resident.', 'The run was extended from 9 to 18 pairs after large latency spikes; every original sample is retained. Inputs are preprocessed and resident.')
    args.output.write_text(html.replace('__DATA__', json.dumps(data, separators=(',', ':'))))


if __name__ == '__main__':
    main()
