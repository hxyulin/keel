#!/usr/bin/env python3
"""Opt-in Linux/x86-64 research runner; see README.md for scope and privileges."""
import argparse
import ctypes
import hashlib
import itertools
import json
import os
import pathlib
import pwd
import random
import resource
import select
import shutil
import signal
import struct
import subprocess
import tempfile
import time
import traceback

PAGE=4096
MAGIC=0x4b45454c50414745

def digest(data):
    return hashlib.sha256(data).hexdigest()

def checked_file(spec):
    path=pathlib.Path(spec['path']).resolve(strict=True)
    data=path.read_bytes()
    if digest(data)!=spec['sha256']:
        raise ValueError('input hash mismatch: '+str(path))
    return path,data

def elf_code(data):
    if len(data)<64:
        raise ValueError('short ELF header')
    header=struct.unpack_from('<16sHHIQQQIHHHHHH',data)
    if header[0][:6]!=b'\x7fELF\x02\x01' or header[1:3]!=(3,62):
        raise ValueError('requires little-endian x86-64 ELF PIE')
    if header[9]!=56 or not 0<header[10]<1024:
        raise ValueError('program headers')
    segments=[struct.unpack_from('<IIQQQQQQ',data,header[5]+i*56) for i in range(header[10])]
    executable=[s for s in segments if s[0]==1 and s[1]&1]
    if len(executable)!=1:
        raise ValueError('requires one RX PT_LOAD')
    s=executable[0]
    if s[1]&2 or s[5]!=s[6] or s[2]%PAGE!=s[3]%PAGE:
        raise ValueError('unsupported executable segment')
    offset=s[2]//PAGE*PAGE
    vaddr=s[3]//PAGE*PAGE
    length=(s[3]%PAGE+s[5]+PAGE-1)//PAGE*PAGE
    if not 0<length<=16*1024*1024 or offset+length>len(data):
        raise ValueError('executable extent')
    return dict(offset=offset,vaddr=vaddr,length=length,code=data[offset:offset+length])

def layouts(pfns,pages,groups,seed):
    if groups<2 or groups*pages>len(pfns) or len(set(pfns))!=len(pfns):
        raise ValueError('nonoverlapping physical sets required')
    order=sorted(range(len(pfns)),key=lambda i:pfns[i])
    result=[]
    for group in range(groups):
        first=group*(len(order)-pages)//(groups-1)
        selected=order[first:first+pages]
        shuffled=selected[:]
        random.Random(seed+group).shuffle(shuffled)
        for label,indices in [('sorted',selected),('shuffled',shuffled)]:
            result.append(dict(group=group,order=label,indices=indices,pfns=[pfns[i] for i in indices]))
    return result

def schedule(workloads,placements,arms,blocks,seed):
    if len(arms)!=3 or blocks<6 or blocks%6:
        raise ValueError('three arms and multiples of six measured blocks required')
    rng=random.Random(seed)
    permutations=list(itertools.permutations(arms))
    offsets={(w,i):rng.randrange(6) for w in range(workloads) for i in range(placements)}
    result=[]
    for block in range(blocks+2):
        cells=list(itertools.product(range(workloads),range(placements)))
        rng.shuffle(cells)
        result.append([(w,p,arm) for w,p in cells for arm in permutations[(block+offsets[(w,p)])%6]])
    return result

def entries(fd,address,pages):
    data=os.pread(fd,pages*8,address//PAGE*8)
    if len(data)!=pages*8:
        raise ValueError('short pagemap read')
    values=[x[0] for x in struct.iter_unpack('=Q',data)]
    if not all(x>>63 and x&((1<<55)-1) for x in values):
        raise ValueError('physical frame unavailable/nonresident')
    return values

def pfns(values):
    return [x&((1<<55)-1) for x in values]

def read_ready(fd,timeout):
    deadline=time.monotonic()+timeout
    data=b''
    while len(data)<32:
        remaining=deadline-time.monotonic()
        if remaining<=0 or not select.select([fd],[],[],remaining)[0]:
            raise TimeoutError('loader ready barrier')
        chunk=os.read(fd,32-len(data))
        if not chunk:
            raise ValueError('loader exited before ready barrier')
        data+=chunk
    return struct.unpack('=4Q',data)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=pathlib.Path,required=True)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--execute',action='store_true')
    parser.add_argument('--check-only',action='store_true',help='run all loader/correctness gates without warmups or measured blocks')
    args=parser.parse_args()
    if not args.execute:
        parser.error('requires --execute')
    if os.geteuid()!=0 or os.uname().machine!='x86_64' or os.sysconf('SC_PAGE_SIZE')!=PAGE:
        parser.error('requires privileged Linux/x86-64 page observation')
    config=json.loads(args.config.read_text())
    output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    output.chmod(0o755)
    output.joinpath('config.json').write_text(json.dumps(config,indent=2)+'\n')
    raw=output.joinpath('raw.jsonl').open('x')
    def emit(value):
        raw.write(json.dumps(value)+'\n');raw.flush()
    user=pwd.getpwuid(config['uid'])
    if user.pw_uid==0:
        raise ValueError('application must run as a non-root user')
    allowed=os.sched_getaffinity(0)
    cpu=config['cpu'];controller=config['controller_cpu']
    if cpu==controller or not {cpu,controller}.issubset(allowed):
        raise ValueError('CPU binding')
    os.sched_setscheduler(0,os.SCHED_OTHER,os.sched_param(0))
    os.setpriority(os.PRIO_PROCESS,0,0)
    os.sched_setaffinity(0,{controller})
    libc=ctypes.CDLL(None,use_errno=True)
    libc.mmap.argtypes=[ctypes.c_void_p,ctypes.c_size_t,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_long]
    libc.mmap.restype=ctypes.c_void_p
    for op in ['mlock','munlock','munmap']:
        getattr(libc,op).argtypes=[ctypes.c_void_p,ctypes.c_size_t]
    libc.personality.argtypes=[ctypes.c_ulong];libc.personality.restype=ctypes.c_int
    aslr=pathlib.Path('/proc/sys/kernel/randomize_va_space').read_text()
    supplementary=os.getgrouplist(user.pw_name,user.pw_gid)
    def setup():
        personality=libc.personality(0xffffffff)
        if personality<0 or libc.personality(personality|0x40000)<0:
            raise OSError(ctypes.get_errno(),'personality')
        os.sched_setscheduler(0,os.SCHED_OTHER,os.sched_param(0))
        os.setpriority(os.PRIO_PROCESS,0,0);os.sched_setaffinity(0,{cpu})
        os.setgroups(supplementary);os.setgid(user.pw_gid);os.setuid(user.pw_uid)
    pool=None;pool_map=None;pool_fd=None;self_pagemap=None
    scratch=None;status='FAILED';completed=0
    try:
        arms=config['arms']
        if len(arms)!=3 or config['repetitions'] not in [1,2,3,4]:
            raise ValueError('arm/repetition count')
        workloads=[];inputs=[]
        for spec in config['workloads']:
            workload=dict(spec)
            workload['images']={}
            for arm in arms:
                path,data=checked_file(spec['binaries'][arm]);image=elf_code(data)
                workload['images'][arm]=dict(image,path=str(path),sha256=digest(data))
                inputs.append(spec['binaries'][arm])
            _,workload['expected']=checked_file(spec['expected']);inputs.append(spec['expected'])
            workload['stdin_bytes']=None
            if spec.get('stdin'):
                _,workload['stdin_bytes']=checked_file(spec['stdin']);inputs.append(spec['stdin'])
            for dependency in spec['inputs']:
                checked_file(dependency);inputs.append(dependency)
            if any(k in spec['environment'] for k in ['LD_PRELOAD','KEEL_TEXT_POOL','KEEL_TEXT_PLAN']):
                raise ValueError('reserved environment')
            if len({image['vaddr'] for image in workload['images'].values()})!=1:
                raise ValueError('variants must have equal RX virtual offsets')
            workloads.append(workload)
        max_pages=max(image['length']//PAGE for w in workloads for image in w['images'].values())
        pool_bytes=16*1024*1024
        while pool_bytes//PAGE*4<max_pages*config['groups']*5:
            pool_bytes*=2
        if pool_bytes>64*1024*1024:
            raise ValueError('pool exceeds 64MiB bound')
        source=pathlib.Path(__file__).with_name('map_text.c')
        output.joinpath('map_text.c').write_bytes(source.read_bytes())
        output.joinpath('run.py').write_bytes(pathlib.Path(__file__).read_bytes())
        library=output/'map_text.so'
        command=[config['clang'],'-std=c11','-O2','-shared','-fPIC',str(source),'-ldl','-o',str(library)]
        build=subprocess.run(command,capture_output=True,timeout=60)
        output.joinpath('build.stdout').write_bytes(build.stdout);output.joinpath('build.stderr').write_bytes(build.stderr)
        emit(dict(kind='build',argv=command,rc=build.returncode,compiler_sha256=digest(pathlib.Path(config['clang']).resolve().read_bytes())))
        if build.returncode:
            raise ValueError('loader build failed')
        library.chmod(0o555)
        scratch=pathlib.Path(tempfile.mkdtemp(prefix='keel-fixed-backing-',dir=config['pool_directory']))
        scratch.chmod(0o755);pool=scratch/'pages.bin';plan=scratch/'plan.bin'
        os.sched_setaffinity(0,{cpu})
        fd=os.open(pool,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o444)
        try:
            if os.write(fd,b'\0'*pool_bytes)!=pool_bytes:
                raise ValueError('short pool write')
        finally:os.close(fd)
        pool_fd=os.open(pool,os.O_RDONLY)
        pool_map=libc.mmap(None,pool_bytes,1,1,pool_fd,0)
        if pool_map in [None,ctypes.c_void_p(-1).value] or libc.mlock(pool_map,pool_bytes):
            raise OSError(ctypes.get_errno(),'lock private pool')
        os.sched_setaffinity(0,{controller})
        self_pagemap=os.open('/proc/self/pagemap',os.O_RDONLY)
        physical=pfns(entries(self_pagemap,pool_map,pool_bytes//PAGE))
        placements=layouts(physical,max_pages,config['groups'],config['seed'])
        order=schedule(len(workloads),len(placements),arms,config['blocks'],config['seed'])
        protocol=dict(config=config,check_only=args.check_only,placements=placements,schedule=order,pool_pfns=physical,pool_bytes=pool_bytes,library_sha256=digest(library.read_bytes()),harness_sha256=digest(pathlib.Path(__file__).read_bytes()),helper_sha256=digest(source.read_bytes()),host=os.uname()._asdict() if hasattr(os.uname(),'_asdict') else list(os.uname()),cpuinfo=pathlib.Path('/proc/cpuinfo').read_text().split('\n\n')[0],global_aslr=aslr,estimand='wall time from release of verified loader barrier through process completion; excludes launch/loader/preparation/PFN observation',no_profiler=True)
        output.joinpath('protocol.json').write_text(json.dumps(protocol,indent=2)+'\n')
        emit(dict(kind='sealed',protocol_sha256=digest(output.joinpath('protocol.json').read_bytes())))
        deadline=time.monotonic()+config.get('budget_seconds',1800)
        def prepare(image,placement):
            if pfns(entries(self_pagemap,pool_map,pool_bytes//PAGE))!=physical:
                raise ValueError('parent physical backing moved')
            n=image['length']//PAGE;indices=placement['indices'][:n]
            fd=os.open(pool,os.O_WRONLY)
            try:
                for logical,index in enumerate(indices):
                    if os.pwrite(fd,image['code'][logical*PAGE:(logical+1)*PAGE],index*PAGE)!=PAGE:
                        raise ValueError('short code write')
            finally:os.close(fd)
            if pfns(entries(self_pagemap,pool_map,pool_bytes//PAGE))!=physical:
                raise ValueError('physical backing changed on write')
            plan.write_bytes(struct.pack('=%dQ'%(n+1),n,*indices))
            return placement['pfns'][:n]
        def invoke(w,p,arm,phase,block,rep):
            nonlocal completed
            if time.monotonic()>deadline:
                raise TimeoutError('campaign budget')
            workload=workloads[w];placement=placements[p];image=workload['images'][arm]
            wanted=prepare(image,placement)
            ready_r,ready_w=os.pipe();go_r,go_w=os.pipe()
            child=None
            before=resource.getrusage(resource.RUSAGE_CHILDREN);launch=time.monotonic_ns()
            try:
                os.dup2(ready_w,198);os.dup2(go_r,199)
                try:
                    env=dict(workload['environment'],LD_PRELOAD=str(library),KEEL_TEXT_POOL=str(pool),KEEL_TEXT_PLAN=str(plan))
                    child=subprocess.Popen(workload['argv'],executable=image['path'],cwd=workload['cwd'],env=env,stdin=subprocess.PIPE if workload['stdin_bytes'] is not None else subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,pass_fds=(198,199),preexec_fn=setup,start_new_session=True)
                finally:
                    os.close(198);os.close(199);os.close(ready_w);ready_w=-1;os.close(go_r);go_r=-1
                magic,start,length,n=read_ready(ready_r,15)
                if magic!=MAGIC or length!=image['length'] or n!=len(wanted):
                    raise ValueError('loader acknowledgement')
                if start-image['vaddr']!=config['expected_base']:
                    raise ValueError('virtual base changed')
                fd=os.open('/proc/%d/pagemap'%child.pid,os.O_RDONLY)
                try:actual_entries=entries(fd,start,n)
                finally:os.close(fd)
                if pfns(actual_entries)!=wanted:
                    raise ValueError('child code physical frame mismatch')
                observed=dict(policy=os.sched_getscheduler(child.pid),nice=os.getpriority(os.PRIO_PROCESS,child.pid),affinity=sorted(os.sched_getaffinity(child.pid)))
                if observed!=dict(policy=0,nice=0,affinity=[cpu]):
                    raise ValueError('scheduling settings changed')
                process_status=pathlib.Path('/proc/%d/status'%child.pid).read_text()
                actual_uids=[int(x) for x in next(line for line in process_status.splitlines() if line.startswith('Uid:')).split()[1:]]
                if actual_uids!=[user.pw_uid]*4:
                    raise ValueError('application credentials')
                start_time=time.monotonic_ns();os.write(go_w,b'G');os.close(go_w);go_w=-1
                stdout,stderr=child.communicate(input=workload['stdin_bytes'],timeout=30)
                wall=time.monotonic_ns()-start_time
                usage=resource.getrusage(resource.RUSAGE_CHILDREN)
                row=dict(kind='result',workload=workload['id'],w=w,placement=p,group=placement['group'],order=placement['order'],arm=arm,phase=phase,block=block,rep=rep,wall_ns=wall,instrumented_process_ns=time.monotonic_ns()-launch,cpu_s=usage.ru_utime+usage.ru_stime-before.ru_utime-before.ru_stime,rc=child.returncode,stdout_sha256=digest(stdout),stderr_sha256=digest(stderr),code_sha256=digest(image['code']),code_start=start,code_length=length,pagemap_entries=actual_entries,settings=observed,uids=actual_uids,major_faults=usage.ru_majflt-before.ru_majflt)
                emit(row)
                if child.returncode or stdout!=workload['expected'] or stderr:
                    output.joinpath('failed.stdout').write_bytes(stdout);output.joinpath('failed.stderr').write_bytes(stderr)
                    raise ValueError('application correctness')
                completed+=1
            except BaseException:
                if child is not None:
                    if child.poll() is None:
                        try:os.killpg(child.pid,signal.SIGKILL)
                        except ProcessLookupError:pass
                    try:stdout,stderr=child.communicate(timeout=5)
                    except subprocess.TimeoutExpired:raise RuntimeError('owned process failed to reap')
                    output.joinpath('failed.stdout').write_bytes(stdout);output.joinpath('failed.stderr').write_bytes(stderr)
                raise
            finally:
                for fd in [ready_r,ready_w,go_r,go_w]:
                    if fd>=0:os.close(fd)
        for w in range(len(workloads)):
            for p in range(len(placements)):
                for arm in arms:invoke(w,p,arm,'gate',-1,0)
        if not args.check_only:
            emit(dict(kind='before_timing',gates_completed=completed))
            for block,cells in enumerate(order):
                for w,p,arm in cells:
                    for rep in range(config['repetitions']):
                        invoke(w,p,arm,'warmup' if block<2 else 'measured',block,rep)
        if pfns(entries(self_pagemap,pool_map,pool_bytes//PAGE))!=physical:
            raise ValueError('final backing changed')
        for spec in inputs:checked_file(spec)
        if pathlib.Path('/proc/sys/kernel/randomize_va_space').read_text()!=aslr:
            raise ValueError('global ASLR changed externally')
        status='COMPLETE'
    except BaseException as error:
        emit(dict(kind='failure',error=repr(error),traceback=traceback.format_exc()))
        raise
    finally:
        if self_pagemap is not None:os.close(self_pagemap)
        if pool_map not in [None,ctypes.c_void_p(-1).value]:
            libc.munlock(pool_map,pool_bytes);libc.munmap(pool_map,pool_bytes)
        if pool_fd is not None:os.close(pool_fd)
        if scratch is not None:shutil.rmtree(scratch)
        emit(dict(kind='release',status=status,completed_calls=completed,owned_jobs=0,private_pool_removed=True))
        raw.close()

if __name__=='__main__':
    def interrupted(signum,frame):
        raise KeyboardInterrupt('signal '+str(signum))
    signal.signal(signal.SIGTERM,interrupted)
    main()
