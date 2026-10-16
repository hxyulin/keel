"""Offline, disjoint source-site allocation budget; no compiler or timing runs."""
import collections
import gzip
import json
import pathlib
import sys


def category(key, source):
    path,line,symbol=key.split('|',1)[1].split(':',2)
    line=int(line)
    text=(source/path).read_text().splitlines()[line-1]
    if path.endswith('/strings.rs'):return 'interned bytes and interner tables'
    if path.endswith('/lex.rs'):
        return 'lexer token buffer' if line==177 else 'lexer literal scratch and file metadata'
    if path.endswith('/ast.rs'):return 'AST derived-clone payloads (Box and Vec)'
    if path.endswith('/expr.rs'):
        if 'Box::new' in text:return 'AST expression Box payloads'
        if line in [706,725,1263,1478,1479,1485,1512,1561,1574,1586]:return 'AST expression lists and conversion scratch'
        return 'parser literal/name scratch'
    if path.endswith('/init.rs'):
        if line==251:return 'initializer snapshot Vec (excludes recursive clone)'
        if line in [89,102]:return 'AST flattened initializer payload Vec'
        return 'initializer traversal scratch'
    if path.endswith('/types.rs'):
        return 'IR ABI layout payloads' if line==452 else 'C type nodes, sugar, and member tables'
    if path.endswith('/parse.rs'):
        if line in [305,340,521,1705,1817]:return 'C type Rc and function-parameter Vec'
        if 'Box::new' in text:return 'AST statement Box payloads'
        if line in [864,2033,2035,2117,2186,2235]:return 'AST statement/declaration lists'
        return 'parser bindings, declarations and parameter scratch'
    if path.endswith('/entity.rs'):
        for typ in ['InstNode','ValueData','BlockData']:
            if typ in symbol:return 'IR arena '+typ
        return 'IR other entity arenas'
    if path.endswith('/function.rs'):
        return {531:'IR temporary result-type Vec',563:'IR persistent instruction-result Vec',
                578:'IR persistent block instruction Vec',439:'IR persistent block parameter Vec',
                429:'IR persistent block layout Vec',99:'IR signature Vec',100:'IR signature Vec',
                372:'IR symbol String',394:'IR symbol String'}.get(line,'owned type unclassified (identified source site)')
    if path.endswith('/builder.rs'):return 'IR operand/branch/case payloads'
    if path.startswith('crates/keel-frontend/'):
        return 'IR branch argument Vec' if line==350 else 'SSA builder transient storage'
    if path.endswith('/layout.rs'):return 'IR ABI layout payloads'
    if path.endswith('/module.rs'):return 'IR attribute-sync transient names and buffers'
    if path.endswith('/debug.rs'):return 'debug type/variable representation storage'
    if path.endswith('/irgen.rs'):
        return 'IR signature Vec' if line in [277,286,293] else 'IR generation other (mixed final and transient)'
    return 'owned type unclassified (identified source site)'


def summarize(report, source):
    with gzip.open(report/'sites.json.gz','rt') as f:sites=json.load(f)
    with gzip.open(report/'captures.json.gz','rt') as f:captures=json.load(f)
    classification={k:category(k,source) for group in sites['aggregate'].values() for k in group}
    def budgets(group):
        result={}
        for k,v in group.items():
            out=result.setdefault(classification.get(k) or category(k,source),[0]*7)
            for i,x in enumerate(v):out[i]+=x
        return result
    aggregate={app:budgets(v) for app,v in sites['aggregate'].items()}
    for debug in ['false','true']:
        group={}
        for app,rows in aggregate.copy().items():
            if app.endswith(':'+debug):
                for k,v in rows.items():
                    q=group.setdefault(k,[0]*7)
                    for i,x in enumerate(v):q[i]+=x
        aggregate['all:'+debug]=group
    result={'columns':['alloc','realloc','requested_bytes','dealloc_calls_entire_lifecycle','retired_bytes_entire_lifecycle','live_bytes_after_compile','live_bytes_after_returned_drop'],
            'classification':classification,'aggregate':aggregate,'phase':{},'lifetime':{},'representatives':{},'coverage':{}}
    for row in captures['rows']:
        if row['kind']!='corpus':continue
        app=pathlib.Path(row['input']).parents[1].name+':'+str(row['debug']).lower()
        group=result['phase'].setdefault(app,{'totals':[[0]*6 for _ in range(5)],'max_tu_peak_live':0,'truncated_events':0,'instrumentation_bytes':row['runs'][2]['overhead_bytes']})
        for ph,v in row['runs'][2]['phase'].items():
            for i,x in enumerate(v):group['totals'][int(ph)][i]+=x
        group['max_tu_peak_live']=max(group['max_tu_peak_live'],*row['runs'][2]['peak_by_phase'])
        group['truncated_events']+=row['runs'][2]['truncated']
    for app,rows in sites['lifetime'].items():
        group=result['lifetime'].setdefault(app,{})
        for k,v in rows.items():
            out=group.setdefault(classification[k],[[0]*5 for _ in range(3)])
            for a,b in zip(out,v):
                for i,x in enumerate(b):a[i]+=x
    for row in sites['per_tu']:
        if row['kind']=='corpus' and pathlib.Path(row['input']).stem in ['lvm','quickjs','sqlite3']:
            result['representatives'][pathlib.Path(row['input']).stem+':'+str(row['debug']).lower()]=budgets(row['sites'])
    for app,rows in aggregate.items():
        total=sum(sum(v[:2]) for v in rows.values());requested=sum(v[2] for v in rows.values())
        options={
            'function_result_storage':['IR temporary result-type Vec','IR persistent instruction-result Vec'],
            'broader_IR_result_operand_block_storage':['IR temporary result-type Vec','IR persistent instruction-result Vec','IR persistent block instruction Vec','IR persistent block parameter Vec','IR persistent block layout Vec','IR operand/branch/case payloads','IR branch argument Vec'],
            'AST_payload_storage':['AST derived-clone payloads (Box and Vec)','AST expression Box payloads','AST expression lists and conversion scratch','AST flattened initializer payload Vec','AST statement Box payloads','AST statement/declaration lists'],
        }
        result['coverage'][app]={name:{'events':sum(sum(rows.get(c,[0]*7)[:2]) for c in categories),'event_fraction':sum(sum(rows.get(c,[0]*7)[:2]) for c in categories)/total,'requested_bytes':sum(rows.get(c,[0]*7)[2] for c in categories),'requested_fraction':sum(rows.get(c,[0]*7)[2] for c in categories)/requested} for name,categories in options.items()}
    (report/'budget.json').write_text(json.dumps(result,indent=2)+'\n')
    lines=[]
    for mode in ['false','true']:
        rows=aggregate['all:'+mode];total=sum(sum(v[:2]) for v in rows.values());req=sum(v[2] for v in rows.values())
        lines+=['','Normal' if mode=='false' else 'Debug','','| Disjoint category | Alloc + realloc | Events % | Requested MB | Bytes % | Live at compile return MB¹ |','|---|---:|---:|---:|---:|---:|']
        for k,v in sorted(rows.items(),key=lambda kv:sum(kv[1][:2]),reverse=True):
            lines.append('| %s | %s | %.2f | %.3f | %.2f | %.3f |'%(k,format(sum(v[:2]),','),100*sum(v[:2])/total,v[2]/1e6,100*v[2]/req,v[5]/1e6))
    (report/'BUDGET-TABLES.md').write_text('\n'.join(lines)+'\n\n¹ Sum over serial TUs; never simultaneously live. MB = 1,000,000 bytes.\n')
    print(json.dumps(result['coverage'],indent=2))


if __name__=='__main__':summarize(pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]))
