"""Weight every perf event, retaining empty/unknown/incomplete call chains.
Physical FP chains are lower-bound inclusive attribution; tail unknown frames
are retained. Event period, not event count, is the CPU-cycle denominator.
"""
import collections,json,pathlib,re,sys

def decode(path, helper_main="profile::main"):
    selfw=collections.Counter();incw=collections.Counter();folded=collections.Counter();selfn=collections.Counter()
    totals=collections.Counter();weights=[]
    for block in path.read_text().strip().split('\n\n'):
        lines=block.splitlines()
        if not lines:continue
        m=re.search(r':\s+(\d+)\s+cycles:u:\s*$',lines[0])
        if not m:raise ValueError(lines[0])
        weight=int(m[1]);frames=[]
        for line in lines[1:]:
            f=re.match(r'\s*[0-9a-f]+ (.+) \((.*)\)\s*$',line)
            if not f:raise ValueError(line)
            frames.append(f[1])
        totals['samples']+=1;totals['weight']+=weight;weights.append(weight)
        leaf=frames[0] if frames else '[no unwind frames]'
        selfw[leaf]+=weight;selfn[leaf]+=1
        for name in set(frames):incw[name]+=weight
        folded[';'.join(frames) if frames else '[no unwind frames]']+=weight
        for key,condition in [('empty_chain',not frames),('unknown_leaf',not frames or '[unknown]' in leaf),('any_unknown_frame',any('[unknown]' in f for f in frames)),('does_not_reach_helper_main',helper_main not in frames)]:
            if condition:totals[key+'_samples']+=1;totals[key+'_weight']+=weight
    return {'raw_script':path.name,'totals':dict(totals),'periods':weights,'self_weight':selfw.most_common(),'self_samples':selfn.most_common(),'inclusive_weight':incw.most_common(),'folded_weight':folded.most_common()}

def main(root):
    r={}
    for app in ['lua','quickjs','sqlite']:
        for i in range(2):r[f'{app}:{i}']=decode(root/f'{app}-{i}.perf.script.txt')
    for app in ['lua','quickjs','sqlite']:
        for i in range(2):
            p=root/f'preprocess-{app}-{i}.perf.script.txt'
            if p.exists():r[f'preprocess-{app}:{i}']=decode(p,'preprocess_linux::main')
    (root/'decoded.json').write_text(json.dumps(r,indent=2)+'\n')
    for k,x in r.items():
        total=x['totals']['weight'];print(k,x['totals'])
        print('SELF',[(n,round(v/total*100,2)) for n,v in x['self_weight'][:12]])
        print('INCL',[(n,round(v/total*100,2)) for n,v in x['inclusive_weight'] if n.startswith('keel_c::')][:15])
if __name__=='__main__':main(pathlib.Path(sys.argv[1]))
