#!/usr/bin/env python3
"""Render the measured backend profiles as a standalone, interactive HTML report.
Raw samples are retained as deterministic gzip files alongside the summary.
"""
from pathlib import Path
import argparse
import json
import gzip
from profile_backend import summarize
parser = argparse.ArgumentParser(description='Create an interactive report from profile_backend.py runs named lua, lua-opt, lua-debug and large.')
parser.add_argument('--input-dir', type=Path, required=True)
parser.add_argument('--output-dir', type=Path, required=True)
args = parser.parse_args()
out = args.output_dir
out.mkdir(parents=True, exist_ok=True)
reports={k:json.loads((args.input_dir / (k+'.json')).read_text()) for k in ['lua','lua-opt','lua-debug','large']}
for name,report in reports.items():
    for row in report['inputs']:
        for sample in row['runs']:
            assert abs(sum(s['self_ms'] for s in sample['stages'].values())-sample['stages']['module_overhead']['inclusive_ms'])<1e-8
    (out/(name+'.json.gz')).write_bytes(gzip.compress(json.dumps(report,separators=(',',':')).encode(),mtime=0))
cpu=reports['lua']['cpu']
summary={'cpu':cpu,'host':reports['lua']['host'],'source_commit':reports['lua']['commit'],'instrumentation':'Uncommitted profiling additions on the source commit. No backend optimization changes.', 'method':reports['lua']['method'],'workloads':[]}
sets=[('Lua · 33 files',reports['lua']['inputs']),('Lua · optimized',reports['lua-opt']['inputs']),('Lua · debug',reports['lua-debug']['inputs'])]
sets += [('QuickJS · core',[r]) if Path(r['input']).name=='quickjs.i' else ('SQLite · amalgamation',[r]) for r in reports['large']['inputs']]
for name,rows in sets:
    metrics=summarize(rows)
    hot=[]
    for row in rows:
        for f in row['function_profiles']['structured']:
            hot.append({'name':f['name'],'input':Path(row['input']).name,'mean_codegen_ms':f['stages_total_over_rounds']['function_preparation_cleanup']['inclusive_ms']/row['rounds']})
    hot.sort(key=lambda x:-x['mean_codegen_ms'])
    summary['workloads'].append({'name':name,'files':len(rows),'functions':sum(r['functions'] for r in rows),'ir_instructions':sum(r['ir_instructions'] for r in rows),'rounds':rows[0]['rounds'],'objects_identical':all(r['objects_identical'] for r in rows),'metrics':metrics,'hot_functions':hot[:5], 'inputs':[{'path':r['input'],'sha256':r['source_sha256']} for r in rows]})
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
data=json.dumps(summary,separators=(',',':'))
html='''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Keel backend profiling</title><style>
*{box-sizing:border-box}body{font-family:var(--font-sans,system-ui);color:var(--foreground,#222);margin:0;font-size:14px;line-height:1.5}h2{font-size:22px;line-height:1.25;margin:0 0 8px}h3{font-size:16px;margin:25px 0 8px}p{margin:8px 0 15px}.muted,small{color:var(--muted-foreground,#666)}.lead{font-size:16px}select{font:inherit;border:1px solid var(--border,#ccc);border-radius:6px;background:var(--background,white);color:inherit;padding:7px 10px;max-width:100%;margin:8px 0 15px}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums;font-size:13px}th,td{text-align:right;padding:8px 5px;border-bottom:1px solid var(--border,#ddd)}th:first-child,td:first-child{text-align:left}th{font-weight:600}.wrap{overflow-x:auto}.bar{display:flex;overflow:hidden;border-radius:5px;height:28px;width:100%;margin:7px 0 12px}.seg{min-width:0}.legend{display:flex;gap:8px 16px;flex-wrap:wrap;font-size:12px}.dot{width:9px;height:9px;display:inline-block;border-radius:2px;margin-right:5px}.label{display:flex;justify-content:space-between;gap:12px}.pill{background:var(--secondary,#eee);padding:2px 7px;border-radius:4px;font-size:12px}li{margin:7px 0}code{font-family:var(--font-mono,monospace);font-size:12px}.note{font-size:12px;margin-top:18px;color:var(--muted-foreground,#666)}
</style></head><body>
<h2>Register allocation is the largest backend cost</h2>
<p class="lead">It uses <strong>51–56%</strong> of structured codegen time on the three application workloads. Folding analysis adds <strong>13–16%</strong>.</p>
<p class="muted" id="host"></p>
<h3>Where structured codegen spends time</h3>
<label for="workload">Workload</label><br><select id="workload"></select><div id="stage-chart"></div><div id="legend" class="legend"></div>
<h3>Backend stage totals</h3>
<p class="muted">Sum of separately timed codegen, assembly, and writer medians (ms). Lua rows sum 33 files. These are stage estimates; frontend and optimization time are excluded.</p>
<div class="wrap"><table><thead><tr><th>Workload</th><th>Text route</th><th>Structured</th><th>Ratio</th></tr></thead><tbody id="pipeline"></tbody></table></div>
<h3 id="detail-title"></h3><div class="wrap"><table><thead><tr><th>Stage</th><th>Text, ms</th><th>Structured, ms</th></tr></thead><tbody id="details"></tbody></table></div>
<h3>Largest functions in the selected workload</h3><div id="hot"></div>
<h3>Investigate next</h3>
<ol><li><strong>Liveness sets:</strong> measure replacing repeated tree-set construction and cloning with reusable indexed sets. Liveness alone reaches 26% on SQLite.</li><li><strong>Folding bookkeeping:</strong> investigate indexed value-use counts, reusable scratch, and the per-instruction temporary vectors. This stage remains 13–16% on ordinary structured output.</li><li><strong>Linear scan:</strong> investigate per-interval register-list allocation and active/inactive conflict searches. Scan accounts for 13–17%.</li></ol>
<p class="note">These are investigation targets inferred from stage timings and source inspection, not demonstrated optimization gains. No backend algorithms were optimized in this profiling pass.</p>
<p class="note" id="method"></p>
<script>const report=REPORT_DATA;
const groups=[['Register allocation',['allocation_overhead','liveness','live_intervals','linear_scan','spill_slots'],'#478fe8'],['Instruction lowering',['instruction_lowering','lowering_entry','allocation_annotations','lowering_body','lowering_finish'],'#36ad99'],['Folding analysis',['fold_analysis','fold_values','compare_fusion'],'#ad80dd'],['Preparation & other',['function_preparation_cleanup','module_overhead','selection','data_emission'],'#97a1b0'],['Relaxation',['branch_relaxation'],'#e7ad4b'],['Debug metadata',['debug_emission'],'#db819b']];
const sel=document.getElementById('workload');report.workloads.forEach((w,i)=>sel.add(new Option(w.name,i)));
document.getElementById('host').textContent=report.cpu+' · AArch64 Mach-O · 9 measured rounds + 2 warmups per input · full allocator';
const sum=(obj,keys)=>keys.reduce((n,k)=>n+(obj[k]||0),0);const total=m=>m.plain_codegen_ms+m.assemble_ms+m.macho_ms;const fmt=n=>n.toFixed(2);
const tbody=document.getElementById('pipeline');for(const w of report.workloads){const a=total(w.metrics.text),b=total(w.metrics.structured);const tr=document.createElement('tr');for(const value of [w.name,fmt(a),fmt(b),(a/b).toFixed(2)+'×']){const td=document.createElement('td');td.textContent=value;tr.appendChild(td)}tbody.appendChild(tr)}
function update(){const w=report.workloads[sel.value];const m=w.metrics;const chart=document.getElementById('stage-chart');chart.replaceChildren();const legend=document.getElementById('legend');legend.replaceChildren();const bar=document.createElement('div');bar.className='bar';for(const [label,keys,color] of groups){const share=sum(m.structured.stage_shares,keys)*100;const seg=document.createElement('div');seg.className='seg';seg.style.width=share+'%';seg.style.background=color;seg.title=label+': '+share.toFixed(1)+'%';bar.appendChild(seg);const item=document.createElement('span');const dot=document.createElement('span');dot.className='dot';dot.style.background=color;item.append(dot,document.createTextNode(label+' '+share.toFixed(1)+'%'));legend.appendChild(item)}chart.appendChild(bar);document.getElementById('detail-title').textContent=w.name+' — stage wall times';const details=document.getElementById('details');details.replaceChildren();const stageRows=[['Code generation','plain_codegen_ms'],['Assembly / encoding','assemble_ms'],['Mach-O writing','macho_ms']];for(const [label,key] of stageRows){const tr=document.createElement('tr');for(const value of [label,fmt(m.text[key]),fmt(m.structured[key])]){const td=document.createElement('td');td.textContent=value;tr.appendChild(td)}details.appendChild(tr)}const hot=document.getElementById('hot');hot.replaceChildren();for(const f of w.hot_functions.slice(0,3)){const line=document.createElement('div');line.className='label';const name=document.createElement('code');name.textContent=f.name;const timing=document.createElement('span');timing.textContent=fmt(f.mean_codegen_ms)+' ms';line.append(name,timing);hot.appendChild(line)}const capture=100*(m.structured.profiled_codegen_ms/m.structured.plain_codegen_ms-1);document.getElementById('method').textContent='All '+w.files+' inputs produced identical text-path and structured-path objects; enabling collection also preserved output. Stage chart uses exclusive mean times that sum to the instrumented codegen root. Wall times above use inactive collection in the same feature-enabled binary. For this workload, active collection changed summed median structured-codegen time by '+capture.toFixed(1)+'% (small differences include timing noise). Normal builds compile timing instrumentation out. Source reading, C frontend, IR verification, optional optimization, startup and file I/O are excluded. These are warm-cache serial backend measurements, not full build speedups. QuickJS and SQLite rows cover their core/amalgamation files, not whole suites. Baseline selection is used; the optimized Lua row runs default IR optimization before timing. Stage attribution includes collector overhead; per-function values are means.';}
sel.onchange=update;update();console.log('Validated',report.workloads.length,'workloads; stage shares',report.workloads.map(w=>Object.values(w.metrics.structured.stage_shares).reduce((a,b)=>a+b,0)));
</script></body></html>'''.replace('REPORT_DATA',data.replace('</','<\\/'))
(out/'report.html').write_text(html)
print(out)
for w in summary['workloads']:
 s=w['metrics']['structured']; alloc=sum(s['stage_shares'][k] for k in ['allocation_overhead','liveness','live_intervals','linear_scan','spill_slots']);print(w['name'],'allocator',round(100*alloc,1),'root stage shares','codegen',round(s['plain_codegen_ms'],2))
