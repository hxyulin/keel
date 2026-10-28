#!/usr/bin/env python3
"""Audit a completed fixed-backing packet and report set/block paired effects."""
import argparse
import hashlib
import json
import math
import pathlib
import random
import statistics

def percentile(values,q):
    values=sorted(values);index=(len(values)-1)*q;lo=int(index);hi=min(lo+1,len(values)-1)
    return values[lo]+(values[hi]-values[lo])*(index-lo)

def analyze(output):
    protocol=json.loads(output.joinpath('protocol.json').read_text());cfg=protocol['config']
    records=[json.loads(x) for x in output.joinpath('raw.jsonl').read_text().splitlines()]
    sets=[]
    for group in range(cfg['groups']):
        pair=[p for p in protocol['placements'] if p['group']==group]
        assert len(pair)==2 and {p['order'] for p in pair}=={'sorted','shuffled'}
        assert set(pair[0]['pfns'])==set(pair[1]['pfns'])
        assert all(not set(pair[0]['pfns'])&previous for previous in sets)
        sets.append(set(pair[0]['pfns']))
    rows=[r for r in records if r['kind']=='result']
    assert records[-1]['kind']=='release' and records[-1]['status']=='COMPLETE' and records[-1]['owned_jobs']==0
    assert not any(r['kind']=='failure' for r in records)
    for r in rows:
        assert r['rc']==0 and r['stderr_sha256']==hashlib.sha256(b'').hexdigest()
        spec=cfg['workloads'][r['w']]
        assert r['stdout_sha256']==spec['expected']['sha256']
        assert r['settings']==dict(policy=0,nice=0,affinity=[cfg['cpu']]) and r['uids']==[cfg['uid']]*4
        placement=protocol['placements'][r['placement']]
        physical=[x&((1<<55)-1) for x in r['pagemap_entries']]
        assert all(x>>63 for x in r['pagemap_entries'])
        assert physical==placement['pfns'][:len(physical)] and len(physical)*4096==r['code_length']
    route_count=len(cfg['workloads'])*len(protocol['placements'])*3
    assert len([r for r in rows if r['phase']=='gate'])==route_count
    if protocol['check_only']:
        assert len(rows)==route_count
        return dict(status='CORRECTNESS_GATES_PASS',calls=len(rows))
    assert len(rows)==route_count*(1+(cfg['blocks']+2)*cfg['repetitions'])
    for block,order in enumerate(protocol['schedule']):
        actual=[(r['w'],r['placement'],r['arm']) for r in rows if r['block']==block]
        assert actual==[tuple(x) for cell in order for x in [cell]*cfg['repetitions']]
    measured=[r for r in rows if r['phase']=='measured']
    lookup={}
    for r in measured:lookup.setdefault((r['w'],r['group'],r['order'],r['block'],r['arm']),[]).append(r['wall_ns'])
    results={}
    for w,workload in enumerate(cfg['workloads']):
        matrices={key:[] for key in ['control','optimized']}
        group_layouts=[]
        for group in range(cfg['groups']):
            group_logs={key:[] for key in matrices}
            layout_stats={}
            for layout in ['sorted','shuffled']:
                logs={key:[] for key in matrices}
                for block in range(2,cfg['blocks']+2):
                    a,b,n=[sum(lookup[(w,group,layout,block,label)]) for label in cfg['arms']]
                    logs['control'].append(math.log(b/a))
                    logs['optimized'].append(math.log(n)-.5*(math.log(a)+math.log(b)))
                layout_stats[layout]={k:math.exp(statistics.mean(v)) for k,v in logs.items()}
                for key in matrices:group_logs[key].append(logs[key])
            for key in matrices:
                matrices[key].append([statistics.mean(group_logs[key][layout][block] for layout in range(2)) for block in range(cfg['blocks'])])
            group_layouts.append(layout_stats)
        rng=random.Random(cfg['seed']+100+w)
        draws={key:[] for key in matrices}
        for _ in range(10000):
            groups=[rng.randrange(cfg['groups']) for _ in range(cfg['groups'])]
            blocks=[rng.randrange(cfg['blocks']) for _ in range(cfg['blocks'])]
            for key,matrix in matrices.items():
                draws[key].append(math.exp(statistics.mean(matrix[g][b] for g in groups for b in blocks)))
        estimates={key:dict(ratio=math.exp(statistics.mean(statistics.mean(row) for row in matrix)),conditional_95ci=[percentile(draws[key],.025),percentile(draws[key],.975)]) for key,matrix in matrices.items()}
        control=estimates['control']
        adequate=control['conditional_95ci'][0]>=.98 and control['conditional_95ci'][1]<=1.02 and all(.95<=v[layout]['control']<=1.05 for v in group_layouts for layout in ['sorted','shuffled'])
        results[workload['id']]=dict(controls_adequate=adequate,estimates=estimates,per_set_and_layout=group_layouts)
    return dict(status='CONTROLS_PASS_FINITE_COMPARISON' if all(v['controls_adequate'] for v in results.values()) else 'CONTROLS_FAIL_PROFIT_UNRESOLVED',calls=len(rows),measured_calls=len(measured),physical_sets=cfg['groups'],workloads=results,estimand=protocol['estimand'],limits='Conditional fixed-layout, warm-page, post-loader application wall time. Both layouts share frames within each set; bootstrap resamples sets and timing blocks, not individual launches. One machine/session, no multiple-comparison correction or ordinary-deployment claim.')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('output',type=pathlib.Path)
    args=parser.parse_args();result=analyze(args.output)
    args.output.joinpath('summary.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
