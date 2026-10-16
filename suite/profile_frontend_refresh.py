"""Exact-source, serial frontend refresh. Remote root is private to this task.
Run setup before builds; modes are separate to prevent diagnostic overlap.
No application executables are run. Compiler sources are patched only for
explicit diagnostic builds by setup-diagnostics, never for measured latency.
"""
import argparse, collections, datetime, hashlib, json, os, pathlib, random
import resource, signal, statistics, subprocess, time

ROOT = pathlib.Path('/tmp/keel-frontend-refresh-9eb96be')
BASE = '9eb96be6d6e9d87e4f860c27947694b227bdb857'
PROD = pathlib.Path('/tmp/keel-bench-target-6e6d2c2/release/keel')
PERF = pathlib.Path('/tmp/keel-current-app-profile/tools/usr/bin/perf')
ENV = dict(os.environ, LD_LIBRARY_PATH='/tmp/keel-current-app-profile/tools/usr/lib')

def sha(p): return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def run(cmd, **kw):
    p = subprocess.run(list(map(str,cmd)),text=True,capture_output=True,**kw)
    r = dict(command=list(map(str,cmd)),stdout=p.stdout,stderr=p.stderr,returncode=p.returncode)
    if p.returncode: raise RuntimeError(r)
    return r

def snapshot():
    return {k:run(v) for k,v in dict(time=['date','-u'],load=['uptime'],processes=['ps','-eo','pid,ppid,pcpu,etime,args','--sort=-pcpu'],cpu=['lscpu'],affinity=['taskset','-pc',str(os.getpid())],governor=['cat','/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'],vmstat=['vmstat','1','2']).items()}

def inputs():
    v={a:sorted((ROOT/'inputs'/a/'pre').glob('*.i')) for a in ['lua','quickjs','sqlite']}
    assert list(map(len,v.values()))==[33,9,2]
    return v

def rawinputs():
    r=ROOT/'raw'
    return {'lua': sorted(p for p in (r/'lua/lua-5.4.7/src').glob('*.c') if p.name!='luac.c'),
    'quickjs':[r/'quickjs/quickjs-2024-01-13'/n for n in ['qjs.c','quickjs.c','libregexp.c','libunicode.c','cutils.c','quickjs-libc.c','libbf.c']]+[r/'quickjs/gen'/n for n in ['repl.c','qjscalc.c']],
    'sqlite':[r/'sqlite/sqlite-amalgamation-3460100'/n for n in ['shell.c','sqlite3.c']]}

def flags(app):
    return {'lua':['-DLUA_USE_LINUX=1'],'quickjs':['-D_GNU_SOURCE=1','-DCONFIG_VERSION="2024-01-13"','-DCONFIG_BIGNUM=1','-I',str(ROOT/'raw/quickjs/quickjs-2024-01-13')],'sqlite':['-DSQLITE_THREADSAFE=1']}[app]

def setup():
    # Feature-free driver calls the unchanged public compile API. Its Debug IR
    # snapshots are compared with the staged helper (including debug metadata).
    p=ROOT/'source/crates/keel-c/examples'
    t=(p/'profile.rs').read_text()
    t=t.replace('let (staged, _) = keel_c::profile::compile(src, path, opts)?;', 'let staged = keel_c::compile(src, path, opts)?;')
    t=t.replace('let (module, times) = keel_c::profile::compile(black_box(src), path, opts)?;', 'let module = keel_c::compile(black_box(src), path, opts)?;\n                let times: () = ();')
    start=t.index('            if let Some(t) = stages {')
    end=t.index('            println!("}}");',start)
    t=t[:start]+'            let _ = stages;\n'+t[end:]
    (p/'resident.rs').write_text(t)
    # Host-correct preprocessing helper; source feature changes only output of
    # stats mode. Linux source definitions match suite manifests.
    t=(p/'profile_preprocess.rs').read_text().replace('LUA_USE_MACOSX','LUA_USE_LINUX')
    (p/'preprocess_linux.rs').write_text(t)

def store(name, data):
    (ROOT/'evidence').mkdir(exist_ok=True)
    (ROOT/'evidence'/f'{name}.json').write_text(json.dumps(data,indent=2)+'\n')

def resident():
    rng=random.Random(20261016); paths=inputs()
    bins={'baseline':ROOT/'target-plain/release/examples/resident','staged':ROOT/'target-stage/release/examples/profile'}
    r={'source_revision':BASE,'start':snapshot(),'seed':20261016,'rounds':13,'samples':[], 'validation':[], 'binaries':{m:{'path':str(p),'sha256':sha(p)} for m,p in bins.items()},'inputs':{a:[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in ps] for a,ps in paths.items()}}
    for app,ps in paths.items():
        for debug in [False,True]:
            manifests=[]
            for mode,b in list(bins.items())+[('fp',ROOT/'target-fp/release/examples/profile')]:
                dest=ROOT/'evidence/ir'/f'{app}-{debug}'/mode
                result=run([b,'prepare','1',dest,*ps,*(['--debug'] if debug else [])])
                manifest={p.name:sha(p) for p in dest.glob('*.ir')}
                assert len(manifest)==len(ps);manifests.append(manifest)
                r['validation'].append(dict(app=app,debug=debug,mode=mode,ir_sha256=manifest,**result))
            assert manifests[0]==manifests[1]==manifests[2]
    # The validation above is warmup. Each timing process additionally warms
    # every TU and checks public/staged equality outside its measured boundary.
    for i in range(13):
        jobs=[(a,d) for a in paths for d in [False,True]];rng.shuffle(jobs)
        for app,d in jobs:
            ps=paths[app];ps=ps[i%len(ps):]+ps[:i%len(ps)]
            for mode in (['baseline','staged'] if i%2==0 else ['staged','baseline']):
                x=run([bins[mode],mode,'1',*ps,*(['--debug'] if d else [])])
                r['samples'].append(dict(round=i,app=app,debug=d,mode=mode,rows=[json.loads(s) for s in x['stdout'].splitlines()],**x))
        store('resident',r);print('resident round',i+1,flush=True)
    r['end']=snapshot();store('resident',r)

def cli():
    assert sha(PROD)=='eaf11fd8bbff2c3840762209c1ca423a572fd88acd7d942e0d736ca115c81659'
    rng=random.Random(20261016);raw=rawinputs();pre=inputs()
    r={'binary':str(PROD),'sha256':sha(PROD),'start':snapshot(),'samples':[],'warmups':[],'first_invocations':[],'raw_inputs':{a:[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in ps] for a,ps in raw.items()}}
    def commands(app,mode,i):
        ps=pre[app] if mode=='pre-ir' else raw[app];ps=ps[i%len(ps):]+ps[:i%len(ps)]
        return [[PROD,'cc',p,*([] if mode=='pre-ir' else flags(app)),*(['-O'] if mode=='raw-fullopt' else ['--emit-ir']),'-o','/dev/null'] for p in ps]
    def one(app,mode,i):
        rows=[]
        for cmd in commands(app,mode,i):
            t=time.perf_counter();x=run(cmd);elapsed=time.perf_counter()-t
            rows.append(dict(wall_ms=elapsed*1000,**x))
        return dict(app=app,mode=mode,round=i,rows=rows,total_ms=sum(x['wall_ms'] for x in rows))
    modes=['pre-ir','raw-ir','raw-fullopt']
    # Cold process/CLI on warm filesystem caches; never drop system caches.
    for app in raw:
        for mode in modes:r['first_invocations'].append(one(app,mode,0))
    for app in raw:
        for mode in modes:r['warmups'].append(one(app,mode,0))
    store('cli',r)
    for i in range(11):
        jobs=[(a,m) for a in raw for m in modes];rng.shuffle(jobs)
        for app,mode in jobs:r['samples'].append(one(app,mode,i))
        store('cli',r);print('cli round',i+1,flush=True)
    r['end']=snapshot();store('cli',r)

def preprocess():
    b=ROOT/'target-stage/release/examples/preprocess_linux';rng=random.Random(20261016)
    r={'start':snapshot(),'binary_sha256':sha(b),'samples':[],'stats':{}}
    ps=rawinputs()
    for i in range(13):
        apps=list(ps);rng.shuffle(apps)
        for app in apps:
            # / is the existing Linux system root. Header reads are inside the
            # phase; raw top-level file reads, output drop and startup excluded.
            files=ps[app];files=files[i%len(files):]+files[:i%len(files)]
            x=run([b,'time','1',app,'/',ROOT/'native-pre'/app,*files])
            r['samples'].append(dict(app=app,round=i,rows=[json.loads(s) for s in x['stdout'].splitlines()],**x))
        store('preprocess',r)
    for app,files in ps.items():
        r['stats'][app]=run([b,'stats','1',app,'/',ROOT/'native-pre'/app,*files])
        r.setdefault('preparation',{})[app]=run([b,'prepare','1',app,'/',ROOT/'native-pre'/app,*files])
    r['end']=snapshot();store('preprocess',r)

def allocations():
    b=ROOT/'target-stage/release/examples/profile_allocations'
    r={'start':snapshot(),'binary_sha256':sha(b),'runs':[]}
    for app,ps in inputs().items():
        for d in [False,True]:
            # Fresh child wrapper gives per-process RSS, with known warmup and
            # resident input overhead. Counts exclude returned-module drop.
            wrapper='import resource,subprocess,sys; p=subprocess.run(sys.argv[1:]); print("RSS_KIB",resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,file=sys.stderr);sys.exit(p.returncode)'
            r['runs'].append(dict(app=app,debug=d,**run(['python3','-c',wrapper,b,*ps,*(['--debug'] if d else [])])))
    r['end']=snapshot();store('allocations',r)

def sample():
    b=ROOT/'target-fp/release/examples/profile';r={'start':snapshot(),'binary_sha256':sha(b),'profiles':[]}
    # Existing perf permissions only. Task-scoped software event, attach after
    # validation and warmup. Two independent 8-second captures per workload.
    for app,ps in inputs().items():
        for capture in range(2):
            log=ROOT/'evidence'/f'{app}-{capture}.helper.stderr'
            with log.open('w+') as err:
                proc=subprocess.Popen([str(b),'sample','100000',*map(str,ps)],stdout=subprocess.DEVNULL,stderr=err)
                try:
                    deadline=time.monotonic()+30
                    while 'ready:' not in log.read_text():
                        if proc.poll() is not None or time.monotonic()>deadline:raise RuntimeError(log.read_text())
                        time.sleep(.05)
                    data=ROOT/'evidence'/f'{app}-{capture}.perf.data'
                    record=run([PERF,'record','-e','cpu-clock:u','-F','499','-g','--call-graph','fp','-p',str(proc.pid),'-o',data,'--','sleep','8'],env=ENV)
                    decoded=run([PERF,'script','--no-inline','-i',data,'-F','comm,pid,tid,time,event,ip,sym,dso,period'],env=ENV)
                    (data.with_suffix('.script.txt')).write_text(decoded['stdout'])
                    r['profiles'].append(dict(app=app,capture=capture,record=record,decode_stderr=decoded['stderr'],decode_command=decoded['command'],data_sha256=sha(data)))
                finally:
                    proc.terminate();proc.wait(timeout=15)
            store('samples',r)
    r['end']=snapshot();store('samples',r)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('mode',choices=['setup','resident','cli','preprocess','allocations','sample']);a=p.parse_args()
    globals()[a.mode]()
