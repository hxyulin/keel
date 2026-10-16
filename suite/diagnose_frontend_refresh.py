"""Untimed diagnostic source generation and serial collection for the refresh.
Run only in a disposable copy of the exact measured revision, after timing.
The initializer clocks are diagnostic attribution estimates, not benchmarks.
"""
import json,pathlib,shutil,subprocess,sys
from profile_frontend_refresh import ROOT,inputs,rawinputs,flags,run,store,sha,PROD,snapshot

def setup():
    src=ROOT/'source'; dst=ROOT/'diagnostic-source'
    shutil.copytree(src,dst)
    p=dst/'crates/keel-c/src/init.rs';t=p.read_text()
    old='            let one: Vec<InitItem<\'a>> = items[first..].to_vec();'
    new='''            let clone_items = items.len() - first;
            let clone_start = std::time::Instant::now();
'''+old
    assert t.count(old)==1;t=t.replace(old,new)
    old='''            next += 1;
            count = count.max(next as u64);'''
    new='''            let clone_ns = clone_start.elapsed().as_nanos();
            INIT_CLONE_STATS.with(|s| { let mut s=s.borrow_mut();
                s.calls += 1; s.items += clone_items as u64;
                s.range_extra += clone_items as u64 * n.saturating_sub(1) as u64;
                s.ns += clone_ns;
            });
'''+old
    assert t.count(old)==1;t=t.replace(old,new)
    t+='''
#[derive(Default)]
struct InitCloneStats { calls:u64, items:u64, range_extra:u64, ns:u128 }
impl Drop for InitCloneStats { fn drop(&mut self) {
 eprintln!("INIT_CLONE calls={} items={} range_extra={} inclusive_ns={} expr_size={} init_item_size={} ty_size={}",
 self.calls,self.items,self.range_extra,self.ns,std::mem::size_of::<crate::ast::Expr>(),std::mem::size_of::<crate::ast::InitItem>(),std::mem::size_of::<crate::types::Ty>());
}}
thread_local! { static INIT_CLONE_STATS:std::cell::RefCell<InitCloneStats>=Default::default(); }
'''
    p.write_text(t)
    # Histogram covers exact requested allocation size buckets; it cannot infer
    # allocating Rust types. This binary is never used for timings or RSS.
    p=dst/'crates/keel-c/examples/profile_allocations.rs';t=p.read_text()
    t=t.replace('    enabled: bool,','    histogram: [u64; 32],\n    enabled: bool,')
    t=t.replace('        enabled: false,','        histogram: [0;32],\n        enabled: false,')
    t=t.replace('                c.alloc += 1;','                c.histogram[(usize::BITS-layout.size().leading_zeros()) as usize] += 1;\n                c.alloc += 1;')
    t=t.replace('                c.realloc += 1;','                c.histogram[(usize::BITS-size.leading_zeros()) as usize] += 1;\n                c.realloc += 1;')
    t=t.replace('        let functions = module.functions.len();','        eprintln!("ALLOC_HIST path={path:?} counts={:?}", counts.histogram);\n        let functions = module.functions.len();')
    p.write_text(t)
    # Whole CLI clocks retain full phases and all verification. Main, optimizer,
    # frontend and backend semantics remain identical. Output byte checks follow.
    p=dst/'crates/keel/src/main.rs';t=p.read_text()
    def repl(a,b):
        nonlocal t
        assert t.count(a)==1,(a,t.count(a));t=t.replace(a,b)
    repl('            let preprocessed = if opts.preprocess_only', '            let phase_start = std::time::Instant::now();\n            let preprocessed = if opts.preprocess_only')
    repl('            if opts.preprocess_only {\n                drop(src);','            eprintln!("PHASE preprocess_ns {}",phase_start.elapsed().as_nanos());\n            if opts.preprocess_only {\n                drop(src);')
    repl('            let mut module = {\n                profiling::scope!("c-frontend");','            let phase_start = std::time::Instant::now();\n            let mut module = {\n                profiling::scope!("c-frontend");')
    repl('            drop(src);\n            if let Err(errors)', '            eprintln!("PHASE frontend_ns {}",phase_start.elapsed().as_nanos());\n            drop(src);\n            let phase_start=std::time::Instant::now();\n            if let Err(errors)')
    # The exact cc branch occurs after the frontend compile, other commands keep
    # their original paths. Insert markers only in the cc substring.
    at=t.index('            eprintln!("PHASE frontend_ns')
    head=t[:at];tail=t[at:]
    old='''            opts.optimize(&mut module)
                .map_err(|e| format!("{input}: {e}"))?;'''
    assert tail.count(old)>=1
    tail=tail.replace(old,'''            eprintln!("PHASE verify_ns {}",phase_start.elapsed().as_nanos());
            let phase_start=std::time::Instant::now();
'''+old+'''
            eprintln!("PHASE optimize_ns {}",phase_start.elapsed().as_nanos());
''',1)
    old='''            emit_module(&opts, &module).map_err(|e| format!("{input}: {e}"))'''
    tail=tail.replace(old,'''            let phase_start=std::time::Instant::now();
            let result = emit_module(&opts, &module).map_err(|e| format!("{input}: {e}"));
            eprintln!("PHASE backend_output_ns {}",phase_start.elapsed().as_nanos());
            result''',1)
    t=head+tail;p.write_text(t)
    # Context only: optimizer verification is counted distinctly from pass
    # transforms, including call-contracts. No optimizer campaign or ablation.
    p=dst/'crates/keel-opt/src/lib.rs';t=p.read_text()
    old='        profiling::scope!("pass", pass.name());'
    assert t.count(old)==1
    t=t.replace(old,old+'\n        let _diagnostic_clock = DiagnosticPassClock::new(pass.name());')
    # Restrict wrappers to the pipeline function, preserving its error paths.
    start=t.index('pub fn run_pipeline_for_target(');end=t.index('/// Runs [`DEFAULT_PIPELINE`]',start)
    section=t[start:end].replace('verify(func)', '{ let s=std::time::Instant::now(); let v=verify(func); DIAGNOSTIC_VERIFY_NS.with(|n| n.set(n.get()+s.elapsed().as_nanos())); v }')
    t=t[:start]+section+t[end:]
    t+=r'''
thread_local! { static DIAGNOSTIC_VERIFY_NS:std::cell::Cell<u128>=const {std::cell::Cell::new(0)}; }
struct DiagnosticPassClock<'a> {name:&'a str,start:std::time::Instant,verify_before:u128}
impl<'a> DiagnosticPassClock<'a> {fn new(name:&'a str)->Self {
 Self{name,start:std::time::Instant::now(),verify_before:DIAGNOSTIC_VERIFY_NS.with(|n|n.get())}
}}
impl Drop for DiagnosticPassClock<'_> {fn drop(&mut self) {
 eprintln!("OPT_CONTEXT {} total_ns={} verify_ns={}",self.name,self.start.elapsed().as_nanos(),DIAGNOSTIC_VERIFY_NS.with(|n|n.get())-self.verify_before);
}}
'''
    p.write_text(t)
    store('diagnostic-source', {'files':{str(p.relative_to(dst)):sha(p) for p in dst.rglob('*.rs')},'generator_sha256':sha(__file__)})

def memory_setup():
    p=ROOT/'source/crates/keel-c/examples/resident_memory.rs'
    p.write_text(r'''
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args:Vec<_>=std::env::args().skip(1).collect();
 let opts=keel_c::Options{debug_info:args.iter().any(|x|x=="--debug"),..Default::default()};
 let inputs:Vec<_>=args.iter().filter(|x|x.as_str()!="--debug").map(|p|Ok((p,std::fs::read_to_string(p)?))).collect::<Result<_,std::io::Error>>()?;
 for (p,src) in &inputs {drop(std::hint::black_box(keel_c::compile(src,p,opts)?));}
 Ok(())
}
''')

def memory():
    b=ROOT/'target-memory/release/examples/resident_memory';r={'start':snapshot(),'binary_sha256':sha(b),'runs':[]}
    wrapper='import resource,subprocess,sys; p=subprocess.run(sys.argv[1:]); print("RSS_KIB",resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,file=sys.stderr);sys.exit(p.returncode)'
    for i in range(3):
        for app,ps in inputs().items():
            for d in [False,True]:
                r['runs'].append(dict(app=app,debug=d,repeat=i,**run(['python3','-c',wrapper,b,*ps,*(['--debug'] if d else [])])))
    r['end']=snapshot();store('memory',r)

def collect():
    r={'start':snapshot(),'initializer':[],'allocations_histogram':[],'cli_phases':[],'parity':[]}
    db=ROOT/'target-diagnostic/release'
    for app,ps in inputs().items():
        for debug in [False,True]:
            suf=['--debug'] if debug else []
            r['initializer'].append(dict(app=app,debug=debug,**run([db/'examples/profile','baseline','1',*ps,*suf])))
            dest=ROOT/'evidence/ir'/f'{app}-{debug}'/'diagnostic'
            x=run([db/'examples/profile','prepare','1',dest,*ps,*suf])
            baseline=ROOT/'evidence/ir'/f'{app}-{debug}'/'baseline'
            assert {p.name:sha(p) for p in dest.glob('*.ir')}=={p.name:sha(p) for p in baseline.glob('*.ir')}
            r['parity'].append(dict(app=app,debug=debug,**x))
            r['allocations_histogram'].append(dict(app=app,debug=debug,**run([db/'examples/profile_allocations',*ps,*suf])))
    # Diagnostic phase timings are contextual; 11 rounds alternate program order.
    # Includes initializer clocks, so do not treat as the production CLI baseline.
    raws=rawinputs()
    for i in range(11):
        for app in (list(raws) if i%2==0 else list(reversed(raws))):
            for p in raws[app]:
                output=ROOT/'evidence/context.s'
                cmd=[db/'keel','cc',p,*flags(app),'-O','-o',output]
                x=run(cmd)
                if i==0:
                    original=ROOT/'evidence/original.s'
                    run([PROD,'cc',p,*flags(app),'-O','-o',original])
                    assert sha(output)==sha(original),p
                r['cli_phases'].append(dict(app=app,round=i,path=str(p),**x))
        store('diagnostics',r);print('diagnostic round',i+1,flush=True)
    r['end']=snapshot();r['binaries']={str(p):sha(p) for p in [db/'keel',db/'examples/profile',db/'examples/profile_allocations']};store('diagnostics',r)

if __name__=='__main__':globals()[sys.argv[1]]()
