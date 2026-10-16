"""Lossless raw evidence summarizer; bootstrap resamples complete paired rounds."""
import collections,gzip,json,math,pathlib,random,re,statistics,sys

def load(root,n):
    p=root/(n+'.json')
    return json.loads(p.read_text() if p.exists() else gzip.decompress((root/(n+'.json.gz')).read_bytes()))
def describe(xs):
    rng=random.Random(20261016);bs=sorted(statistics.median(rng.choices(xs,k=len(xs))) for _ in range(10000))
    med=statistics.median(xs)
    return {'samples':xs,'n':len(xs),'median':med,'min':min(xs),'max':max(xs),'mad':statistics.median(abs(x-med) for x in xs),'median_ci95':[bs[249],bs[9749]]}
def main(root):
    r=load(root,'resident');out={'resident':{},'per_tu':{},'cli':{},'preprocess':{}}
    for a in ['lua','quickjs','sqlite']:
        for d in [False,True]:
            key=f'{a}:{d}';out['resident'][key]={}
            totals={}
            for m in ['baseline','staged']:
                rows=[x for x in r['samples'] if x['app']==a and x['debug']==d and x['mode']==m]
                keys=['total_ms','module_drop_ms']+(['lex_ms','parse_ms','irgen_ms','cleanup_ms'] if m=='staged' else [])
                out['resident'][key][m]={k:describe([sum(y[k] for y in x['rows']) for x in rows]) for k in keys}
                totals[m]=[sum(y['total_ms'] for y in x['rows']) for x in rows]
                if m=='staged':
                    paths=sorted(set(y['path'] for x in rows for y in x['rows']))
                    tu=[]
                    for p in paths:
                        ys=[y for x in rows for y in x['rows'] if y['path']==p]
                        tu.append({'path':p,'bytes':ys[0]['bytes'],'functions':ys[0]['functions'],'tokens':ys[0]['tokens'],**{k:describe([y[k] for y in ys]) for k in keys}})
                    out['per_tu'][key]=sorted(tu,key=lambda x:-x['total_ms']['median'])
            out['resident'][key]['staged_plain_paired_ratio']=describe([x/y for x,y in zip(totals['staged'],totals['baseline'])])
    if (root/'cli.json').exists() or (root/'cli.json.gz').exists():
        c=load(root,'cli')
        for a in ['lua','quickjs','sqlite']:
            for m in ['raw-ir','pre-ir','raw-fullopt']:
                out['cli'][f'{a}:{m}']=describe([x['total_ms'] for x in c['samples'] if x['app']==a and x['mode']==m])
    if (root/'preprocess.json').exists() or (root/'preprocess.json.gz').exists():
        p=load(root,'preprocess')
        for a in ['lua','quickjs','sqlite']:
            out['preprocess'][a]=describe([sum(y['preprocess_ms'] for y in x['rows']) for x in p['samples'] if x['app']==a])
    exists=lambda n:(root/(n+'.json')).exists() or (root/(n+'.json.gz')).exists()
    if exists('diagnostics'):
        d=load(root,'diagnostics');out['cli_diagnostic_context']={}
        for a in ['lua','quickjs','sqlite']:
            rs=[x for x in d['cli_phases'] if x['app']==a];v={}
            for phase in ['preprocess','frontend','verify','optimize','backend_output']:
                v[phase+'_ms']=describe([sum(int(re.search('PHASE '+phase+'_ns (\\d+)',x['stderr'])[1])/1e6 for x in rs if x['round']==i) for i in range(11)])
            for key,pattern,col in [('optimizer_verification_ms',r'OPT_CONTEXT \S+ total_ns=(\d+) verify_ns=(\d+)',1),('call_contracts_total_ms',r'OPT_CONTEXT call-contracts total_ns=(\d+) verify_ns=(\d+)',0),('call_contracts_verify_ms',r'OPT_CONTEXT call-contracts total_ns=(\d+) verify_ns=(\d+)',1)]:
                v[key]=describe([sum(int(t[col]) for x in rs if x['round']==i for t in re.findall(pattern,x['stderr']))/1e6 for i in range(11)])
            out['cli_diagnostic_context'][a]=v
    if exists('memory'):
        r=load(root,'memory');out['rss_mib']={}
        for a in ['lua','quickjs','sqlite']:
            for d in [False,True]:
                xs=[int(re.search(r'RSS_KIB (\d+)',x['stderr'])[1])/1024 for x in r['runs'] if x['app']==a and x['debug']==d]
                out['rss_mib'][f'{a}:{d}']={'observations':xs,'median':statistics.median(xs),'min':min(xs),'max':max(xs)}
    if exists('allocations'):
        out['allocations']={}
        for x in load(root,'allocations')['runs']:
            rows=[json.loads(s) for s in x['stdout'].splitlines()]
            out['allocations'][f"{x['app']}:{x['debug']}"]={k:sum(y[k] for y in rows) for k in ['alloc','realloc','dealloc','requested_bytes']}
    if exists('preprocess-allocations'):
        out['preprocess_allocations']={}
        for a,x in load(root,'preprocess-allocations')['runs'].items():
            rows=[json.loads(s) for s in x['stdout'].splitlines()]
            out['preprocess_allocations'][a]={k:sum(y[k] for y in rows) for k in ['alloc','realloc','dealloc','requested_bytes','output_bytes']}
    out['scaling']={}
    for a in ['lua','quickjs','sqlite']:
        ps=out['per_tu'][a+':False'];xs=[p['bytes'] for p in ps];ys=[p['total_ms']['median'] for p in ps]
        out['scaling'][a]={'translation_units':len(ps),'bytes':sum(xs),'tokens':sum(p['tokens'] for p in ps),'functions':sum(p['functions'] for p in ps),'median_time_sum_ms_per_decimal_mb':sum(ys)/sum(xs)*1e6,'byte_time_pearson':(sum((x-statistics.mean(xs))*(y-statistics.mean(ys)) for x,y in zip(xs,ys))/math.sqrt(sum((x-statistics.mean(xs))**2 for x in xs)*sum((y-statistics.mean(ys))**2 for y in ys))) if len(xs)>2 else None}
    if (root/'tests.log').exists():
        text=(root/'tests.log').read_text();audit=[]
        for m in re.finditer(r'  left: (\[[^\n]*\])\n right: (\[[^\n]*\])',text):
            left,right=map(json.loads,m.groups())
            norm=lambda xs:[re.split(r'\s+(?:Loc=<|\[)',x,maxsplit=1)[0] for x in xs]
            audit.append({'same_after_removing_clang_metadata':norm(left)==norm(right),'left_count':len(left),'right_count':len(right)})
        out['preprocessor_test_format_audit']=audit
    (root/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
    for k,v in out['resident'].items():
        print(k, 'public',round(v['baseline']['total_ms']['median'],3),'stages', {k:round(x['median'],3) for k,x in v['staged'].items()})
    for kind in ['cli','preprocess']:
        print(kind,{k:round(v['median'],3) for k,v in out[kind].items()})
if __name__=='__main__':main(pathlib.Path(sys.argv[1]))
