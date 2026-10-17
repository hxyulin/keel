"""Subprocess-only native backend. All executions have deadlines and durable logs."""
import hashlib
import json
import os
import platform
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from .manifest import LabError, source
from .cancel import TerminationRequested, defer_spawn_cancellation, ignore_cancellation
from .stats import schedule, summarize, common_iterations

FLAGS=['-O2','-fno-lto','-Wall','-Wextra','-Werror']

def digest(p):
    h=hashlib.sha256()
    with open(p,'rb') as f:
        for b in iter(lambda:f.read(65536),b''): h.update(b)
    return h.hexdigest()

def write(path, value):
    with open(path,'x') as f: json.dump(value,f,indent=2,sort_keys=True); f.write('\n')

def hostarch():
    return {'arm64':'aarch64','AMD64':'x86_64'}.get(platform.machine(),platform.machine())

class Runner:
    def __init__(self, output, timeout=20, budget=180):
        self.output=Path(output).resolve()
        self.output.mkdir(parents=True,exist_ok=False)
        self.timeout=timeout
        self.deadline=time.monotonic()+budget
        self.commands=[]
        self.evidence=None
    def run(self, argv, allow_failure=False):
        if time.monotonic()>=self.deadline: raise LabError('campaign deadline exceeded')
        argv=[str(x) for x in argv]
        n=len(self.commands); stem=self.output/f'command-{n:05}'
        rec={'argv':argv,'timeout_seconds':min(self.timeout,self.deadline-time.monotonic()),'stdout':str(stem)+'.stdout','stderr':str(stem)+'.stderr'}
        self.commands.append(rec)
        p=None
        try:
            with open(rec['stdout'],'xb') as out, open(rec['stderr'],'xb') as err:
                with defer_spawn_cancellation():
                    p=subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=out,stderr=err,shell=False,start_new_session=(os.name=='posix'))
                rec['pid']=p.pid
                rec['returncode']=p.wait(timeout=rec['timeout_seconds'])
        except subprocess.TimeoutExpired:
            rec['status']='timeout'
        except OSError as e:
            rec['status']='launch_failed'; rec['error']=str(e)
        except BaseException as e:
            rec['status']='interrupted' if isinstance(e,KeyboardInterrupt) else 'exceptional_exit'
            rec['exception']=type(e).__name__
            if isinstance(e,KeyboardInterrupt):
                rec['signal']='SIGTERM' if isinstance(e,TerminationRequested) else 'SIGINT'
            raise
        finally:
            with ignore_cancellation():
                if p is not None and 'returncode' not in rec:
                    rec['pid']=p.pid
                    # The new session belongs to this Popen only. Kill the group
                    # even if the leader exited, so its remaining children stop.
                    try:
                        if os.name=='posix': os.killpg(p.pid,signal.SIGKILL)
                        else: p.kill()
                    except ProcessLookupError:
                        pass
                    except OSError as e:
                        rec['cleanup_error']=str(e)
                    try:
                        rec['returncode']=p.wait(timeout=5)
                        rec['cleanup']='owned process group killed; child reaped' if os.name=='posix' else 'child killed and reaped'
                    except subprocess.TimeoutExpired:
                        rec['cleanup_error']='child not reaped within five seconds after kill'
                for stream in ('stdout','stderr'):
                    if Path(rec[stream]).is_file(): rec[stream+'_sha256']=digest(rec[stream])
                write(str(stem)+'.json',rec)
        if (rec.get('returncode')!=0 or rec.get('status')) and not allow_failure:
            raise LabError(f'command failed: {rec}; logs retained')
        with open(rec['stdout'],'rb') as f: text=f.read(2*1024*1024).decode(errors='replace')
        return rec,text

def executable(name):
    p=shutil.which(name)
    if not p: raise LabError(f'executable unavailable: {name}')
    return str(Path(p).resolve())

# Resolve only Apple's documented system launchers. Explicit custom executables
# retain their identity; arbitrary wrappers cannot be inferred from --version.
APPLE_LAUNCHERS={'/usr/bin/clang':'clang','/usr/bin/clang++':'clang++','/usr/bin/otool':'otool'}
XCRUN='/usr/bin/xcrun'

def identity(path):
    p=Path(path).resolve()
    if not p.is_file() or not os.access(p,os.X_OK):
        raise LabError(f'tool is not a regular executable: {p}')
    return {'path':str(p),'sha256':digest(p)}

def resolve_tool(r, requested):
    selected=executable(requested)
    requested_identity=identity(selected)
    result={**requested_identity,'status':'available','requested':{'argument':str(requested),**requested_identity},'identity_scope':'selected executable; arbitrary wrapper delegation is not inferred'}
    if platform.system()=='Darwin' and selected in APPLE_LAUNCHERS:
        resolver=identity(XCRUN)
        # Snapshot the selection environment, disable xcrun's lookup cache, and
        # invoke the result directly so later developer-selection changes cannot
        # redirect subsequent compiler/disassembler commands.
        selection={k:os.environ.get(k) for k in ('DEVELOPER_DIR','TOOLCHAINS','SDKROOT')}
        rec,text=r.run([resolver['path'],'--no-cache','--find',APPLE_LAUNCHERS[selected]],allow_failure=True)
        result['resolver']={**resolver,'selection_environment':selection,'command':rec}
        if rec.get('returncode')!=0 or len(text.strip().splitlines())!=1 or not Path(text.strip()).is_absolute():
            raise LabError(f'could not resolve Apple tool launcher: {selected}; resolver logs retained')
        actual=identity(text.strip())
        if actual['path'] in APPLE_LAUNCHERS or actual['path']==selected:
            raise LabError(f'Apple tool resolution returned a launcher: {actual["path"]}')
        result.update(actual)
        result['identity_scope']='actual executable resolved by xcrun; invoked directly'
    return result

def tool_fingerprints(tool):
    files={}
    for record in (tool,tool.get('requested',{}),tool.get('resolver',{})):
        if record.get('path') and record.get('sha256'): files[record['path']]=record['sha256']
    return files

def build(r, root, selected, clang):
    evidence={'compiler':{'status':'unused'},'host':{'arch':hostarch(),'platform':platform.platform(),'uname':list(platform.uname())},'flags':FLAGS,'patterns':[],'inputs':{},'artifacts':{}}
    r.evidence=evidence
    for p in sorted(root.glob('lab/*.py'))+list(root.glob('run.py'))+list(root.glob('driver.c'))+list(root.glob('manifest.schema.json')):
        evidence['inputs'][str(p.resolve())]=digest(p)
    # Git provenance is diagnostic; hashes are the replay/mutation boundary.
    for arg,key in [(['rev-parse','HEAD'],'commit'),(['rev-parse','HEAD^{tree}'],'tree'),(['status','--porcelain'],'dirty')]:
        _,v=r.run(['git','-C',str(root),*arg],allow_failure=True); evidence[key]=v.strip()
    # Optional adapter stays behind its standalone CLI boundary. Discovery
    # probes versions/help only; it never runs MCA or instruction benchmarks.
    if (root/'tools.py').is_file():
        evidence['inputs'][str((root/'tools.py').resolve())]=digest(root/'tools.py')
        rec,_=r.run([sys.executable,'-B',root/'tools.py','discover','--output',r.output/'tools-discovery.json'],allow_failure=True)
        evidence['optional_tools']={'command':rec,'report':str(r.output/'tools-discovery.json'),'status':'recorded' if rec.get('returncode')==0 else 'discovery_failed'}
    driver=None
    dis=shutil.which('otool') if platform.system()=='Darwin' else (shutil.which('llvm-objdump') or shutil.which('objdump'))
    evidence['disassembler']={'status':'available' if dis else 'unsupported','path':dis,'reason':None if dis else 'no otool/objdump found'}
    if dis:
        try:
            evidence['disassembler']=resolve_tool(r,dis)
            dis=evidence['disassembler']['path']
            rec,version=r.run([dis,'--version'] if platform.system()!='Darwin' else [dis,'-h'],allow_failure=True)
            evidence['disassembler'].update(version_probe=rec,version_output=version)
            evidence['inputs'].update(tool_fingerprints(evidence['disassembler']))
        except LabError as e:
            evidence['disassembler']={'status':'unresolved','requested':identity(dis),'reason':str(e)}
            evidence['inputs'].update(tool_fingerprints(evidence['disassembler']))
            dis=None
    for path,m in selected:
        mp=str(path.resolve()); evidence['inputs'][mp]=digest(path)
        for s in [m['oracle'],*m['variants'],*m.get('mca',{}).values()]:
            p=source(root,s['source']); evidence['inputs'][str(p)]=digest(p)
        row={'id':m['id'],'manifest':m,'manifest_path':mp,'variants':{}}
        evidence['patterns'].append(row)
        if m['arch']!=hostarch():
            row['status']='unsupported'; row['reason']='manifest arch differs from native host'; continue
        if driver is None:
            compiler=resolve_tool(r,clang)
            cc=compiler['path']
            _,version=r.run([cc,'--version'])
            if 'clang' not in version.lower(): raise LabError('selected compiler is not Clang')
            evidence['compiler']={**compiler,'version':version}
            evidence['inputs'].update(tool_fingerprints(evidence['compiler']))
            driver=r.output/'driver.o'
            r.run([cc,*FLAGS,'-std=c11','-c',root/'driver.c','-o',driver])
            evidence['artifacts'][str(driver)]=digest(driver)
        row['status']='built'
        oracle=r.output/f"{m['id']}-oracle.o"
        r.run([cc,*FLAGS,'-std=c11','-c',source(root,m['oracle']['source']),'-o',oracle])
        evidence['artifacts'][str(oracle)]=digest(oracle)
        for v in m['variants']:
            stem=r.output/f"{m['id']}-{v['id']}"
            obj=Path(str(stem)+'.o'); binary=Path(str(stem)+'.bin')
            r.run([cc,*FLAGS,'-c',source(root,v['source']),'-o',obj])
            r.run([cc,'-fno-lto',driver,oracle,obj,'-o',binary])
            row['variants'][v['id']]={'binary':str(binary),'object':str(obj)}
            evidence['artifacts'].update({str(obj):digest(obj),str(binary):digest(binary)})
            if dis:
                for artifact in (obj,binary):
                    rec,_=r.run([dis,'-tvV' if platform.system()=='Darwin' else '-d',artifact],allow_failure=True)
                    if rec.get('returncode')!=0: row.setdefault('disassembly_failures',[]).append(rec)
    return evidence

def invoke(r, binary, m, iterations, seed, timer):
    rec,text=r.run([binary,str(iterations),str(seed),str(m['data_words']),str(m['mask']),timer],allow_failure=True)
    if rec.get('returncode')==3:
        raise LabError(f'unsupported timer: {text.strip()}')
    if rec.get('returncode')!=0: raise LabError(f'driver failed: {rec}; logs retained')
    try: s=json.loads(text)
    except ValueError as e: raise LabError('driver emitted invalid JSON') from e
    if s.get('status')!='ok' or s.get('checksum')!=s.get('oracle') or s.get('iterations')!=iterations or s.get('seed')!=seed or (iterations==0 and s['checksum']!=seed):
        raise LabError('driver checksum/protocol mismatch')
    if s.get('unit')!=('ticks' if timer=='tsc' else 'ns') or type(s.get('elapsed')) is not int or s['elapsed']<0 or type(s.get('wall_ns')) is not int or s['wall_ns']<0:
        raise LabError('driver timer protocol mismatch')
    return s

def check(r, evidence):
    cases=[]
    for row in evidence['patterns']:
        if row['status']=='unsupported': continue
        m=row['manifest']
        for v,paths in row['variants'].items():
            for seed in m['seeds']:
                for n in m['correctness_iterations']:
                    s=invoke(r,paths['binary'],m,n,seed,'monotonic')
                    cases.append({'pattern':m['id'],'variant':v,**s})
        row['status']='checked'
    write(r.output/'correctness.json',cases)
    if verify_record(fingerprints(evidence)): raise LabError('mutation during build/check')
    return cases

def fingerprints(evidence):
    files={**evidence['inputs'],**evidence['artifacts']}
    for tool in (evidence['compiler'],evidence.get('disassembler',{})):
        files.update(tool_fingerprints(tool))
    return files

def verify_record(record):
    errors=[]
    for name,expected in record.items():
        try:
            if digest(name)!=expected: errors.append({'path':name,'status':'changed'})
        except OSError: errors.append({'path':name,'status':'missing'})
    return errors

def bench(r,evidence,args):
    protocol={'timer':args.timer,'target_ns':args.target_ms*1000000,'pilot_iterations':args.pilot_iterations,'max_iterations':args.max_iterations,'cycles':args.cycles,'warmups':2,'seed_policy':'first manifest seed','schedule':'all permutations in manifest variant order per cycle','bootstrap':'paired whole blocks within order strata; deterministic 2000 draws','no_outlier_removal':True,'no_empty_baseline_subtraction':True}
    write(r.output/'protocol.json',protocol)
    frozen=fingerprints(evidence); frozen[str(r.output/'protocol.json')]=digest(r.output/'protocol.json')
    write(r.output/'frozen.json',frozen)
    results=[]
    for row in evidence['patterns']:
        if row['status']!='checked': continue
        m=row['manifest']; ids=list(row['variants']); seed=m['seeds'][0]
        if verify_record(frozen): raise LabError('mutation before calibration')
        cal=[{'variant':v,**invoke(r,row['variants'][v]['binary'],m,args.pilot_iterations,seed,args.timer)} for v in ids]
        write(r.output/f"{m['id']}-calibration.json",cal)
        count=common_iterations(cal,protocol['target_ns'],args.max_iterations)
        orders=schedule(ids,args.cycles)
        write(r.output/f"{m['id']}-fixed.json",{'iterations':count,'orders':orders})
        warm=[]
        for _ in range(2):
            for v in ids: warm.append({'variant':v,**invoke(r,row['variants'][v]['binary'],m,count,seed,args.timer)})
        write(r.output/f"{m['id']}-warmup.json",warm)
        blocks=[]
        for order in orders:
            block=[]
            for v in order:
                s=invoke(r,row['variants'][v]['binary'],m,count,seed,args.timer)
                block.append({'variant':v,'work_count':count*m['work_per_iteration'],'work_unit':m['work_unit'],**s})
            blocks.append(block)
        write(r.output/f"{m['id']}-samples.json",blocks)
        if verify_record(frozen): raise LabError('mutation during benchmark')
        results.append({'pattern':m['id'],'iterations':count,'work_unit':m['work_unit'],**summarize(blocks,ids,count*m['work_per_iteration'])})
    write(r.output/'results.json',results)

def seal(r,evidence):
    files=fingerprints(evidence) if evidence else {}
    for p in r.output.iterdir():
        if p.is_file() and p.name!='integrity.json': files[str(p)]=digest(p)
    write(r.output/'integrity.json',files)
