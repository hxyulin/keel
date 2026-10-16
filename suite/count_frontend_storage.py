"""Untimed, exact-source counters for two bounded storage questions.
The copy/target are new: prior diagnostic or production artifacts never change.
"""
import json,pathlib,shutil,sys
from profile_frontend_refresh import ROOT,inputs,run,store,sha,snapshot

def setup():
    dst=ROOT/'storage-source';shutil.copytree(ROOT/'source',dst)
    p=dst/'crates/keel-ir/src/function.rs';t=p.read_text()
    old='''        let inst = self.create_inst(data, &result_tys);
        self.place(inst, at);'''
    assert t.count(old)==1
    t=t.replace(old,'''        INSERT_STORAGE.with(|s| {let mut s=s.borrow_mut();
            s.0+=1;s.1+=u64::from(result_tys.capacity()!=0);
            s.2+=result_tys.capacity() as u64 * std::mem::size_of::<Type>() as u64;
            s.3+=u64::from(result_tys.len()>1);
        });
'''+old)
    t+='''
#[derive(Default)] struct InsertStorage(u64,u64,u64,u64);
impl Drop for InsertStorage {fn drop(&mut self) {eprintln!("INSERT_STORAGE calls={} heap_vectors={} capacity_bytes={} multi_result={} type_size={}",self.0,self.1,self.2,self.3,std::mem::size_of::<Type>());}}
thread_local! {static INSERT_STORAGE:std::cell::RefCell<InsertStorage>=Default::default();}
''';p.write_text(t)
    p=dst/'crates/keel-c/src/strings.rs';t=p.read_text()
    old='''    pub fn intern(&mut self, text: &str) -> &'a str {
        if let Some(&name) = self.names.get(text) {'''
    assert t.count(old)==1
    t=t.replace(old,old.replace('        if let', '        INTERN_STORAGE.with(|s| {let mut s=s.borrow_mut();s.0+=1;s.2+=text.len() as u64;});\n        if let'))
    t=t.replace('        let name = self.arena.alloc_str(text);','        INTERN_STORAGE.with(|s| {s.borrow_mut().1+=1;});\n        let name = self.arena.alloc_str(text);')
    t+='''
#[derive(Default)] struct InternStorage(u64,u64,u64);
impl Drop for InternStorage {fn drop(&mut self) {eprintln!("INTERN_STORAGE calls={} misses={} hash_input_bytes={}",self.0,self.1,self.2);}}
thread_local! {static INTERN_STORAGE:std::cell::RefCell<InternStorage>=Default::default();}
''';p.write_text(t)
    store('storage-source',{'files':{str(p.relative_to(dst)):sha(p) for p in dst.rglob('*.rs')},'generator_sha256':sha(__file__)})

def collect():
    b=ROOT/'target-storage/release/examples/profile';r={'start':snapshot(),'binary_sha256':sha(b),'runs':[],'parity':[]}
    for a,ps in inputs().items():
        for d in [False,True]:
            suf=['--debug'] if d else []
            x=run([b,'baseline','1',*ps,*suf]);r['runs'].append(dict(app=a,debug=d,compile_passes=3,**x))
            dest=ROOT/'evidence/ir'/f'{a}-{d}'/'storage'
            x=run([b,'prepare','1',dest,*ps,*suf]);base=dest.parent/'baseline'
            assert {p.name:sha(p) for p in dest.glob('*.ir')}=={p.name:sha(p) for p in base.glob('*.ir')}
            r['parity'].append(dict(app=a,debug=d,**x))
    r['end']=snapshot();store('storage',r)
if __name__=='__main__':globals()[sys.argv[1]]()
