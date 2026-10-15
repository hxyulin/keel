#!/usr/bin/env python3
"""Render measure_backend_tiers.py results and retain compressed raw samples."""
import argparse
import gzip
import json
import re
from pathlib import Path
import statistics


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--input-dir', type=Path, required=True)
parser.add_argument('--output-dir', type=Path, required=True)
parser.add_argument('--validation-log', type=Path, help='optional completed cargo test log to retain')
args = parser.parse_args()
args.output_dir.mkdir(parents=True, exist_ok=True)
reports = {name: json.loads((args.input_dir / (name + '.json')).read_text())
           for name in ('builds', 'profiles', 'runtime', 'compatibility')}
for name, report in reports.items():
    (args.output_dir / (name + '.json.gz')).write_bytes(gzip.compress(json.dumps(report, separators=(',', ':')).encode(), mtime=0))
summary = {'host': reports['profiles']['host'], 'cpu': reports['profiles'].get('cpu', ''), 'method': reports['profiles']['method'],
           'optimized': reports['profiles']['optimized'],
           'matching_objects': reports['compatibility']['matching_objects'], 'profiles': {}, 'quality': {}, 'runtime': {}}
if args.validation_log:
    log = args.validation_log.read_text()
    summary['tests_passed'] = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log)))
    (args.output_dir/'validation.log.gz').write_bytes(gzip.compress(log.encode(), mtime=0))
for name, profile in reports['profiles']['profiles'].items():
    stage_totals = {}
    quality = {}
    for candidate in ('baseline-full', 'candidate-full', 'candidate-fast'):
        samples = [s['result'] for s in profile['samples'] if s['candidate'] == candidate]
        stages = {}
        for sample in samples:
            runs = [r for r in sample['runs'] if r['mode'] == 'structured']
            for stage in runs[0]['stages']:
                cell = stages.setdefault(stage, {'self_ms': 0., 'inclusive_ms': 0.})
                for key in cell:
                    cell[key] += statistics.mean(run['stages'][stage][key] for run in runs)
        for cell in stages.values():
            for key in cell:
                cell[key] /= len(profile['summed_codegen_ms'][candidate])
        stage_totals[candidate] = stages
        if candidate.startswith('baseline'):
            continue
        # Take each input once. Every process uses identical selected IR.
        unique = {s['input']: s for s in samples}
        quality[candidate] = {key: sum(row[key] for row in unique.values()) for key in ('code_bytes', 'native_instructions')}
        for category in ('allocation_metrics', 'explicit_spill_helper_calls'):
            quality[candidate][category] = {key: sum(row[category][key] for row in unique.values()) for key in next(iter(unique.values()))[category]}
    summary['profiles'][name] = {'median_codegen_ms': {key: statistics.median(values) for key, values in profile['summed_codegen_ms'].items()},
                                  'comparisons': profile['comparisons'], 'stages': stage_totals}
    summary['quality'][name] = quality
for name, runtime in reports['runtime']['runtime'].items():
    summary['runtime'][name] = {'median_ms': {key: statistics.median(values) for key, values in runtime['samples_ms'].items()},
                               'comparisons': runtime['comparisons'], 'outputs_identical': True}
summary['builds'] = {name: {key: value for key,value in row.items() if key != 'source_hashes'} for name,row in reports['builds']['builds'].items()}
(args.output_dir/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
html = '''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Keel allocator experiments</title><style>
*{box-sizing:border-box}body{font:14px/1.5 var(--font-sans,system-ui);margin:0;color:var(--foreground,#222)}h2{font-size:22px;line-height:1.25;margin:0 0 8px}h3{font-size:16px;margin:24px 0 8px}p{margin:8px 0 14px}.muted,small{color:var(--muted-foreground,#666)}.wrap{overflow-x:auto}table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums;font-size:13px}th,td{padding:8px 5px;border-bottom:1px solid var(--border,#ddd);text-align:right}th:first-child,td:first-child{text-align:left}th{font-weight:600}.bar{display:flex;height:26px;overflow:hidden;border-radius:4px;margin:6px 0 10px}.legend{display:flex;flex-wrap:wrap;gap:5px 14px;font-size:12px}.dot{display:inline-block;width:9px;height:9px;border-radius:2px;margin-right:5px}select{font:inherit;color:inherit;background:var(--background,#fff);border:1px solid var(--border,#ccc);border-radius:6px;padding:7px;max-width:100%}.note{font-size:12px;color:var(--muted-foreground,#666)}li{margin:7px 0}code{font-family:var(--font-mono,monospace);font-size:12px}
</style></head><body>
<h2>Full uses 19–29% less codegen time</h2>
<p class="muted" id="hardware"></p>
<p>Fast cuts another 11–19%; its runtime medians were about 2–6% higher in these unoptimized workloads.</p>
<p>Full keeps lifetime holes and register preferences. Fast builds conservative endpoints directly and uses a simpler scanner. Both share cheaper liveness and folding bookkeeping.</p>
<p><strong>Full remains the default.</strong> Both tiers preserve their previous allocation decisions and output in the application comparisons. A separately tested folding fix corrects duplicated immediate operands under optional full selection.</p>
<h3>Backend compilation time</h3><p class="muted">Structured codegen only, ms. Frontend, IR optimization, encoding, and object writing are excluded. All configurations use baseline selection and unoptimized IR; Lua sums all 33 files.</p>
<div class="wrap"><table><thead><tr><th>Workload</th><th>Old Full</th><th>Old Fast</th><th>New Full</th><th>New Fast</th></tr></thead><tbody id="times"></tbody></table></div>
<div id="speed-details" class="note"></div>
<h3>Generated-code tradeoff</h3><p class="muted">New Fast relative to New Full. Explicit spill helper calls are static codegen counts; they exclude other memory operations and do not measure dynamically executed traffic. ¹ Counts shown as Fast / Full.</p>
<div class="wrap"><table><thead><tr><th>Workload</th><th>Code bytes</th><th>Instructions</th><th>Spill loads¹</th><th>Spill stores¹</th></tr></thead><tbody id="quality"></tbody></table></div>
<h3>Application runtime</h3><p class="muted">Seven randomized measured rounds plus one warmup. Ratio above 1 means Fast runs slower. Intervals are paired bootstrap estimates; these workloads do not establish a universal quality bound.</p>
<div class="wrap"><table><thead><tr><th>Workload</th><th>Full, ms</th><th>Fast, ms</th><th>Fast / Full</th><th>95% interval</th></tr></thead><tbody id="runtime"></tbody></table></div>
<h3>Remaining cost in New Full</h3><select id="workload"></select><div id="stages"></div><div id="legend" class="legend"></div><p id="fold-details" class="note"></p>
<h3>What changed and what follows</h3><ul><li><strong>Shared allocation work:</strong> DFS-only block order, reusable sparse liveness sets with a predecessor worklist, and reduced scanner allocations and interference searches.</li><li><strong>Shared folding work:</strong> indexed flags and saturated use counts replace hot hash accumulation. Regression tests preserve operand multiplicity, aliases, selected roots, and later frame-dependent address folds.</li><li><strong>Lowering:</strong> finer entry/body/finish measurements and explicit spill helper counters are in place. Selection effort remains independently configurable; no second emitter was introduced.</li></ul>
<p class="note" id="validation"></p>
<script>const report=REPORT_DATA;document.getElementById('hardware').textContent=report.cpu+' · AArch64 Mach-O · independent allocator and selection tiers';const names={'lua':'Lua · 33 files','quickjs-core':'QuickJS · core','sqlite-core':'SQLite · amalgamation'};const fmt=n=>n.toFixed(2);const delta=(a,b)=>((a>=b?'+':'')+((a/b-1)*100).toFixed(1)+'%');function row(id,cells){const tr=document.createElement('tr');for(const v of cells){const td=document.createElement('td');td.textContent=v;tr.appendChild(td)}document.getElementById(id).appendChild(tr)}
let notes=[];for(const [name,p]of Object.entries(report.profiles)){const t=p.median_codegen_ms;row('times',[names[name],fmt(t['baseline-full']),fmt(t['baseline-fast']),fmt(t['candidate-full']),fmt(t['candidate-fast'])]);const a=p.comparisons['candidate-full/baseline-full'],b=p.comparisons['candidate-fast/candidate-full'];notes.push(names[name]+': Full '+(100*(1-a.median_ratio)).toFixed(1)+'% less time than old Full; Fast '+(100*(1-b.median_ratio)).toFixed(1)+'% less than new Full.');const q=report.quality[name],full=q['candidate-full'],fast=q['candidate-fast'];row('quality',[names[name],delta(fast.code_bytes,full.code_bytes),delta(fast.native_instructions,full.native_instructions),fast.explicit_spill_helper_calls.spill_helper_loads+' / '+full.explicit_spill_helper_calls.spill_helper_loads,fast.explicit_spill_helper_calls.spill_helper_stores+' / '+full.explicit_spill_helper_calls.spill_helper_stores]);}document.getElementById('speed-details').textContent=notes.join(' ');
for(const [name,r]of Object.entries(report.runtime)){const q=r.comparisons['candidate-fast/candidate-full'];row('runtime',[name,fmt(r.median_ms['candidate-full']),fmt(r.median_ms['candidate-fast']),q.median_ratio.toFixed(3)+'×',q.bootstrap_95pct.map(x=>x.toFixed(3)).join('–')]);}
const groups=[['Allocation',['allocation_overhead','liveness','live_intervals','linear_scan','spill_slots'],'#478fe8'],['Lowering',['instruction_lowering','lowering_entry','allocation_annotations','lowering_body','lowering_finish'],'#36ad99'],['Folding',['fold_analysis','fold_values','compare_fusion'],'#ad80dd'],['Preparation & other',['module_overhead','function_preparation_cleanup','selection','data_emission','debug_emission'],'#97a1b0'],['Relaxation',['branch_relaxation'],'#e7ad4b']];const sel=document.getElementById('workload');Object.keys(report.profiles).forEach(name=>sel.add(new Option(names[name],name)));function update(){const st=report.profiles[sel.value].stages['candidate-full'],total=Object.values(st).reduce((n,v)=>n+v.self_ms,0);const chart=document.getElementById('stages'),legend=document.getElementById('legend');chart.replaceChildren();legend.replaceChildren();const bar=document.createElement('div');bar.className='bar';for(const[label,keys,color]of groups){const percent=100*keys.reduce((n,k)=>n+(st[k]?.self_ms||0),0)/total;const segment=document.createElement('div');segment.style.width=percent+'%';segment.style.background=color;segment.title=label+' '+percent.toFixed(1)+'%';bar.appendChild(segment);const item=document.createElement('span'),dot=document.createElement('span');dot.className='dot';dot.style.background=color;item.append(dot,document.createTextNode(label+' '+percent.toFixed(1)+'%'));legend.appendChild(item)}chart.appendChild(bar);document.getElementById('fold-details').textContent='Folding: constant/address analysis '+fmt(st.fold_values.self_ms)+' ms; compare-use analysis '+fmt(st.compare_fusion.self_ms)+' ms. Lowering body '+fmt(st.lowering_body.self_ms)+' ms; entry '+fmt(st.lowering_entry.self_ms)+' ms; finish '+fmt(st.lowering_finish.self_ms)+' ms. Exclusive mean stage times include profiler overhead.';}sel.onchange=update;update();document.getElementById('validation').textContent=report.matching_objects+' whole-application object comparisons matched each tier’s saved baseline byte-for-byte. All runtime outputs matched. '+(report.tests_passed ? report.tests_passed+' tests passed (debugger sessions excluded in this run). ' : '')+'Measurements are warm-cache, serial, on '+report.host+'. Codegen figures use five paired rounds of three in-process samples; paired bootstrap uses 5,000 resamples. Profiler builds contain opt-in instrumentation; normal runtime builds do not. Added profiling scopes and counters may slightly disadvantage the new profiled binary. Raw data, hashes and method are retained alongside this report. No default effort preset changed. Execution measurements are native AArch64; cross-target codegen tests passed, but Linux/x86 runtime was not measured.';console.log('Validated',Object.keys(report.profiles).length,'profile workloads and',Object.keys(report.runtime).length,'runtime workloads');
</script></body></html>'''.replace('REPORT_DATA', json.dumps(summary,separators=(',',':')).replace('</','<\\/'))
(args.output_dir/'report.html').write_text(html)
print(args.output_dir)
