"""One-session result-pool experiment. Run only in a private x86-64 Linux working directory.
Uses the committed initializer harness's strict output gate and corpus options.
Every child command, failure, and raw sample is retained; no profile collection.
"""
import hashlib, importlib.util, json, os, pathlib, random, shutil, statistics, subprocess, sys, tarfile, time
R=pathlib.Path(__file__).resolve().parent
E=R/'evidence'; E.mkdir(exist_ok=True)
BASE='cbe16154722b48831b1ece7968ebd80af35ed638'
CAND='71565f0'
ENV=dict(os.environ,CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='0')
def sha(p): return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def save(n,x): (E/(n+'.json')).write_text(json.dumps(x,indent=2)+'\n')
def run(cmd,cwd=None,check=True,env=None):
    cmd=list(map(str,cmd)); t=time.perf_counter(); p=subprocess.run(cmd,cwd=cwd,env=env or ENV,text=True,capture_output=True)
    x=dict(command=cmd,cwd=str(cwd),utc=time.time(),wall_ms=(time.perf_counter()-t)*1000,returncode=p.returncode,stdout=p.stdout,stderr=p.stderr)
    with (E/'commands.jsonl').open('a') as f: f.write(json.dumps(x)+'\n')
    if check and p.returncode: raise RuntimeError(x)
    return x
spec=importlib.util.spec_from_file_location('h',R/'base/suite/measure_init_snapshot.py'); h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
h.ROOT=R; h.E=E; h.ENV=ENV; h.run=run

def source(v,mode): return R/(v+'-'+mode+'-'+('cbe1615' if v=='base' else CAND))
def binary(v,mode,name): return R/('target-'+source(v,mode).name)/'release'/name

def setup():
    a=R/'base/suite/reports/c-frontend-refresh-20261016/corpus.tar.gz'
    assert sha(a)=='8cb3f123d55000f44587cc01c8a163deeb801411694bce0f915dbc4d93c0d408'
    with tarfile.open(a) as t:
        assert len(t.getmembers())==187; t.extractall(R/'corpus',filter='data')
    save('host-before',run(['sh','-c','hostname; date -u; rustc -Vv; cargo -V; clang --version; lscpu; taskset -pc $$; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; df -h .; ps -eo pid,ppid,pcpu,etime,args --sort=-pcpu | head -25']))
    for v in ['base','candidate']:
        for mode in ['plain','stage']:
            dst=source(v,mode); dst.mkdir(exist_ok=False)
            for n in ['Cargo.toml','Cargo.lock']: shutil.copyfile(R/v/n,dst/n)
            for n in ['crates','.cargo']: shutil.copytree(R/v/n,dst/n)
            (dst/'crates/keel-c/examples/pool_resident.rs').write_text(h.RESIDENT)
            save(dst.name+'-source',{str(p.relative_to(dst)):sha(p) for p in sorted(dst.rglob('*')) if p.is_file()})
    # Replay the sealed, explicitly enumerated synthetic Darwin declaration edits.
    plan=json.loads((R/'base/suite/reports/init-snapshot-validation-20261016/adaptation-plan.json').read_text())
    manifest={}
    for name,e in plan['files'].items():
        p=R/'corpus/inputs'/name; data=p.read_bytes(); assert sha(p)==e['original_sha256']
        chunks=[];cursor=0
        for ed in e['edits']:
            offset=ed['offset'];old=ed['old'].encode();new=ed['new'].encode()
            assert data[offset:offset+len(old)]==old
            first=old.index(b'"')+1;assert new==old[:first]+b'_'+old[first:]
            chunks.extend([data[cursor:offset],new]);cursor=offset+len(old)
        chunks.append(data[cursor:]);dst=R/'darwin-inputs'/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(b''.join(chunks))
        manifest[name]=dict(original=sha(p),adapted=sha(dst))
    save('adaptation',manifest)
    save('inputs',{str(p.relative_to(R/'corpus')):sha(p) for p in sorted((R/'corpus').rglob('*')) if p.is_file()})
    save('identity',dict(base=BASE,candidate=CAND,harness=sha(__file__),environment={k:v for k,v in ENV.items() if k.startswith(('CARGO','RUST','CC','LD_'))}))

def build():
    for v in ['base','candidate']:
        for mode in ['plain','stage']:
            dst=source(v,mode);env=dict(ENV,CARGO_TARGET_DIR=str(R/('target-'+dst.name)))
            cmd=['cargo','build','--release','--locked','-p','keel-c']
            if mode=='plain':cmd+=['-p','keel','--bin','keel','--example','pool_resident','--example','profile_allocations']
            else:cmd+=['--features','profile','--example','profile']
            save(dst.name+'-build',run(cmd,dst,env=env))
            names=['keel','examples/pool_resident','examples/profile_allocations'] if mode=='plain' else ['examples/profile']
            save(dst.name+'-binaries',{n:dict(path=str(binary(v,mode,n)),sha256=sha(binary(v,mode,n)),bytes=binary(v,mode,n).stat().st_size) for n in names})
            target=R/('target-'+dst.name)/'release/.fingerprint'
            save(dst.name+'-fingerprints',{str(p.relative_to(target)):json.loads(p.read_text()) for p in target.rglob('*.json')})

def parity():
    rows=[]
    for target in ['x86','darwin']:
        for d in ['normal','debug']:
            for a,ps in h.inputs().items():
                if target=='darwin':ps=[R/'darwin-inputs'/p.relative_to(R/'corpus/inputs') for p in ps]
                dests=[]
                for v in ['base','candidate']:
                    dest=E/'ir'/target/d/a/v;dest.mkdir(parents=True,exist_ok=False)
                    run([binary(v,'plain','examples/pool_resident'),'ir',target,d,dest,*ps])
                    m=h.output_manifest(dest,ps);rows.append(dict(target=target,debug=d,app=a,variant=v,files=m));dests.append(dest)
                h.compare_outputs(*dests,ps)
    # Header-free fixture and genuine late IRgen error are separate cohorts.
    fixture=R/'candidate/crates/keel-c/tests/fixtures/initializer_snapshot.c'
    error=R/'irgen-failure.c';error.write_text('double late(__int128 x) { int a = 42; return (double)(x + a); }\n')
    for target in ['x86','darwin']:
        for d in ['normal','debug']:
            for label,p in [('header-free',fixture),('late-error',error)]:
                dests=[]
                for v in ['base','candidate']:
                    dest=E/'ir'/target/d/label/v;dest.mkdir(parents=True,exist_ok=False)
                    run([binary(v,'plain','examples/pool_resident'),'ir',target,d,dest,p]);dests.append(dest)
                if label=='header-free': h.compare_outputs(*dests,[p])
                else:
                    output=[next(x.iterdir()).read_text() for x in dests]
                    expected='Diagnostic { file: '+json.dumps(str(p))+', line: 1, col: 46, message: '+json.dumps('`__int128` to float is not supported')+' }'
                    assert output==[expected,expected]
                    save('late-error-'+target+'-'+d,dict(path=str(p),diagnostic=output[0]))
    save('parity',rows)

def stage():
    rng=random.Random(16102026);rows=[]
    for i in range(11):
        jobs=[(a,d) for a in h.inputs() for d in ['normal','debug']];rng.shuffle(jobs)
        for a,d in jobs:
            ps=h.inputs()[a];ps=ps[i%len(ps):]+ps[:i%len(ps)]
            for v in (['base','candidate'] if i%2==0 else ['candidate','base']):
                x=run([binary(v,'stage','examples/profile'),'staged','1',*ps,*(['--debug'] if d=='debug' else [])])
                rows.append(dict(round=i,app=a,debug=d,variant=v,rows=[json.loads(s) for s in x['stdout'].splitlines()]))
        save('stage',rows);print('stage',i+1,flush=True)

def cli():
    rng=random.Random(16102027);rows=[]
    def one(a,d,m,i,v):
        ps=h.inputs()[a] if m=='pre-ir' else h.rawinputs()[a];ps=ps[i%len(ps):]+ps[:i%len(ps)]
        out=[]
        for p in ps:
            cmd=[binary(v,'plain','keel'),'cc',p,*(['--emit-ir'] if m=='pre-ir' else ['-O',*h.flags(a)]),*(['-g'] if d=='debug' else []),'-o','/dev/null']
            x=run(cmd);out.append(dict(path=str(p),wall_ms=x['wall_ms']))
        return dict(round=i,app=a,debug=d,mode=m,variant=v,rows=out)
    jobs=[(a,d,m) for a in h.inputs() for d in ['normal','debug'] for m in ['pre-ir','raw-opt-asm']]
    warm=[]
    for a,d,m in jobs:
        for v in ['base','candidate']:warm.append(one(a,d,m,0,v))
    save('cli-warmup',warm)
    for i in range(7):
        rng.shuffle(jobs)
        for a,d,m in jobs:
            for v in (['base','candidate'] if i%2==0 else ['candidate','base']):rows.append(one(a,d,m,i,v))
        save('cli',rows);print('cli',i+1,flush=True)

def public():
    rows=[]
    for a,ps in h.inputs().items():
        for d in ['normal','debug']:
            for v in ['base','candidate']:
                x=run([binary(v,'plain','examples/profile_allocations'),*ps,*(['--debug'] if d=='debug' else [])]);rows.append(dict(app=a,debug=d,variant=v,rows=[json.loads(s) for s in x['stdout'].splitlines()]))
    save('public',rows)

def summarize():
    def stats(pairs):
        b=[x[0] for x in pairs];c=[x[1] for x in pairs];ratios=[y/x for x,y in pairs];rng=random.Random(16)
        boots=sorted(statistics.median(rng.choices(ratios,k=len(ratios))) for _ in range(10000))
        return dict(base_median=statistics.median(b),candidate_median=statistics.median(c),paired_ratio=statistics.median(ratios),ci95=[boots[249],boots[9749]],ratios=ratios,base=b,candidate=c,ratio_mad=statistics.median(abs(x-statistics.median(ratios)) for x in ratios))
    result={}
    for mode in ['stage','cli']:
        rows=json.loads((E/(mode+'.json')).read_text());n=11 if mode=='stage' else 7
        for d in ['normal','debug']:
            for a in ['lua','quickjs','sqlite','all']:
                for metric in (['irgen_ms','cleanup_ms','module_drop_ms','total_ms'] if mode=='stage' else ['pre-ir','raw-opt-asm']):
                    pairs=[]
                    for i in range(n):
                        pair=[]
                        for v in ['base','candidate']:
                            rs=[x for x in rows if x['round']==i and x['debug']==d and x['variant']==v and (a=='all' or x['app']==a) and (mode=='stage' or x['mode']==metric)]
                            pair.append(sum(y[metric if mode=='stage' else 'wall_ms'] for x in rs for y in x['rows']))
                        pairs.append(pair)
                    result['/'.join([mode,d,a,metric])]=stats(pairs)
    save('summary',result)

if __name__=='__main__':
    for op in sys.argv[1:]: globals()[op]()
