#!/usr/bin/env python3
"""Finite comparison: run ONLY under suite/fast_opt_locked.py. No Rust build.
Usage: run.py KEEL /tmp/NEW_DIRECTORY [--static-only]
Set CLANG/WASM_LD/NODE to existing executables to override discovery.
"""
import hashlib,json,os,pathlib,random,shutil,statistics,subprocess,sys,time
from decode import decode,stripped
HERE=pathlib.Path(__file__).resolve().parent
NAMES=['scalar','wide','integer_loop','branch_switch','checksum','pointer_mutate','local_array','recursion']
EXPORTS=NAMES+['object_offset','buffer_offset']

def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def call(args):return subprocess.run(list(map(str,args)),capture_output=True,text=True)
def quiet():
    cpus=os.cpu_count() or 1;load=os.getloadavg()
    probe=call(['ps','-axo','pid,pcpu,comm']);procs=probe.stdout
    busy=[]
    for line in procs.splitlines()[1:]:
        fields=line.strip().split(None,2)
        if len(fields)==3 and float(fields[1])>=50:busy.append(line.strip())
    return dict(load=list(load),cpus=cpus,busy=busy,ps_returncode=probe.returncode,ok=probe.returncode==0 and bool(procs.strip()) and load[0]<=max(1,cpus/2) and not busy)
def main():
    binary=pathlib.Path(sys.argv[1]).resolve();out=pathlib.Path(sys.argv[2]);assert out.is_absolute() and str(out).startswith(('/tmp/','/private/tmp/'))
    out.mkdir(exist_ok=False);shutil.copy2(binary,out/'keel');binary=out/'keel'
    source=out/'corpus.c';shutil.copy2(HERE/'corpus.c',source)
    clang=pathlib.Path(shutil.which(os.environ.get('CLANG','clang'))).resolve()
    node=pathlib.Path(call([os.environ.get('NODE','node'),'-p','process.execPath']).stdout.strip())
    linker=os.environ.get('WASM_LD') or shutil.which('wasm-ld')
    if not linker:
        root=call(['rustc','--print','sysroot']).stdout.strip()
        matches=list(pathlib.Path(root).glob('lib/rustlib/*/bin/gcc-ld/wasm-ld'))
        if matches:linker=str(matches[0])
    report=dict(base=call(['git','rev-parse','HEAD']).stdout.strip(),source_sha256=sha(source),exports=EXPORTS,tools={},profiles={},commands=[],excluded_unsupported=0,preflight=quiet())
    def save():
        (out/'manifest.json').write_text(json.dumps(report,indent=2)+'\n')
    for name,path,args in [('keel',binary,['--version']),('clang',clang,['--version']),('node',node,['--version'])]+([('wasm_ld',pathlib.Path(linker).resolve(),['--version'])] if linker else []):
        v=call([path]+args)
        report['tools'][name]=dict(path=str(path),sha256=sha(path),version=v.stdout.strip(),version_returncode=v.returncode,version_stderr=v.stderr.strip())
    flags=['--target=wasm32-unknown-unknown','-mcpu=mvp','-std=c11','-ffreestanding','-nostdlib','-nostdinc','-fno-builtin','-fno-lto','-fno-fast-math','-I',str(HERE.parent.parent/'crates/keel-c/include')]
    commands={}
    for mode in ('plain','basic'):
        commands[mode]=[binary,'cc','--target','wasm32-unknown-unknown','--emit-wasm','--wasm-memory-pages','1','--wasm-data-floor','1024','--wasm-stack-bottom','32768','--wasm-stack-top','65536']
        for name in EXPORTS:commands[mode]+=['--export',name]
        if mode=='basic':commands[mode]+=['--wasm-opt','basic']
        commands[mode]+=[source,'-o',out/f'{mode}.raw.wasm']
    for mode,opt in [('clang-O0','-O0'),('clang-O2','-O2')]:
        assembly=[clang]+flags+[opt,'-S',source,'-o',out/f'{mode}.s']
        obj=[clang]+flags+[opt,'-c',source,'-o',out/f'{mode}.o']
        for args in (assembly,obj):
            r=call(args);report['commands'].append(dict(argv=list(map(str,args)),returncode=r.returncode,stderr=r.stderr))
            if r.returncode:
                report['failure']=dict(stage='diagnostic emission',profile=mode,diagnostic=r.stderr);save();return 1
        args=[clang]+flags+[opt]
        if linker:args+=['-fuse-ld='+str(linker)]
        args+=['-Wl,--no-entry','-Wl,--export-memory','-Wl,--initial-memory=65536','-Wl,--max-memory=65536','-Wl,--stack-first','-Wl,--global-base=32768','-Wl,-z,stack-size=32768','-Wl,--strip-all']
        args+=['-Wl,--export='+n for n in EXPORTS]+[source,'-o',out/f'{mode}.raw.wasm']
        commands[mode]=args
    for mode,args in commands.items():
        r=call(args);report['commands'].append(dict(argv=list(map(str,args)),returncode=r.returncode,stderr=r.stderr))
        if r.returncode:
            report['profiles'][mode]=dict(emission='failed',diagnostic=r.stderr);continue
        raw=out/f'{mode}.raw.wasm';dest=out/f'{mode}.wasm'
        try:
            dest.write_bytes(stripped(raw.read_bytes()));decoded=decode(dest.read_bytes())
        except (AssertionError,ValueError,IndexError) as error:
            report['profiles'][mode]=dict(emission='passed',static_policy='failed',diagnostic=str(error))
            report['timing']='not attempted: static policy failed';save();return 1
        (out/f'{mode}.decoded.json').write_text(json.dumps(decoded,indent=2)+'\n')
        report['profiles'][mode]=dict(emission='passed',raw_sha256=sha(raw),sha256=sha(dest),raw_bytes=raw.stat().st_size,**decoded)
    report['suite_sha256']={p.name:sha(p) for p in sorted(HERE.iterdir()) if p.is_file()}
    report['ranking']=sorted([dict(profile=m,function=f.get('export') or f['index'],body_bytes=f['body_bytes'],locals=f['locals'],instructions=f['instructions']) for m,p in report['profiles'].items() for f in p.get('functions',[])],key=lambda x:-x['body_bytes'])
    save();print('STATIC_READY '+str(out/'manifest.json'),flush=True)
    args=[node,HERE/'harness.cjs',out,'--correctness-only']
    r=call(args);(out/'correctness.log').write_text(r.stdout+r.stderr)
    if r.returncode:
        report['correctness']=dict(status='failed',returncode=r.returncode,diagnostic=r.stderr)
        report['timing']='not attempted: correctness failed';save();return 1
    report['correctness']=json.loads(r.stdout);save()
    if '--static-only' in sys.argv:
        report['timing']='not requested: static-only';save();return
    # Exactly one bounded attempt. Quiet failure defers all timing, never correctness.
    report['timing_gate']=quiet()
    if not report['preflight']['ok'] or not report['timing_gate']['ok']:
        report['timing']='deferred: quiet preflight/gate failed';save();return
    rng=random.Random(0x5eed);compile_samples={m:[] for m in commands if report['profiles'][m]['emission']=='passed'}
    report['compile_rounds']=[]
    report['compile_samples_ms']=compile_samples
    for round_ in range(7):
        gate=quiet();report['compile_rounds'].append(gate)
        if not gate['ok']:report['timing']='deferred: per-round quiet gate failed';save();return
        order=list(compile_samples);rng.shuffle(order)
        report['compile_rounds'][-1]['order']=order
        for mode in order:
            start=time.perf_counter_ns();r=call(commands[mode]);elapsed=(time.perf_counter_ns()-start)/1e6
            if r.returncode or sha(out/f'{mode}.raw.wasm')!=report['profiles'][mode]['raw_sha256']:
                report['failure']=dict(stage='timed emission',profile=mode,returncode=r.returncode,diagnostic=r.stderr or 'non-deterministic output')
                report['timing']='failed: timed emission';save();return 1
            compile_samples[mode].append(elapsed)
    report['compile_ms']={m:dict(raw=v,median=statistics.median(v),range=[min(v),max(v)]) for m,v in compile_samples.items()}
    report['compile_ratios']={m+'/'+ref:dict(median_ratio=statistics.median(v)/statistics.median(compile_samples[ref]),paired_raw=[a/b for a,b in zip(v,compile_samples[ref])]) for m,v in compile_samples.items() for ref in ('clang-O0','clang-O2') if ref in compile_samples}
    r=call([node,HERE/'harness.cjs',out]);(out/'runtime.log').write_text(r.stdout+r.stderr)
    if r.returncode:
        report['runtime']=dict(status='failed',returncode=r.returncode,diagnostic=r.stderr)
        report['timing']='runtime failed';save();return 1
    report['runtime']=json.loads(r.stdout);report['timing']='completed' if not report['runtime'].get('deferred') else 'runtime deferred';save()
if __name__=='__main__':sys.exit(main())
