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
    (root/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
    for k,v in out['resident'].items():
        print(k, 'public',round(v['baseline']['total_ms']['median'],3),'stages', {k:round(x['median'],3) for k,x in v['staged'].items()})
    for kind in ['cli','preprocess']:
        print(kind,{k:round(v['median'],3) for k,v in out[kind].items()})
if __name__=='__main__':main(pathlib.Path(sys.argv[1]))
