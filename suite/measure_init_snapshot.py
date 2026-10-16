"""Frozen-revision LinuxHost validation and paired measurement; additive helpers only.
Run from a private remote root containing baseline/, candidate/ and this script.
All subprocess commands, output, failures and negative samples are retained.
"""
import hashlib,json,os,pathlib,random,shutil,statistics,subprocess,sys,tarfile,time
ROOT=pathlib.Path(__file__).resolve().parent
E=ROOT/'evidence'; E.mkdir(exist_ok=True)
BASE='ea14dcf3cdc367c7c92e50cf2df05be09eb8dba9'
CAND='c2b6e1ae3d09a01e50ac92813f4d149e5765966f'
ENV=dict(os.environ,CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='0')
def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def save(n,v):(E/(n+'.json')).write_text(json.dumps(v,indent=2)+'\n')
def run(cmd,cwd=None,check=True,env=None):
    cmd=list(map(str,cmd)); start=time.time(); t=time.perf_counter()
    p=subprocess.run(cmd,cwd=cwd,env=env or ENV,text=True,capture_output=True)
    r=dict(command=cmd,cwd=str(cwd) if cwd else None,start=start,wall_ms=(time.perf_counter()-t)*1000,returncode=p.returncode,stdout=p.stdout,stderr=p.stderr)
    with (E/'commands.jsonl').open('a') as f:f.write(json.dumps(r)+'\n')
    if check and p.returncode:raise RuntimeError(r)
    return r
RESIDENT=r'''
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let a: Vec<_> = std::env::args().collect();
 let target = if a[2] == "x86" {keel_ir::Target::X86_64LinuxGnu} else {keel_ir::Target::Aarch64AppleDarwin};
 let opts=keel_c::Options {target,debug_info:a[3]=="debug"};
 let inputs:Vec<_>=a[5..].iter().map(|p|Ok((p,std::fs::read_to_string(p)?))).collect::<Result<_,std::io::Error>>()?;
 for (path,src) in &inputs {
  match keel_c::compile(src,path,opts) {
   Ok(m)=>{keel_ir::verify_module(&m).map_err(|e|format!("{e:?}"))?;
    if a[1]=="ir" {std::fs::create_dir_all(&a[4])?;std::fs::write(std::path::Path::new(&a[4]).join(std::path::Path::new(path).file_name().unwrap()).with_extension("ir"),format!("{m:?}"))?;}
   },
   Err(e)=>{if a[1]=="ir" {std::fs::create_dir_all(&a[4])?;std::fs::write(std::path::Path::new(&a[4]).join(std::path::Path::new(path).file_name().unwrap()).with_extension("err"),format!("{e:?}"))?;}else{return Err(e.into());}}
  }
 }
 if a[1]=="ir" {return Ok(());}
 for (path,src) in &inputs {
  let t=Instant::now();let m=keel_c::compile(black_box(src),path,opts)?;let compile_ms=t.elapsed().as_secs_f64()*1000.;
  let t=Instant::now();drop(black_box(m));let module_drop_ms=t.elapsed().as_secs_f64()*1000.;
  println!("{{\"path\":{path:?},\"compile_ms\":{compile_ms},\"module_drop_ms\":{module_drop_ms}}}");
 }
 Ok(())
}
'''
def setup():
    archive=ROOT/'baseline/suite/reports/c-frontend-refresh-20261016/corpus.tar.gz'
    assert sha(archive)=='8cb3f123d55000f44587cc01c8a163deeb801411694bce0f915dbc4d93c0d408'
    with tarfile.open(archive) as t:
        assert len(t.getmembers())==187;t.extractall(ROOT/'corpus',filter='data')
    for v in ['baseline','candidate']:
        (ROOT/v/'crates/keel-c/examples/snapshot_resident.rs').write_text(RESIDENT)
    save('setup',{'base':BASE,'candidate':CAND,'branch':'perf/init-snapshot-gate','root':str(ROOT),'harness_sha256':sha(__file__),'archive_sha256':sha(archive),'environment':{k:v for k,v in ENV.items() if k.startswith(('CARGO','RUST','CC','CXX','LD_'))},'host':run(['sh','-c','hostname; uname -a; rustc -Vv; cargo -V; clang --version; lscpu; taskset -pc $$; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; df -h .; ps -eo pid,ppid,pcpu,etime,args --sort=-pcpu | head -25'])})
    for v in ['baseline','candidate']:
        save(v+'-source',{'revision':BASE if v=='baseline' else CAND,'files':{str(p.relative_to(ROOT/v)):sha(p) for p in sorted((ROOT/v).rglob('*')) if p.is_file() and (p.suffix=='.rs' or p.name in ['Cargo.lock','Cargo.toml'])}})
def build():
    for v in ['baseline','candidate']:
        env=dict(ENV,CARGO_TARGET_DIR=str(ROOT/('target-'+v+'-release')))
        r=run(['cargo','build','--release','--locked','-p','keel','-p','keel-c','--example','snapshot_resident','--example','profile_allocations','--bin','keel'],ROOT/v,env=env);save(v+'-build',r)
        for n in ['keel','examples/snapshot_resident','examples/profile_allocations']:
            b=ROOT/('target-'+v+'-release/release')/n
            save(v+'-'+n.replace('/','-'),{'path':str(b),'sha256':sha(b),'size':b.stat().st_size})
def checks():
    out={}
    for v in ['baseline','candidate']:
        env=dict(ENV,CARGO_TARGET_DIR=str(ROOT/('target-'+v+'-check')))
        out[v]=[]
        for args in [['-p','keel-c','--test','initializers'],['--workspace','--no-fail-fast']]:
            r=run(['cargo','test','--locked',*args],ROOT/v,env=env,check=False);out[v].append(r);save('checks',out)
        assert out[v][0]['returncode']==0

def inputs():
    d={a:sorted((ROOT/'corpus/inputs'/a/'pre').glob('*.i')) for a in ['lua','quickjs','sqlite']}
    assert [len(x) for x in d.values()]==[33,9,2];return d

def binary(v,name='examples/snapshot_resident'):return ROOT/('target-'+v+'-release/release')/name

def parity():
    cases=inputs();fix=ROOT/'fixtures';fix.mkdir(exist_ok=True)
    shutil.copy(ROOT/'candidate/crates/keel-c/tests/fixtures/initializer_snapshot.c',fix/'initializer_snapshot.c')
    invalid=['int a[2] = {[1 ... 0] = 3};','int a[2] = {[0 ... 2] = 3};','int a[1] = {1, 2};','int a[2] = {.x = 1};','int a[2] = {[0 ... 1] = };','struct S { int x; }; struct S a = {.bad = 1};','int a[2] = {[0 ... 1] = {1, 2}};','int f(void) { int a[2] = {[0 ... 1] = (1 + )}; return 0; }']
    for i,s in enumerate(invalid):(fix/f'bad{i}.c').write_text('# 71 "malformed-initializer.c"\n'+s+'\n')
    cases['fixtures']=sorted(fix.glob('*.c'));result=[]
    for a,ps in cases.items():
        for target in ['x86','arm']:
            for d in ['normal','debug']:
                manifests=[]
                for v in ['baseline','candidate']:
                    dest=E/'ir'/a/target/d/v
                    r=run([binary(v),'ir',target,d,dest,*ps]);m={p.name:sha(p) for p in dest.iterdir()};assert len(m)==len(ps);manifests.append(m)
                    result.append(dict(app=a,target=target,debug=d,variant=v,files=m,command=r['command']))
                assert manifests[0]==manifests[1],(a,target,d)
    save('parity',result)
    save('inputs',{str(p.relative_to(ROOT/'corpus')):{'sha256':sha(p),'bytes':p.stat().st_size} for p in sorted((ROOT/'corpus').rglob('*')) if p.is_file()})

def resident():
    rng=random.Random(71620261016);r=[];pss=inputs()
    for i in range(13):
        jobs=[(a,d) for a in pss for d in ['normal','debug']];rng.shuffle(jobs)
        for a,d in jobs:
            ps=pss[a];ps=ps[i%len(ps):]+ps[:i%len(ps)]
            order=['baseline','candidate'] if i%2==0 else ['candidate','baseline']
            for v in order:
                x=run([binary(v),'time','x86',d,'unused',*ps]);r.append(dict(round=i,app=a,debug=d,variant=v,rows=[json.loads(s) for s in x['stdout'].splitlines()],command=x['command']))
        save('resident',r);print('resident',i+1,flush=True)

def rawinputs():
    r=ROOT/'corpus/raw'
    return {'lua': sorted(p for p in (r/'lua/lua-5.4.7/src').glob('*.c') if p.name!='luac.c'), 'quickjs':[r/'quickjs/quickjs-2024-01-13'/n for n in ['qjs.c','quickjs.c','libregexp.c','libunicode.c','cutils.c','quickjs-libc.c','libbf.c']]+[r/'quickjs/gen'/n for n in ['repl.c','qjscalc.c']], 'sqlite':[r/'sqlite/sqlite-amalgamation-3460100'/n for n in ['shell.c','sqlite3.c']]}
def flags(a):return {'lua':['-DLUA_USE_LINUX=1'],'quickjs':['-D_GNU_SOURCE=1','-DCONFIG_VERSION="2024-01-13"','-DCONFIG_BIGNUM=1','-I',str(ROOT/'corpus/raw/quickjs/quickjs-2024-01-13')],'sqlite':['-DSQLITE_THREADSAFE=1','-D_GNU_SOURCE=1','-DLONGDOUBLE_TYPE=double']}[a]
def cli():
    pre=inputs();raw=rawinputs();rng=random.Random(71720261016);r=[];warm=[]
    def one(a,d,m,i,v):
        ps=pre[a] if m=='pre-ir' else raw[a];ps=ps[i%len(ps):]+ps[:i%len(ps)];rows=[]
        for p in ps:
            cmd=[binary(v,'keel'),'cc',p,*(['--emit-ir'] if m=='pre-ir' else ['-O',*flags(a)]),*(['-g'] if d=='debug' else []),'-o','/dev/null'];x=run(cmd);rows.append(x)
        return dict(round=i,app=a,debug=d,mode=m,variant=v,total_ms=sum(x['wall_ms'] for x in rows),rows=rows)
    jobs=[(a,d,m) for a in pre for d in ['normal','debug'] for m in ['pre-ir','raw-opt-asm']]
    for a,d,m in jobs:
        for v in ['baseline','candidate']:warm.append(one(a,d,m,0,v));save('cli-warmups',warm)
    for i in range(11):
        rng.shuffle(jobs)
        for a,d,m in jobs:
            for v in (['baseline','candidate'] if i%2==0 else ['candidate','baseline']):r.append(one(a,d,m,i,v))
        save('cli',r);print('cli',i+1,flush=True)

def allocations():
    r=[]
    for repeat in range(2):
        for a,ps in inputs().items():
            for d in ['normal','debug']:
                for v in ['baseline','candidate']:
                    x=run([binary(v,'examples/profile_allocations'),*ps,*(['--debug'] if d=='debug' else [])]);r.append(dict(repeat=repeat,app=a,debug=d,variant=v,rows=[json.loads(s) for s in x['stdout'].splitlines()],command=x['command']))
    save('allocations',r)

# Copied-only diagnostic. Fixed TLS counters allocate no storage on the heap.
# The allocator window covers only the source-identified to_vec expression.
PROBE=r'''
use std::{alloc::{GlobalAlloc,Layout,System},cell::Cell};
thread_local! {
 static ENABLED:Cell<bool>=const {Cell::new(false)};
 static COUNTS:Cell<[u64;9]>=const {Cell::new([0;9])};
}
fn add(i:usize,n:u64){COUNTS.with(|c|{let mut a=c.get();a[i]+=n;c.set(a);});}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
 unsafe fn alloc(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc(l)};ENABLED.with(|e|if e.get(){add(5,1);add(7,l.size() as u64)});p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};ENABLED.with(|e|if e.get(){add(5,1);add(7,l.size() as u64)});p}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {let p=unsafe{System.realloc(p,l,n)};ENABLED.with(|e|if e.get(){add(6,1);add(7,n as u64)});p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){ENABLED.with(|e|if e.get(){add(8,1)});unsafe{System.dealloc(p,l)}}
}
#[global_allocator] static ALLOCATOR:Allocator=Allocator;
pub fn boundary(items:usize,n:usize){add(0,1);add(1,items as u64);add(4,(n-1) as u64);}
pub fn start(items:usize){add(2,1);add(3,items as u64);ENABLED.with(|e|{assert!(!e.get());e.set(true)});}
pub fn end(){ENABLED.with(|e|{assert!(e.get());e.set(false)});}
pub fn reset(){ENABLED.with(|e|e.set(false));COUNTS.with(|c|c.set([0;9]));}
pub fn take()->[u64;9]{COUNTS.with(|c|c.get())}
'''
ATTR=r'''
fn main()->Result<(),Box<dyn std::error::Error>> {
 let a:Vec<_>=std::env::args().collect();let debug_info=a[1]=="debug";
 for path in &a[3..] {
  let src=std::fs::read_to_string(path)?;keel_c::snapshot_probe::reset();
  let m=keel_c::compile(&src,path,keel_c::Options{debug_info,..Default::default()})?;
  let c=keel_c::snapshot_probe::take();keel_ir::verify_module(&m).map_err(|e|format!("{e:?}"))?;
  std::fs::create_dir_all(&a[2])?;std::fs::write(std::path::Path::new(&a[2]).join(std::path::Path::new(path).file_name().unwrap()).with_extension("ir"),format!("{m:?}"))?;
  println!("{{\"path\":{path:?},\"boundary_regions\":{},\"boundary_items\":{},\"snapshot_regions\":{},\"snapshot_items\":{},\"extra_iterations\":{},\"clone_alloc\":{},\"clone_realloc\":{},\"clone_requested_bytes\":{},\"clone_dealloc\":{}}}",c[0],c[1],c[2],c[3],c[4],c[5],c[6],c[7],c[8]);
 }
 Ok(())
}
'''
def attribution():
    r=[]
    for v in ['baseline','candidate']:
        dst=ROOT/(v+'-attribution');assert not dst.exists()
        shutil.copytree(ROOT/v,dst,ignore=shutil.ignore_patterns('suite','target'))
        lib=dst/'crates/keel-c/src/lib.rs';lib.write_text(lib.read_text()+'\npub mod snapshot_probe;\n')
        (dst/'crates/keel-c/src/snapshot_probe.rs').write_text(PROBE)
        p=dst/'crates/keel-c/src/init.rs';s=p.read_text();needle='            let (n, stride) = range;'
        assert s.count(needle)==1;s=s.replace(needle,needle+'\n            crate::snapshot_probe::boundary(items.len()-first,n);')
        needle="let one: Vec<InitItem<'a>> = items[first..].to_vec();";assert s.count(needle)==1
        s=s.replace(needle,'crate::snapshot_probe::start(items.len()-first);\n                '+needle+'\n                crate::snapshot_probe::end();');p.write_text(s)
        (dst/'crates/keel-c/examples/snapshot_attribution.rs').write_text(ATTR)
        save(v+'-attribution-source',{'files':{str(p.relative_to(dst)):sha(p) for p in sorted(dst.rglob('*')) if p.is_file()}})
        env=dict(ENV,CARGO_TARGET_DIR=str(ROOT/('target-'+v+'-attribution')))
        save(v+'-attribution-build',run(['cargo','build','--locked','--release','-p','keel-c','--example','snapshot_attribution'],dst,env=env))
        b=ROOT/('target-'+v+'-attribution/release/examples/snapshot_attribution');save(v+'-attribution-binary',{'path':str(b),'sha256':sha(b)})
        for repeat in range(2):
            for a,ps in inputs().items():
                for d in ['normal','debug']:
                    dest=E/'attribution-ir'/v/str(repeat)/a/d
                    x=run([b,d,dest,*ps]);rows=[json.loads(s) for s in x['stdout'].splitlines()]
                    expected=E/'ir'/a/'x86'/d/v
                    assert {p.name:sha(p) for p in dest.iterdir()}=={p.name:sha(p) for p in expected.iterdir()}
                    r.append(dict(repeat=repeat,app=a,debug=d,variant=v,rows=rows,command=x['command']))
        save('attribution',r)

def provenance():
    save('tools',{n:{'path':shutil.which(n),'resolved':str(pathlib.Path(shutil.which(n)).resolve()),'sha256':sha(pathlib.Path(shutil.which(n)).resolve())} for n in ['cargo','rustc','clang','python3']})
    for v in ['baseline','candidate']:
        t=ROOT/('target-'+v+'-release')
        save(v+'-fingerprints',{str(p.relative_to(t)):json.loads(p.read_text()) for p in sorted(t.glob('release/.fingerprint/*/*.json'))})
        save(v+'-size',run(['size',binary(v,'keel'),binary(v)]))
    save('measurement-host-before',run(['sh','-c','date -u; uptime; ps -eo pid,ppid,pcpu,etime,args --sort=-pcpu | head -25; taskset -pc $$']))

def summarize():
    def load(n):return json.loads((E/(n+'.json')).read_text())
    rng=random.Random(71820261016)
    def stats(pairs):
        ratios=[100*(c/b-1) for b,c in pairs];n=len(ratios)
        boot=sorted(statistics.median(rng.choices(ratios,k=n)) for _ in range(20000))
        return dict(pairs=n,baseline_median_ms=statistics.median(b for b,c in pairs),candidate_median_ms=statistics.median(c for b,c in pairs),paired_median_percent=statistics.median(ratios),bootstrap_95_percent=[boot[499],boot[19499]],paired_percent=ratios)
    out={'percent_convention':'candidate / baseline - 1; negative is faster','bootstrap':'20000 paired resamples of paired per-round percent changes; percentile 95%; seeds retained','resident':{},'cli':{},'allocation_reconciliation':{}}
    for label in ['resident','cli']:
        data=load(label); groups={}
        for x in data:
            key=(x['app'],x['debug'],x.get('mode','compile'))
            value=x['total_ms'] if label=='cli' else sum(y['compile_ms'] for y in x['rows'])
            groups.setdefault(key,{}).setdefault(x['round'],{})[x['variant']]=value
        # Aggregate same-round sums before calculating ratios, preserving pairs.
        for d in ['normal','debug']:
            for m in (['pre-ir','raw-opt-asm'] if label=='cli' else ['compile']):
                z={}
                for (a,debug,mode),rs in list(groups.items()):
                    if debug==d and mode==m:
                        for i,vs in rs.items():
                            for v,value in vs.items():z.setdefault(i,{}).setdefault(v,0);z[i][v]+=value
                groups[('all',d,m)]=z
        for key,rs in groups.items():out[label]['/'.join(key)]=stats([(vs['baseline'],vs['candidate']) for i,vs in sorted(rs.items())])
        if label=='resident':
            for a in ['lua','quickjs','sqlite','all']:
                for d in ['normal','debug']:
                    z={}
                    for x in data:
                        if x['debug']==d and (a=='all' or x['app']==a):
                            z.setdefault(x['round'],{}).setdefault(x['variant'],0);z[x['round']][x['variant']]+=sum(y['module_drop_ms'] for y in x['rows'])
                    out[label][f'{a}/{d}/module_drop']=stats([(vs['baseline'],vs['candidate']) for _,vs in sorted(z.items())])
    alloc=load('allocations');attr=load('attribution')
    for a in ['lua','quickjs','sqlite']:
        for d in ['normal','debug']:
            def rows(ds,v,repeat):return next(x['rows'] for x in ds if x['app']==a and x['debug']==d and x['variant']==v and x['repeat']==repeat)
            ab={v:rows(alloc,v,0) for v in ['baseline','candidate']};at={v:rows(attr,v,0) for v in ab}
            for v in ab:
                assert ab[v]==rows(alloc,v,1);assert at[v]==rows(attr,v,1)
            reconciled=[]
            for b,c,tb,tc in zip(ab['baseline'],ab['candidate'],at['baseline'],at['candidate']):
                assert b['path']==c['path']==tb['path']==tc['path']
                delta={k:b[k]-c[k] for k in ['alloc','realloc','requested_bytes','dealloc']}
                pred={k:tb['clone_'+k]-tc['clone_'+k] for k in ['alloc','realloc','requested_bytes']}
                assert all(delta[k]==pred[k] for k in pred),(b['path'],delta,pred)
                reconciled.append({'path':b['path'],'public_reduction':delta,'clone_reduction':pred})
            out['allocation_reconciliation'][a+'/'+d]={'baseline':{k:sum(x[k] for x in ab['baseline']) for k in ['alloc','realloc','requested_bytes']},'candidate':{k:sum(x[k] for x in ab['candidate']) for k in ['alloc','realloc','requested_bytes']},'source_baseline':{k:sum(x[k] for x in at['baseline']) for k in at['baseline'][0] if k!='path'},'source_candidate':{k:sum(x[k] for x in at['candidate']) for k in at['candidate'][0] if k!='path'},'per_tu':reconciled}
    save('summary',out)
    save('measurement-host-after',run(['sh','-c','date -u; uptime; ps -eo pid,ppid,pcpu,etime,args --sort=-pcpu | head -25']))

if __name__=='__main__':
    try:
        for mode in sys.argv[1:]:globals()[mode]()
    except BaseException as e:
        (E/'failure.txt').write_text(repr(e));raise
