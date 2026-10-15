"""Compiler-only inline/cleanup and bounded-scan ablations on unchanged apps.

Prepared sources are read-only. Build all configurations before timing; compare
outputs against Clang O2 and run application correctness fixtures in each build.
"""
import argparse
import concurrent.futures
import json
import pathlib
import platform
import random
import re
import shutil
import time

from compare_app_clang import measure, compare
from profile_app_loops import APPS, invocation, run, sha, text_bytes

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = 'simplify-cfg,canonicalize,instcombine,simplify-cfg,gvn,dce,gcm'
INLINE = 'inline,simplify-cfg,canonicalize,instcombine,simplify-cfg,gvn,stack-forward,canonicalize,instcombine,dce,gcm'
PIPELINES = {'keel_base': BASE, 'keel_vector': BASE+',loop-vectorize',
             'keel_inline': INLINE, 'keel_combined': INLINE+',loop-vectorize'}
SCAN = '''unsigned long prefix(const unsigned char *p,unsigned long n){const unsigned char*q=p;while(q<p+n&&*q<128)q++;return q-p;}'''
HARNESS = r'''
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
extern unsigned long prefix(const unsigned char*,unsigned long);
static double now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;}
int main(int argc,char**argv){
 unsigned long n=strtoul(argv[1],0,10),stop=strtoul(argv[2],0,10),repeat=64000000/(n+1)+10000;
 unsigned char*p=malloc(n+1);if(!p)return 2;memset(p,127,n+1);if(stop<n)p[stop]=255;
 if(prefix(p,n)!=stop)return 3;
 unsigned long sum=0;double t=now();for(unsigned long i=0;i<repeat;i++)sum+=prefix(p,n);t=now()-t;
 if(sum!=repeat*stop)return 4;printf("%.9f\n",t);free(p);return 0;
}'''


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compiler',type=pathlib.Path,required=True)
    p.add_argument('--prepared',type=pathlib.Path,default=ROOT/'target/suite')
    p.add_argument('--work-dir',type=pathlib.Path,required=True)
    p.add_argument('--rounds',type=int,default=7)
    p.add_argument('--jobs',type=int,default=4)
    p.add_argument('--cpu',type=int,default=0)
    p.add_argument('--reuse-builds',action='store_true')
    p.add_argument('--build-only',action='store_true')
    args=p.parse_args()
    if args.rounds<5 or args.jobs<1:p.error('use at least five rounds and one build worker')
    args.compiler=args.compiler.resolve();args.prepared=args.prepared.resolve()
    work=args.work_dir.resolve()
    if work==args.prepared or args.prepared in work.parents:p.error('work directory must be outside prepared sources')
    work.mkdir(parents=True,exist_ok=True);output=work/'report.json'
    save=lambda:output.write_text(json.dumps(report,indent=2)+'\n')
    report={'host':platform.platform(),'compiler_sha256':sha(args.compiler),'script_sha256':sha(__file__),
            'measurement_helper_sha256':sha(ROOT/'suite/compare_app_clang.py'),
            'pipelines':PIPELINES,'clang_flags':['-O2','-ffp-contract=off'],
            'clang_version':run(['clang','--version']).stdout.decode(),
            'method':'Original prepared inputs, identical immutable compiler for all Keel builds. Inline variants include stack-forward and post-inline scalar cleanup. Two warmups then rotated/reversed configuration order; seven rounds by default, paired bootstrap intervals (2000 resamples). Linux timed commands pin the requested CPU. All builds and correctness fixtures finish before timing. Apps measure whole CLI duration; scan control measures self-checking native harness inner duration. No source expansion or diagnostic division rewrites.',
            'rounds':args.rounds,'pinned_cpu':args.cpu if platform.system()=='Linux' else None,'apps':{},'scan':{}}
    if args.reuse_builds:
        report=json.loads(output.read_text())
        assert report['compiler_sha256']==sha(args.compiler)
        for d in [*report['apps'].values(),report['scan']]:
            for name,binary in d['binaries'].items():assert sha(binary)==d['binary_sha256'][name]
    else:
        for app in APPS:
            sources=sorted((args.prepared/app/'pre').glob('*.i'));assert sources,app
            detail={'source_sha256':{str(s):sha(s) for s in sources},'binaries':{},'build_seconds':{},'code':{},'checks':{}}
            for name in [*PIPELINES,'clang_o2']:
                directory=work/app/name;directory.mkdir(parents=True,exist_ok=True)
                started=time.perf_counter()
                flags=['-p',PIPELINES[name]] if name in PIPELINES else ['-O2','-ffp-contract=off']
                def unit(source):
                    obj=directory/(source.stem+'.o')
                    command=[args.compiler,'cc',*flags,'--emit-object',source,'-o',obj] if name in PIPELINES else ['clang',*flags,'-c',source,'-o',obj]
                    run(command,timeout=600);return obj
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:objects=list(pool.map(unit,sources))
                binary=directory/APPS[app][0];run(['clang',*objects,'-o',binary,'-lm','-lpthread'])
                detail['build_seconds'][name]=time.perf_counter()-started
                detail['binaries'][name]=str(binary);detail['code'][name]={'text_bytes':len(text_bytes(binary))}
                if app=='quickjs' and name in PIPELINES:
                    ir=run([args.compiler,'cc',*flags,'--emit-ir',args.prepared/app/'pre/quickjs.i'],timeout=600).stdout.decode()
                    (directory/'quickjs.keel').write_text(ir)
                    detail['code'][name].update({'quickjs_ir_sha256':sha(directory/'quickjs.keel'),
                        'helper_calls':{s:len(re.findall(r'call @'+s+r'\(',ir)) for s in ['JS_NewInt32','JS_NewBool','JS_DupValue']},
                        'scan_functions':[f.split('(')[0] for f in ir.split('func @')[1:] if 'load.i64x2' in f and 'vextract' in f]})
                checks=[]
                if app=='quickjs':
                    tests=directory/'tests';shutil.copytree(args.prepared/app/'quickjs-2024-01-13/tests',tests,dirs_exist_ok=True)
                    for test,extra in [('test_closure',[]),('test_language',[]),('test_builtin',[]),('test_loop',[]),('test_std',[]),('test_worker',[]),('test_op_overloading',['--bignum']),('test_bignum',['--bignum']),('test_qjscalc',['--qjscalc'])]:
                        result=run([binary,*extra,tests/(test+'.js')],cwd=directory,timeout=300)
                        (directory/(test+'.log')).write_bytes(result.stdout+result.stderr);checks.append(test)
                elif app=='lua':
                    tests=directory/'tests';shutil.copytree(args.prepared/app/'lua-5.4.7-tests',tests,dirs_exist_ok=True)
                    result=run([binary,'-e','_U=true','all.lua'],cwd=tests,timeout=300)
                    (directory/'correctness.log').write_bytes(result.stdout+result.stderr);checks.append('Lua portable suite')
                else:
                    answer=run([binary,':memory:'],input=(ROOT/'suite/sqlite/queries.sql').read_bytes()).stdout
                    (directory/'correctness.log').write_bytes(answer)
                    if name!='keel_base':assert answer==(work/app/'keel_base/correctness.log').read_bytes()
                    checks.append('queries.sql matches base')
                detail['checks'][name]=checks
                print('built and checked',app,name,flush=True)
            detail['binary_sha256']={n:sha(b) for n,b in detail['binaries'].items()}
            report['apps'][app]=detail;save()
        (work/'scan.c').write_text(SCAN);(work/'scan-harness.c').write_text(HARNESS)
        detail={'binaries':{},'source_sha256':sha(work/'scan.c'),'harness_sha256':sha(work/'scan-harness.c')}
        for name in [*PIPELINES,'clang_o2']:
            obj=work/(name+'-scan.o');binary=work/(name+'-scan')
            if name in PIPELINES:run([args.compiler,'cc','-p',PIPELINES[name],'--emit-object',work/'scan.c','-o',obj])
            else:run(['clang','-O2','-ffp-contract=off','-c',work/'scan.c','-o',obj])
            run(['clang','-O2',work/'scan-harness.c',obj,'-o',binary]);detail['binaries'][name]=str(binary)
        detail['binary_sha256']={n:sha(b) for n,b in detail['binaries'].items()};report['scan']=detail;save()
    if args.build_only:return
    def pinned(cmd):return ['taskset','-c',str(args.cpu),*map(str,cmd)] if platform.system()=='Linux' else cmd
    def contrasts(row):
        s=row['samples_ms']
        row['feature_comparisons']={a+'_over_'+b:compare(s,a,b) for a,b in [('keel_vector','keel_base'),('keel_inline','keel_base'),('keel_combined','keel_vector'),('keel_combined','keel_inline')]}
        return row
    for app,detail in report['apps'].items():
        detail['runtime']={}
        for bench in APPS[app][1]:
            fixture=ROOT/'suite'/app/('bench.sql' if app=='sqlite' else 'bench.lua' if app=='lua' else 'bench.js')
            _,stdin=invocation(app,detail['binaries']['clang_o2'],fixture,bench)
            def command(name):return pinned(invocation(app,detail['binaries'][name],fixture,bench)[0])
            detail['runtime'][bench]=contrasts(measure(args,detail['binaries'],command,stdin))
            print('timed',app,bench,detail['runtime'][bench]['median_ms'],flush=True);save()
    report['scan']['runtime']={}
    for n,stop in [(0,0),(8,8),(16,16),(64,64),(4096,4096),(4096,0),(4096,7),(4096,127)]:
        def command(name):return pinned([report['scan']['binaries'][name],str(n),str(stop)])
        key=f'length{n}-prefix{stop}'
        report['scan']['runtime'][key]=contrasts(measure(args,report['scan']['binaries'],command,kernel=True))
        print('scan',key,report['scan']['runtime'][key]['median_ms'],flush=True);save()
    assert report['compiler_sha256']==sha(args.compiler)
    for app,d in report['apps'].items():
        assert all(sha(path)==digest for path,digest in d['source_sha256'].items())
    report['complete']=True;save()

if __name__=='__main__':main()
