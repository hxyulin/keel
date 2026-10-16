"""Generate isolated UNTIMED fixed-source counters; never a latency binary.
No stack capture. Original public allocator forwarding is copied unchanged.
"""
import importlib.util,json,pathlib,shutil,sys
R=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('run',R/'run.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
DIAG='''
#[doc(hidden)]
pub mod pool_diagnostic {
    use std::cell::Cell;
    thread_local! { static COUNTS: Cell<[u64; 17]> = const { Cell::new([0; 17]) }; }
    pub fn add(index: usize, n: u64) { COUNTS.with(|c| { let mut a=c.get(); a[index]+=n; c.set(a); }); }
    pub fn counts() -> [u64;17] { COUNTS.with(|c| c.get()) }
    pub fn reset() { COUNTS.with(|c| c.set([0;17])); }
    pub fn growth(kind: usize, before: usize, after: usize, size: usize) {
        if before != after { add(kind, 1); add(kind+1, (after*size) as u64); }
    }
}
'''
def generate():
 for v in ['base','candidate']:
    dst=h.source(v,'counts-v2');dst.mkdir(exist_ok=False)
    src=h.source(v,'plain')
    for n in ['Cargo.toml','Cargo.lock']:shutil.copyfile(src/n,dst/n)
    for n in ['crates','.cargo']:shutil.copytree(src/n,dst/n)
    p=dst/'crates/keel-ir/src/lib.rs';p.write_text(p.read_text()+DIAG)
    p=dst/'crates/keel-c/src/irgen.rs';s=p.read_text().replace('                    self.module.functions.push(func);', '                    let cap=self.module.functions.capacity();\n                    self.module.functions.push(func);\n                    keel_ir::pool_diagnostic::growth(15,cap,self.module.functions.capacity(),std::mem::size_of::<Function>());');p.write_text(s)
    p=dst/'crates/keel-ir/src/entity.rs';s=p.read_text().replace('    pub fn len(&self) -> usize {','    pub fn diagnostic_capacity(&self) -> usize { self.items.capacity() }\n\n    pub fn len(&self) -> usize {');p.write_text(s)
    p=dst/'crates/keel-ir/src/function.rs';s=p.read_text()
    s=s.replace('    pub fn insert_inst(&mut self, at: InsertPoint, data: InstData) -> Inst {','    pub fn insert_inst(&mut self, at: InsertPoint, data: InstData) -> Inst {\n        crate::pool_diagnostic::add(8, 1);')
    if v=='candidate':
        s=s.replace('        self.insert_inst_with_scratch(at, data, &mut Vec::new())','''        let before = crate::pool_diagnostic::counts();
        let inst = self.insert_inst_with_scratch(at, data, &mut Vec::new());
        let after = crate::pool_diagnostic::counts();
        crate::pool_diagnostic::add(13, after[0]-before[0]);
        crate::pool_diagnostic::add(14, after[1]-before[1]);
        inst''')
    old='self.for_each_result_type(&data, |ty| result_tys.push(ty));'
    assert old in s
    s=s.replace(old,'''self.for_each_result_type(&data, |ty| {
            let before=result_tys.capacity(); result_tys.push(ty);
            crate::pool_diagnostic::growth(0,before,result_tys.capacity(),std::mem::size_of::<Type>());
        });''')
    if v=='candidate':
        old='        self.result_pool.reserve(result_tys.len());'
        s=s.replace(old,'''        let capacity = self.result_pool.capacity();
        self.result_pool.reserve(result_tys.len());
        crate::pool_diagnostic::growth(4, capacity, self.result_pool.capacity(), std::mem::size_of::<Value>());''')
    else:
        s=s.replace('            self.insts[inst].results.push(value);','''            let capacity = self.insts[inst].results.capacity();
            self.insts[inst].results.push(value);
            crate::pool_diagnostic::growth(2, capacity, self.insts[inst].results.capacity(), std::mem::size_of::<Value>());''')
    old='    fn create_inst(&mut self, data: InstData, result_tys: &[Type]) -> Inst {'
    s=s.replace(old,old+'''
        crate::pool_diagnostic::add(9, 1);
        crate::pool_diagnostic::add(10, result_tys.len() as u64);
        let node_capacity = self.insts.diagnostic_capacity();''')
    s=s.replace('        for (n, &ty) in result_tys.iter().enumerate() {','''        crate::pool_diagnostic::growth(6, node_capacity, self.insts.diagnostic_capacity(), std::mem::size_of::<InstNode>());
        for (n, &ty) in result_tys.iter().enumerate() {''',1)
    if v=='candidate':storage='[self.result_pool.len(), self.result_pool.capacity(), self.insts.iter().filter(|(_,n)| n.block.is_none()).map(|(_,n)| n.results.end-n.results.start).sum()]'
    else:storage='[self.insts.iter().map(|(_,n)| n.results.len()).sum(), self.insts.iter().map(|(_,n)| n.results.capacity()).sum(), self.insts.iter().filter(|(_,n)| n.block.is_none()).map(|(_,n)| n.results.len()).sum()]'
    s+='\nimpl Function { pub fn diagnostic_storage(&self) -> [usize;8] { let a = '+storage+'; [a[0],a[1],a[2],std::mem::size_of::<InstNode>(),std::mem::align_of::<InstNode>(),std::mem::size_of::<Function>(),self.insts.len(),self.insts.diagnostic_capacity()] } }\n'
    p.write_text(s)
    if v=='candidate':
        p=dst/'crates/keel-frontend/src/lib.rs';s=p.read_text().replace('    result_tys: Vec<Type>,','    result_tys: PoolScratch,').replace('            result_tys: Vec::new(),','            result_tys: PoolScratch(Vec::new()),');s+='''
struct PoolScratch(Vec<Type>);
impl std::ops::Deref for PoolScratch {
    type Target=Vec<Type>;
    fn deref(&self)->&Self::Target { &self.0 }
}
impl std::ops::DerefMut for PoolScratch {
    fn deref_mut(&mut self)->&mut Self::Target { &mut self.0 }
}
impl Drop for PoolScratch {
    fn drop(&mut self) {
        keel_ir::pool_diagnostic::add(11,1);
        keel_ir::pool_diagnostic::add(12,self.0.capacity() as u64);
    }
}
''';p.write_text(s)
    p=dst/'crates/keel-c/examples/profile_allocations.rs';s=p.read_text();s=s[:s.index('fn main()')]
    s=s.replace('    requested_bytes: u64,','    requested_bytes: u64,\n    live: i64,').replace('dealloc: 0, requested_bytes: 0,','dealloc: 0, requested_bytes: 0, live: 0,')
    s=s.replace('c.requested_bytes += layout.size() as u64;','c.requested_bytes += layout.size() as u64;\n                c.live += layout.size() as i64;')
    s=s.replace('c.dealloc += 1;','c.dealloc += 1;\n                c.live -= layout.size() as i64;')
    s=s.replace('c.requested_bytes += size as u64;','c.requested_bytes += size as u64;\n                c.live += size as i64-layout.size() as i64;')
    s+='''
fn main() -> Result<(),Box<dyn std::error::Error>> {
    let a:Vec<_>=std::env::args().skip(1).collect();
    let opts=keel_c::Options{debug_info:a.iter().any(|x|x=="--debug"),..Default::default()};
    let inputs:Vec<_>=a.iter().filter(|x|*x!="--debug").map(|p|Ok((p,std::fs::read_to_string(p)?))).collect::<Result<_,std::io::Error>>()?;
    for (path,src) in inputs {
        keel_ir::pool_diagnostic::reset();
        COUNTS.with(|c|c.set(Counts{enabled:true,..Default::default()}));
        let result=keel_c::compile(&src,path,opts);
        let before=COUNTS.with(|c|{let n=c.get();c.set(Counts::default());n});
        let sites=keel_ir::pool_diagnostic::counts();
        let mut storage=[0usize;8];
        match &result {
            Ok(m)=>{keel_ir::verify_module(m).map_err(|e|format!("{e:?}"))?;for f in &m.functions {let a=f.diagnostic_storage();for i in 0..3{storage[i]+=a[i];} storage[3]=a[3];storage[4]=a[4];storage[5]=a[5];storage[6]+=a[6];storage[7]+=a[7];}},
            Err(e)=>eprintln!("{e:?}"),
        }
        COUNTS.with(|c|c.set(Counts{enabled:true,..Default::default()}));
        drop(result);
        let after=COUNTS.with(|c|{let n=c.get();c.set(Counts::default());n});
        println!("{{\\\"path\\\":{path:?},\\\"alloc\\\":{},\\\"realloc\\\":{},\\\"requested_bytes\\\":{},\\\"returned_live\\\":{},\\\"after_drop_live\\\":{},\\\"sites\\\":{sites:?},\\\"storage\\\":{storage:?}}}",before.alloc,before.realloc,before.requested_bytes,before.live,before.live+after.live);
    }
    Ok(())
}
'''
    (dst/'crates/keel-c/examples/pool_counts.rs').write_text(s)
    h.save(dst.name+'-source',{str(p.relative_to(dst)):h.sha(p) for p in sorted(dst.rglob('*')) if p.is_file()})

def build():
 for v in ['base','candidate']:
    dst=h.source(v,'counts-v2');env=dict(h.ENV,CARGO_TARGET_DIR=str(R/('target-'+dst.name)))
    h.save(dst.name+'-build',h.run(['cargo','build','--release','--locked','-p','keel-c','--example','pool_counts'],dst,env=env))
    p=h.binary(v,'counts-v2','examples/pool_counts');h.save(dst.name+'-binary',dict(path=str(p),sha256=h.sha(p),bytes=p.stat().st_size))

def capture():
 rows=[]
 for a,ps in h.h.inputs().items():
    for d in ['normal','debug']:
        for v in ['base','candidate']:
            x=h.run([h.binary(v,'counts-v2','examples/pool_counts'),*ps,*(['--debug'] if d=='debug' else [])]);rows.append(dict(app=a,debug=d,variant=v,rows=[json.loads(s) for s in x['stdout'].splitlines()]))
 h.save('source-counts',rows)
 for v in ['base','candidate']:
    h.save(v+'-error-counts',h.run([h.binary(v,'counts-v2','examples/pool_counts'),R/'irgen-failure.c']))

if __name__=='__main__':
 for op in sys.argv[1:]:globals()[op]()
