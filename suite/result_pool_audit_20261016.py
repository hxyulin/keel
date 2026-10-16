"""Offline arithmetic and parity audit; no compiler, timer, or network access."""
import csv,hashlib,json,pathlib,statistics,sys
R=pathlib.Path(sys.argv[1]);E=R/'evidence'
def load(n):return json.loads((E/(n+'.json')).read_text())
def save(n,x):(E/(n+'.json')).write_text(json.dumps(x,indent=2)+'\n')
public={ (x['debug'],y['path'],x['variant']):y for x in load('public') for y in x['rows'] }
source={ (x['debug'],y['path'],x['variant']):y for x in load('source-counts') for y in x['rows'] }
rows=[]
for d,p,v in sorted(source):
    if v!='base':continue
    b=source[d,p,'base'];c=source[d,p,'candidate'];bs=b['sites'];cs=c['sites']
    for variant,y in [('base',b),('candidate',c)]:
        assert y['after_drop_live']==0,(d,p,variant,'leak')
        for k in ['alloc','realloc','requested_bytes']: assert y[k]==public[d,p,variant][k],(d,p,variant,k)
    assert bs[9:11]==cs[9:11],(p,'insertion/result visits')
    # Selected sites, node arena, and larger returned Function-vector storage.
    be=bs[0]+bs[2]+bs[4]+bs[6]+bs[15];ce=cs[0]+cs[2]+cs[4]+cs[6]+cs[15]
    bb=bs[1]+bs[3]+bs[5]+bs[7]+bs[16];cb=cs[1]+cs[3]+cs[5]+cs[7]+cs[16]
    event_saved=b['alloc']+b['realloc']-c['alloc']-c['realloc'];byte_saved=b['requested_bytes']-c['requested_bytes']
    assert event_saved==be-ce,(d,p,'events',event_saved,be-ce)
    assert byte_saved==bb-cb,(d,p,'bytes',byte_saved,bb-cb)
    rows.append(dict(debug=d,path=p,app=pathlib.Path(p).parts[-3],base_events=b['alloc']+b['realloc'],candidate_events=c['alloc']+c['realloc'],events_saved=event_saved,base_bytes=b['requested_bytes'],candidate_bytes=c['requested_bytes'],bytes_saved=byte_saved,base_sites=bs,candidate_sites=cs,base_storage=b['storage'],candidate_storage=c['storage'],base_returned_live=b['returned_live'],candidate_returned_live=c['returned_live']))
assert len(rows)==88
save('allocation-reconciliation',rows)
with (E/'allocation-reconciliation.csv').open('w') as f:
    keys=[k for k in rows[0] if not k.endswith(('sites','storage'))];w=csv.DictWriter(f,fieldnames=keys,extrasaction='ignore');w.writeheader();w.writerows(rows)
summary={}
for d in ['normal','debug']:
 for app in ['lua','quickjs','sqlite','all']:
    rs=[x for x in rows if x['debug']==d and (app=='all' or x['app']==app)]
    a={k:sum(x[k] for x in rs) for k in ['base_events','candidate_events','events_saved','base_bytes','candidate_bytes','bytes_saved','base_returned_live','candidate_returned_live']}
    for k in ['base_sites','candidate_sites','base_storage','candidate_storage']:
        a[k]=[sum(x[k][i] for x in rs) for i in range(len(rs[0][k]))]
    a['event_reduction']=a['events_saved']/a['base_events'];a['byte_reduction']=a['bytes_saved']/a['base_bytes'];summary[d+'/'+app]=a
save('allocation-summary',summary)
# Late errors have an exact expectation, rather than merely matching arbitrary errors.
for target in ['x86','darwin']:
 for d in ['normal','debug']:
    x=load('late-error-'+target+'-'+d)
    expected='Diagnostic { file: '+json.dumps(x['path'])+', line: 1, col: 46, message: '+json.dumps('`__int128` to float is not supported')+' }'
    assert x['diagnostic']==expected
for v in ['base','candidate']:
    x=load(v+'-error-counts');rs=[json.loads(s) for s in x['stdout'].splitlines()]
    assert len(rs)==1 and rs[0]['after_drop_live']==0
    assert '`__int128` to float is not supported' in x['stderr']
    if v=='candidate':assert rs[0]['sites'][11]>0 and rs[0]['sites'][12]>0
parity=load('parity');assert len(parity)==24
for x in parity:
    assert all(n.endswith('.ir') for n in x['files'])
    for name,digest in x['files'].items():
        p=E/'ir'/x['target']/x['debug']/x['app']/x['variant']/name
        assert hashlib.sha256(p.read_bytes()).hexdigest()==digest
assert sum(len(x['files']) for x in parity if x['variant']=='candidate')==176
save('audit',dict(per_tu_allocations=88,exact_event_and_byte_reconciliations=88,strict_full_debug_ir_pairs=176,late_error_exact_diagnostics=4,public_counter_agreement=176,post_drop_live_bytes=0,layouts=sorted(set((tuple(x['base_storage'][3:6]),tuple(x['candidate_storage'][3:6])) for x in rows))))
print(json.dumps(load('audit'),indent=2))
