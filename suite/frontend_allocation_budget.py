"""Untimed copied-source allocation census. Never invokes timing or profiling.

Generate from the exact committed compiler; build/run commands are separate.
The diagnostic uses glibc backtrace + DWARF inline attribution, not size guesses.
"""
import argparse
import hashlib
import io
import json
import pathlib
import subprocess
import tarfile
import ast
import gzip
import collections
import os

BASE = 'e85816ff651e912580bb8bd800c6c4884047a73d'
ROOT = pathlib.Path(__file__).resolve().parents[1]

DRIVER = r'''
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
const DEPTH: usize = 16;
const NS: usize = 262144;
const NP: usize = 2097152;
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Counts { alloc:u64, realloc:u64, requested:u64, dealloc:u64, retired:u64, freed:u64 }
#[derive(Clone, Copy)]
struct Stack { pcs:[usize;DEPTH], phase:usize, c:Counts, live:u64, returned:u64, free_calls:[u64;5], free_bytes:[u64;5], retired_bytes:[u64;5] }
impl Default for Stack { fn default()->Self {Self{pcs:[0;DEPTH],phase:0,c:Counts::default(),live:0,returned:0,free_calls:[0;5],free_bytes:[0;5],retired_bytes:[0;5]}} }
#[derive(Clone, Copy, Default)]
struct Ptr { p:usize, stack:usize, size:usize }
struct State { stacks:Vec<Stack>, ptrs:Vec<Ptr>, total:[Counts;5], capture:bool, enabled:bool, missing:u64, truncated:u64, live:u64, peaks:[u64;5] }
thread_local! { static STATE:Cell<*mut State>=const{Cell::new(std::ptr::null_mut())}; static BUSY:Cell<bool>=const{Cell::new(false)}; static NESTED:Cell<u64>=const{Cell::new(0)}; }
unsafe extern "C" { fn backtrace(buffer:*mut *mut std::ffi::c_void, size:i32)->i32; }
fn phase()->usize { @PHASE@ }
fn hash(pcs:&[usize;DEPTH], phase:usize)->usize { pcs.iter().fold(phase.wrapping_add(1),|h,p|h.wrapping_mul(1099511628211)^p) }
impl State {
 fn stack(&mut self, phase:usize)->usize {
  let mut pcs=[0usize;DEPTH];
  let n=unsafe{backtrace(pcs.as_mut_ptr().cast(),DEPTH as i32)};
  if n as usize==DEPTH {self.truncated+=1;}
  for p in &mut pcs {if *p>0x10000000 {*p=1;}}
  let mut i=hash(&pcs,phase)&(NS-1);
  for _ in 0..NS {let s=&mut self.stacks[i]; if s.pcs[0]==0 {s.pcs=pcs;s.phase=phase;return i;} if s.phase==phase && s.pcs==pcs {return i;} i=(i+1)&(NS-1);}
  std::process::abort();
 }
 fn slot(&self,p:usize,insert:bool)->usize {
  let mut i=(p>>4).wrapping_mul(11400714819323198485)&(NP-1);
  let mut tomb=None;
  for _ in 0..NP {let v=self.ptrs[i].p;if v==p{return i;} if v==1 && tomb.is_none(){tomb=Some(i);} if v==0 {return if insert {tomb.unwrap_or(i)} else {i};} i=(i+1)&(NP-1);}
  std::process::abort();
 }
 fn retire(&mut self,p:usize,size:usize,dealloc:bool,phase:usize) {
  self.live-=size as u64;
  self.total[phase].retired+=size as u64; if dealloc {self.total[phase].dealloc+=1;self.total[phase].freed+=size as u64;}
  if self.capture {
   let i=self.slot(p,false);let old=self.ptrs[i];
   if old.p!=p {self.missing+=1;return;}
   if old.size!=size {std::process::abort();}
   let s=&mut self.stacks[old.stack]; s.live-=size as u64;s.c.retired+=size as u64;s.retired_bytes[phase]+=size as u64;if dealloc{s.c.dealloc+=1;s.c.freed+=size as u64;s.free_calls[phase]+=1;s.free_bytes[phase]+=size as u64;}
   self.ptrs[i].p=1;
  }
 }
 fn add(&mut self,p:usize,size:usize,realloc:bool,phase:usize) {
  self.live+=size as u64;self.peaks[phase]=self.peaks[phase].max(self.live);
  let c=&mut self.total[phase];if realloc{c.realloc+=1;}else{c.alloc+=1;}c.requested+=size as u64;
  if self.capture {
   let sid=self.stack(phase);let s=&mut self.stacks[sid]; if realloc{s.c.realloc+=1;}else{s.c.alloc+=1;}s.c.requested+=size as u64;s.live+=size as u64;
   let i=self.slot(p,true);if self.ptrs[i].p==p{std::process::abort();} self.ptrs[i]=Ptr{p,stack:sid,size};
  }
 }
}
fn record(f:impl FnOnce(&mut State,usize)) {
 BUSY.with(|busy|{if busy.get(){NESTED.with(|c|c.set(c.get()+1));return;} STATE.with(|cell|{let p=cell.get();if !p.is_null(){unsafe{if (*p).enabled{busy.set(true);f(&mut *p,phase());busy.set(false);}}}});});
}
struct Alloc;
// SAFETY: forwards pointers/layouts unchanged to System; metadata is separately
// allocated before capture and never resized. Single-threaded driver uses TLS.
unsafe impl GlobalAlloc for Alloc {
 unsafe fn alloc(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc(l)};if !p.is_null(){record(|s,ph|s.add(p as usize,l.size(),false,ph));}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};if !p.is_null(){record(|s,ph|s.add(p as usize,l.size(),false,ph));}p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){record(|s,ph|s.retire(p as usize,l.size(),true,ph));unsafe{System.dealloc(p,l)};}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {let q=unsafe{System.realloc(p,l,n)};if !q.is_null(){record(|s,ph|{s.retire(p as usize,l.size(),false,ph);s.add(q as usize,n,true,ph);});}q}
}
#[global_allocator] static ALLOC:Alloc=Alloc;
fn main()->Result<(),Box<dyn std::error::Error>> {
 let a:Vec<String>=std::env::args().collect();
 if a.len()!=7{return Err("TARGET DEBUG INPUT OUTPUT COUNTS CAPTURE".into());}
 let target=match a[1].as_str(){"x86_64"=>keel_ir::Target::X86_64LinuxGnu,"aarch64"=>keel_ir::Target::Aarch64AppleDarwin,_=>return Err("target".into())};
 let opts=keel_c::Options{target,debug_info:a[2].parse()?}; let src=std::fs::read_to_string(&a[3])?;
 let capture:bool=a[6].parse()?;
 let mut state=Box::new(State{stacks:if capture{vec![Stack::default();NS]}else{Vec::new()},ptrs:if capture{vec![Ptr::default();NP]}else{Vec::new()},total:[Counts::default();5],capture,enabled:false,missing:0,truncated:0,live:0,peaks:[0;5]});
 let mut warm=[std::ptr::null_mut();DEPTH];unsafe{backtrace(warm.as_mut_ptr(),DEPTH as i32);}
 STATE.with(|p|p.set(&mut *state));state.enabled=true;
 let result=keel_c::compile(&src,&a[3],opts);
 state.enabled=false;
 let public=state.total;
 for s in &mut state.stacks {s.returned=s.live;}
 match &result {Ok(m)=>{keel_ir::verify_module(m).map_err(|e|format!("{e:?}"))?;std::fs::write(&a[4],format!("{m:?}"))?;},Err(d)=>std::fs::write(&a[4],format!("{d:?}"))?}
 @DROP_PHASE@
 state.enabled=true;drop(result);state.enabled=false;STATE.with(|p|p.set(std::ptr::null_mut()));
 use std::io::Write;
 let mut out=std::io::BufWriter::new(std::fs::File::create(&a[5])?);
 for (i,c) in public.iter().enumerate(){writeln!(out,"B\t{i}\t{}\t{}\t{}\t{}\t{}\t{}",c.alloc,c.realloc,c.requested,c.dealloc,c.retired,c.freed)?;}
 writeln!(out,"P\t{:?}\t{}",state.peaks,state.live)?;
 for (i,c) in state.total.iter().enumerate(){writeln!(out,"T\t{i}\t{}\t{}\t{}\t{}\t{}\t{}",c.alloc,c.realloc,c.requested,c.dealloc,c.retired,c.freed)?;}
 writeln!(out,"M\t{}\t{}",state.missing,state.truncated)?;
 writeln!(out,"H\t{}\t{}",state.stacks.len()*std::mem::size_of::<Stack>()+state.ptrs.len()*std::mem::size_of::<Ptr>(),NESTED.with(|c|c.get()))?;
 for (i,s) in state.stacks.iter().enumerate().filter(|(_,s)|s.pcs[0]!=0){let c=s.c;write!(out,"S\t{i}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",s.phase,c.alloc,c.realloc,c.requested,c.dealloc,c.retired,s.returned,s.live)?;for p in s.pcs.iter().take_while(|p|**p!=0){write!(out,"\t{p:x}")?;}writeln!(out)?;writeln!(out,"L\t{i}\t{:?}\t{:?}\t{:?}",s.free_calls,s.free_bytes,s.retired_bytes)?;}
 Ok(())
}
'''


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def generate(dest, diagnostic):
    dest = dest.resolve()
    repository = pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],cwd=ROOT,text=True).strip())
    if dest.exists() or repository == dest or repository in dest.parents:
        raise ValueError('destination must be NEW and outside repository')
    raw = subprocess.check_output(['git', 'archive', BASE, 'Cargo.toml', 'Cargo.lock', 'crates'], cwd=repository)
    dest.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
        tar.extractall(dest)
    before = {str(p.relative_to(dest)): digest(p) for p in dest.rglob('*') if p.is_file()}
    if diagnostic:
        p = dest/'crates/keel-c/src/lib.rs'
        t = p.read_text()
        def replace(old, new):
            nonlocal t
            assert t.count(old) == 1, old
            t = t.replace(old, new)
        replace('mod ast;', 'pub mod budget_phase;\nmod ast;')
        replace('    let arena = bumpalo::Bump::new();', '    budget_phase::set(0);\n    let arena = bumpalo::Bump::new();')
        replace('    let mut parser = parse::Parser::new', '    budget_phase::set(1);\n    let mut parser = parse::Parser::new')
        replace('    irgen::Gen::new(&parser, &files, opts.debug_info)', '    budget_phase::set(2);\n    let result = irgen::Gen::new(&parser, &files, opts.debug_info)')
        replace('        .map_err(|e| diagnostic(&files, e))\n}', '        .map_err(|e| diagnostic(&files, e));\n    budget_phase::set(3);\n    result\n}')
        p.write_text(t)
        (dest/'crates/keel-c/src/budget_phase.rs').write_text('use std::sync::atomic::{AtomicUsize,Ordering};\nstatic PHASE:AtomicUsize=AtomicUsize::new(0);\npub fn set(p:usize){PHASE.store(p,Ordering::Relaxed);}\npub fn get()->usize{PHASE.load(Ordering::Relaxed)}\n')
    driver = DRIVER.replace('@PHASE@', 'keel_c::budget_phase::get()' if diagnostic else '0')
    driver = driver.replace('@DROP_PHASE@', 'keel_c::budget_phase::set(4);' if diagnostic else '')
    (dest/'crates/keel-c/examples/allocation_budget_census.rs').write_text(driver)
    after = {str(p.relative_to(dest)): digest(p) for p in dest.rglob('*') if p.is_file()}
    manifest = {'base':BASE, 'diagnostic':diagnostic, 'generator_sha256':digest(pathlib.Path(__file__)), 'archive_sha256':hashlib.sha256(raw).hexdigest(), 'before':before, 'after':after}
    (dest/'budget-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(dest)


def parse_counts(path):
    rows = {'B':{}, 'T':{}, 'S':[], 'L':{}}
    for line in path.read_text().splitlines():
        fields = line.split('\t')
        if fields[0] in ['B','T']:
            rows[fields[0]][int(fields[1])] = list(map(int, fields[2:]))
        elif fields[0] == 'S':
            rows['S'].append({'id':int(fields[1]), 'phase':int(fields[2]),
                'counts':list(map(int,fields[3:8])), 'returned':int(fields[8]),
                'after_drop':int(fields[9]), 'pcs':fields[10:]})
        elif fields[0] == 'M':
            rows['missing'], rows['truncated'] = map(int, fields[1:])
        elif fields[0] == 'L':
            rows['L'][int(fields[1])] = [json.loads(f) for f in fields[2:]]
        elif fields[0] == 'H':
            rows['overhead_bytes'], rows['nested'] = map(int,fields[1:])
        elif fields[0] == 'P':
            rows['peak_by_phase'] = ast.literal_eval(fields[1])
            rows['after_drop'] = int(fields[2])
    return rows


def collect(root):
    root = root.resolve()
    evidence = root/'evidence'
    outputs = root/'outputs-v4'
    outputs.mkdir(exist_ok=False)
    fixtures = root/'fixtures'
    fixtures.mkdir(exist_ok=True)
    fixture_source = pathlib.Path(__file__).with_name('validate_c_parsing.py')
    fixture_module = ast.parse(fixture_source.read_text())
    cases = next(ast.literal_eval(node.value) for node in ast.walk(fixture_module)
                 if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id=='cases' for t in node.targets))
    for name, text in cases.items():
        (fixtures/(name+'.i')).write_text(text+'\n')
    binaries = {k:root/('target-'+k+'-v4/release/examples/allocation_budget_census') for k in ['base','diag']}
    result = {'runner_sha256':digest(pathlib.Path(__file__)), 'binaries':{k:{'path':str(p),'sha256':digest(p)} for k,p in binaries.items()}, 'rows':[], 'checks':{}}
    jobs = [('corpus',p,'x86_64',d) for p in sorted((root/'inputs').glob('*/pre/*.i')) for d in [False,True]]
    assert len(jobs)==88
    jobs += [('fixture',p,t,d) for p in sorted(fixtures.glob('*.i')) for t in ['x86_64','aarch64'] for d in [False,True]]
    with (evidence/'capture-commands.jsonl').open('w') as log:
        for index,(kind,source,target,debug) in enumerate(jobs):
            row = {'kind':kind,'input':str(source),'input_sha256':digest(source),'target':target,'debug':debug,'runs':[]}
            all_ir=[]; all_counts=[]
            for run,(mode,capture) in enumerate([('base',False),('diag',False),('diag',True),('diag',True)]):
                stem = outputs/('%03d-%s-%d'%(index,mode,run))
                ir = stem.with_suffix('.ir'); count = stem.with_suffix('.tsv')
                command = [str(binaries[mode]),target,str(debug).lower(),str(source),str(ir),str(count),str(capture).lower()]
                proc = subprocess.run(command, capture_output=True, text=True)
                command_row={'argv':command,'returncode':proc.returncode,'stdout':proc.stdout,'stderr':proc.stderr}
                log.write(json.dumps(command_row)+'\n');log.flush()
                assert proc.returncode==0, command_row
                counts = parse_counts(count)
                public = [sum(counts['B'][p][i] for p in range(5)) for i in range(6)]
                all_ir.append(ir.read_bytes());all_counts.append(counts)
                row['runs'].append({'mode':mode,'capture':capture,'ir':str(ir),'ir_sha256':digest(ir),'ir_bytes':ir.stat().st_size,'counts':str(count),'counts_sha256':digest(count),'public':public,'phase':counts['B'],'missing':counts['missing'],'truncated':counts['truncated'],'peak_by_phase':counts['peak_by_phase'],'overhead_bytes':counts.get('overhead_bytes'),'nested':counts.get('nested')})
            assert all(x==all_ir[0] for x in all_ir), row
            assert all(x['public']==row['runs'][0]['public'] for x in row['runs']), row
            row['raw_stack_rows_equal'] = all_counts[2]==all_counts[3]
            assert all_counts[2]['nested']==all_counts[3]['nested']==0
            assert all_counts[2]['B']==all_counts[3]['B'] and all_counts[2]['T']==all_counts[3]['T'], row
            assert all_counts[2]['missing']==0 and all_counts[2]['after_drop']==0, row
            for counts in all_counts[2:]:
                assert all(s['after_drop']==0 for s in counts['S'])
                assert [sum(s['counts'][i] for s in counts['S']) for i in range(5)] == [sum(counts['T'][p][i] for p in range(5)) for i in range(5)]
                assert sum(s['returned'] for s in counts['S']) == sum(x[2]-x[4] for x in counts['B'].values())
            diagnostic=all_ir[0].startswith(b'Diagnostic {')
            assert diagnostic==(kind=='fixture' and not source.stem.startswith('valid-')),row
            row['diagnostic']=all_ir[0].decode() if diagnostic else None
            result['rows'].append(row)
            with gzip.open(evidence/'captures-partial.json.gz','wt') as f:json.dump(result,f)
            print(index+1,len(jobs),source.name,target,debug,flush=True)
    result['checks']={'corpus':88,'fixtures':44,'captures_per_case':4,'exact_ir_and_diagnostics':True,'public_counter_reconciliation':True,'double_phase_captures_equal':True,'double_raw_stack_captures_equal':all(r['raw_stack_rows_equal'] for r in result['rows']),'all_origins_retired_after_drop':True}
    with gzip.open(evidence/'captures.json.gz','wt') as f:json.dump(result,f,indent=2)
    print(result['checks'],flush=True)


def symbolize(root):
    root=root.resolve(); evidence=root/'evidence'
    with gzip.open(evidence/'captures.json.gz','rt') as f:captures=json.load(f)
    binary=pathlib.Path(captures['binaries']['diag']['path'])
    pcs=set()
    for row in captures['rows']:
        for run in row['runs'][2:]:
            for s in parse_counts(pathlib.Path(run['counts']))['S']:
                pcs.update(int(p,16)-1 for p in s['pcs'] if p!='1')
    ordered=sorted(pcs)
    command=['llvm-symbolizer','--output-style=JSON','--inlines','--obj='+str(binary)]
    proc=subprocess.run(command,input=''.join(hex(pc)+'\n' for pc in ordered),capture_output=True,text=True,check=True)
    with gzip.open(evidence/'symbols-raw.jsonl.gz','wt') as f:f.write(proc.stdout)
    symbols={}
    for line in proc.stdout.splitlines():
        entry=json.loads(line);symbols[int(entry['Address'],16)]=entry['Symbol']
    assert set(symbols)==pcs
    def site(stack):
        frames=[]
        for pc in stack['pcs']:
            if pc=='1':continue
            frames.extend(symbols[int(pc,16)-1])
        project=[f for f in frames if '/crates/' in f['FileName'] and '/examples/' not in f['FileName']]
        if not project:return 'unknown',frames
        f=project[0]
        return 'crates/'+f['FileName'].split('/crates/',1)[1]+':'+str(f['Line'])+':'+f['FunctionName'],frames
    aggregate={}; sample_sites={}; mismatches=[]; lifetime={}; per_tu=[]; arena_audit=collections.Counter()
    for row in captures['rows']:
        groups=[]; life_groups=[]
        for run in row['runs'][2:]:
            grouped={}; lives={}
            raw=parse_counts(pathlib.Path(run['counts']))
            for s in raw['S']:
                key,frames=site(s)
                sample_sites.setdefault(key,frames)
                k=(s['phase'],key)
                values=s['counts']+[s['returned'],s['after_drop']]
                g=grouped.setdefault(k,[0]*7)
                for i,v in enumerate(values):g[i]+=v
                if raw['L']:
                    life=raw['L'][s['id']]
                    slot=lives.setdefault(k,[[0]*5 for _ in range(3)])
                    for a,values in zip(slot,life):
                        for i,v in enumerate(values):a[i]+=v
                if run is row['runs'][2] and 'entity.rs:66' in key:
                    candidates=[f for f in frames if '/crates/' in f['FileName'] and '/examples/' not in f['FileName'] and '/entity.rs' not in f['FileName']]
                    caller=candidates[0] if candidates else {}
                    expected={'Inst':552,'Value':(435,436,559),'Block':428,'StackSlot':354,'FuncRef':370,'DataRef':394,'SigRef':407}
                    kind=next((name for name in expected if 'Arena<keel_ir::'+name+',' in key),None)
                    allowed=expected.get(kind,())
                    if isinstance(allowed,int):allowed=(allowed,)
                    ok=caller.get('FileName','').endswith('/keel-ir/src/function.rs') and caller.get('Line') in allowed
                    arena_audit['validated_events' if ok else 'unknown_events']+=sum(s['counts'][:2])
            groups.append(grouped)
            life_groups.append(lives)
        if groups[0]!=groups[1] or life_groups[0]!=life_groups[1]:mismatches.append({'input':row['input'],'debug':row['debug'],'target':row['target'],'groups':[{str(k):v for k,v in g.items()} for g in groups]})
        per_tu.append({'input':row['input'],'debug':row['debug'],'target':row['target'],'kind':row['kind'], 'sites':{str(ph)+'|'+key:v for (ph,key),v in groups[0].items()}, 'lifetime':{str(ph)+'|'+key:v for (ph,key),v in life_groups[0].items()}})
        if row['kind']=='corpus':
            app=pathlib.Path(row['input']).parents[1].name
            bucket=aggregate.setdefault(app+':'+str(row['debug']).lower(),{})
            for (ph,key),values in groups[0].items():
                item=bucket.setdefault(str(ph)+'|'+key,[0]*7)
                for i,v in enumerate(values):item[i]+=v
            life_bucket=lifetime.setdefault(app+':'+str(row['debug']).lower(),{})
            for (ph,key),values in life_groups[0].items():
                item=life_bucket.setdefault(str(ph)+'|'+key,[[0]*5 for _ in range(3)])
                for a,b in zip(item,values):
                    for i,v in enumerate(b):a[i]+=v
    result={'command':command,'symbolizer_path':subprocess.check_output(['which','llvm-symbolizer'],text=True).strip(),
            'binary_sha256':digest(binary),'unique_pcs':len(pcs),'site_double_capture_mismatches':mismatches,'aggregate':aggregate,'site_sample_stacks':sample_sites,'lifetime':lifetime,'per_tu':per_tu,'arena_owner_audit':dict(arena_audit)}
    result['symbolizer_sha256']=digest(pathlib.Path(result['symbolizer_path']))
    with gzip.open(evidence/'sites.json.gz','wt') as f:json.dump(result,f,indent=2)
    print('unique PCs',len(pcs),'site mismatches',len(mismatches),flush=True)
    for app,sites in aggregate.items():
        print(app,'TOP EVENTS',flush=True)
        for k,v in sorted(sites.items(),key=lambda kv:sum(kv[1][:2]),reverse=True)[:15]:print(k,v,flush=True)


def public_counter(root):
    root=root.resolve();evidence=root/'evidence'
    with gzip.open(evidence/'captures.json.gz','rt') as f:captures=json.load(f)
    binary=root/'target-base-v4/release/examples/profile_allocations'
    result={'binary':str(binary),'sha256':digest(binary),'commands':[],'matched':0}
    by_input={(r['input'],r['debug']):r for r in captures['rows'] if r['kind']=='corpus'}
    for app in ['lua','quickjs','sqlite']:
        paths=sorted((root/'inputs'/app/'pre').glob('*.i'))
        for debug in [False,True]:
            cmd=[str(binary)]+list(map(str,paths))+(['--debug'] if debug else [])
            proc=subprocess.run(cmd,capture_output=True,text=True)
            result['commands'].append({'argv':cmd,'returncode':proc.returncode,'stdout':proc.stdout,'stderr':proc.stderr})
            assert proc.returncode==0
            for line in proc.stdout.splitlines():
                c=json.loads(line);expected=by_input[(c['path'],debug)]['runs'][0]['public']
                assert [c['alloc'],c['realloc'],c['requested_bytes'],c['dealloc']]==expected[:4],(c,expected)
                result['matched']+=1
    assert result['matched']==88
    (evidence/'public-counter.json').write_text(json.dumps(result,indent=2)+'\n')
    print('original public-counter matches',result['matched'],flush=True)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('command', choices=['generate','collect','symbolize','public'])
    p.add_argument('destination', type=pathlib.Path)
    p.add_argument('--diagnostic', action='store_true')
    a = p.parse_args()
    if a.command=='generate':generate(a.destination, a.diagnostic)
    elif a.command=='collect':collect(a.destination)
    elif a.command=='symbolize':symbolize(a.destination)
    else:public_counter(a.destination)
