#!/usr/bin/env python3
"""Validate explicit installed non-PIE CRT inputs against GNU ld on Linux.

Concrete CRT/DSO/archive paths are selected from the installed Clang toolchain.
No startup adapter, input stripping, compiler-driver compatibility shim or
fallback is used. Run compiler/native workloads under the caller's host lock.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--prepare-only', action='store_true')
    mode.add_argument('--verify-only', action='store_true')
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('run on x86-64 Linux')
    if not (args.prepare_only or args.verify_only or args.keel):
        parser.error('--keel is required outside prepare/verify mode')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    records = []

    def run(argv, native=False):
        result = subprocess.run([str(a) for a in argv], capture_output=True, timeout=120,
                                env=dict(os.environ, PATH='/nonexistent') if native else None)
        records.append(dict(argv=[str(a) for a in argv], native=native, rc=result.returncode,
                            stdout=result.stdout.decode(errors='replace'), stderr=result.stderr.decode(errors='replace')))
        (out/('verify-commands.json' if args.verify_only else 'commands.json')).write_text(json.dumps(records,indent=2)+'\n')
        return result

    def checked(argv, native=False):
        r=run(argv,native)
        if r.returncode:
            raise RuntimeError((argv,r.returncode,r.stderr))
        return r

    def installed(name):
        path=Path(checked(['clang','-print-file-name='+name]).stdout.decode().strip()).resolve()
        if not path.is_file():
            raise RuntimeError('Installed toolchain file missing: '+name)
        data=path.read_bytes()
        (out/name).write_bytes(data)
        return dict(source=str(path),sha256=hashlib.sha256(data).hexdigest(),bytes=len(data))

    if not args.verify_only:
        provenance={name:installed(name) for name in ['crt1.o','crti.o','crtbegin.o','crtend.o','crtn.o','libc.so.6','libgcc.a']}
        (out/'installed-inputs.json').write_text(json.dumps(provenance,indent=2)+'\n')
        sources={
            'hello.c': '#include <stdio.h>\nint main(int argc,char **argv){puts("hello crt");return argc==2&&argv[0]&&argv[1][0]==\'x\'?42:1;}\n',
            'divide.c': 'extern int puts(const char*); __int128 divide(__int128 x,__int128 y){return x/y;} int main(){puts("builtin");return divide(84,2)==42?42:1;}\n',
            'dynamic.c': 'extern int puts(const char*); extern const struct entry {long tag; unsigned long value;} _DYNAMIC[]; int main(){puts("dynamic");return _DYNAMIC[0].tag==1?42:1;}\n',
            'init-lite.cc': 'extern "C" int puts(const char*) noexcept; struct A {A();~A();}; A::A(){puts("ctor");} A::~A(){puts("dtor");} A a; int main(){puts("main");return 42;}\n',
            'callbacks.c': 'extern long write(int,const void*,unsigned long); static int calls; void _ITM_registerTMCloneTable(void *p,unsigned long n){(void)p;(void)n;++calls;write(1,"BAD register\\n",13);} void _ITM_deregisterTMCloneTable(void *p){(void)p;++calls;write(1,"BAD deregister\\n",15);} int clone_callbacks(void){return calls;}\n',
            'clone.c': 'extern int puts(const char*); int clone_callbacks(void); int main(){puts("empty clones");return clone_callbacks()==0?42:1;}\n',
            'template.cc': 'template<class T> __attribute__((noinline)) T twice(T x){return x+x;} int main(){return twice(21)==42?42:1;}\n',
        }
        for name,text in sources.items():
            (out/name).write_text(text)
        checked(['clang','-shared','-fPIC','-O2',out/'callbacks.c','-Wl,-soname,libkeel-crt-callbacks.so','-o',out/'libkeel-crt-callbacks.so'])
        cases=[]
        for source,stdout,extra in [('hello.c','hello crt\n',[]),('divide.c','builtin\n',['libgcc.a']),('dynamic.c','dynamic\n',[]),('init-lite.cc','ctor\nmain\ndtor\n',[]),('clone.c','empty clones\n',['libkeel-crt-callbacks.so'])]:
            compiler='clang++' if source.endswith('.cc') else 'clang'
            for level in ['O0','O2']:
                name=source.split('.')[0]+'-'+level
                checked([compiler,'-'+level,'-c',out/source,'-o',out/(name+'.o')])
                inputs=['crt1.o','crti.o','crtbegin.o',name+'.o',*extra,'libc.so.6','crtend.o','crtn.o']
                case=dict(name=name,inputs=inputs,stdout=stdout,status=42,argv=['x'])
                cases.append(case)
                checked(['ld','--eh-frame-hdr','--dynamic-linker','/lib64/ld-linux-x86-64.so.2','-rpath','$ORIGIN',*[out/n for n in inputs],'-o',out/(name+'.gnu')])
                r=run([out/(name+'.gnu'),'x'])
                assert (r.returncode,r.stdout,r.stderr)==(42,stdout.encode(),b''),(name,r)
        (out/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
        # Negative fixtures retain their unsupported contracts unchanged.
        checked(['clang++','-O0','-c',out/'template.cc','-o',out/'template.o'])
        checked(['clang','-O2','-fcf-protection=full','-c',out/'hello.c','-o',out/'hello-cet.o'])
        (out/'extra-sframe.s').write_text('.text\n.globl extra_frame\n.type extra_frame,@function\nextra_frame:\n.cfi_startproc\nret\n.cfi_endproc\n.section .note.GNU-stack,"",@progbits\n')
        checked(['as','--64','--gsframe',out/'extra-sframe.s','-o',out/'extra-sframe.o'])
        hashes={n:hashlib.sha256((out/n).read_bytes()).hexdigest() for n in sorted({n for c in cases for n in c['inputs']}|{'template.o','hello-cet.o','extra-sframe.o'})}
        (out/'input-hashes.json').write_text(json.dumps(hashes,indent=2)+'\n')
        if args.prepare_only:
            print('Prepared 10 actual CRT cases and intact negative fixtures')
            return
    cases=json.loads((out/'cases.json').read_text())
    hashes=json.loads((out/'input-hashes.json').read_text())
    for n,h in hashes.items():
        assert hashlib.sha256((out/n).read_bytes()).hexdigest()==h,n
    for case in cases:
        native=out/(case['name']+'.keel')
        if not args.verify_only:
            checked([args.keel.resolve(),'link','--target','x86_64-unknown-linux-gnu','--rpath','$ORIGIN',*[out/n for n in case['inputs']],'-o',native],True)
        for suffix in ['keel','gnu']:
            for _ in range(3):
                r=run([out/(case['name']+'.'+suffix),*case['argv']], True)
                assert (r.returncode,r.stdout,r.stderr)==(case['status'],case['stdout'].encode(),b''),(case['name'],suffix,r)
        header=checked(['readelf','-lWS',native]).stdout.decode()
        assert 'GNU_PROPERTY' in header and 'SFRAME' in header and 'GNU_EH_FRAME' in header,header
        (out/(case['name']+'.headers.txt')).write_text(header)
        (out/(case['name']+'.notes.txt')).write_bytes(checked(['readelf','-nW',native]).stdout)
        (out/(case['name']+'.sframe.txt')).write_bytes(checked(['readelf','--sframe',native]).stdout)
    if not args.verify_only:
        for name,user,extra,expected in [('template','template.o',[],'COMDAT'),('cet','hello-cet.o',[],'IBT'),('two-sframe','hello-O0.o',['extra-sframe.o'],'SFrame')]:
            dest=out/(name+'.rejected');dest.write_bytes(b'preserved previous output')
            inputs=['crt1.o','crti.o','crtbegin.o',user,*extra,'libc.so.6','crtend.o','crtn.o']
            r=run([args.keel.resolve(),'link','--target','x86_64-unknown-linux-gnu',*[out/n for n in inputs],'-o',dest],True)
            assert r.returncode and expected.lower() in r.stderr.decode().lower(),(name,r)
            assert dest.read_bytes()==b'preserved previous output',name
    for n,h in hashes.items():
        assert hashlib.sha256((out/n).read_bytes()).hexdigest()==h,n
    (out/'report.json').write_text(json.dumps(dict(cases=cases,result='passed',installed_crt=True,full_driver=False,links_executed=not args.verify_only,negative_checks_executed=not args.verify_only,executions_per_tool_per_case=3),indent=2)+'\n')
    print('10 installed-CRT cases passed native/GNU checks (three executions each)')


if __name__ == '__main__':
    main()
