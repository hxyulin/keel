"""Untimed attribution including temporary clone destruction, separate target.
The earlier diagnostic clock excludes temporary-drop time. This variant places
that drop inside the measured diagnostic region; it is never a latency binary.
"""
import json,shutil,sys
from profile_frontend_refresh import ROOT,inputs,run,store,sha,snapshot

def setup():
    dst=ROOT/'initializer-total-source';shutil.copytree(ROOT/'source',dst)
    text=(ROOT/'diagnostic-source/crates/keel-c/src/init.rs').read_text()
    old='            let clone_ns = clone_start.elapsed().as_nanos();'
    assert text.count(old)==1
    text=text.replace(old,'            drop(one); // diagnostic: include destruction of the temporary copies\n'+old)
    (dst/'crates/keel-c/src/init.rs').write_text(text)
    store('initializer-total-source',{'generator_sha256':sha(__file__),'files':{str(p.relative_to(dst)):sha(p) for p in dst.rglob('*.rs')}})

def collect():
    b=ROOT/'target-initializer-total/release/examples/profile';r={'start':snapshot(),'binary_sha256':sha(b),'runs':[],'parity':[]}
    for app,ps in inputs().items():
        for d in [False,True]:
            suffix=['--debug'] if d else []
            r['runs'].append(dict(app=app,debug=d,compile_passes=3,**run([b,'baseline','1',*ps,*suffix])))
            dest=ROOT/'evidence/ir'/f'{app}-{d}'/'initializer-total'
            x=run([b,'prepare','1',dest,*ps,*suffix]);base=dest.parent/'baseline'
            assert {p.name:sha(p) for p in dest.glob('*.ir')}=={p.name:sha(p) for p in base.glob('*.ir')}
            r['parity'].append(dict(app=app,debug=d,**x))
    r['end']=snapshot();store('initializer-total',r)
if __name__=='__main__':globals()[sys.argv[1]]()
