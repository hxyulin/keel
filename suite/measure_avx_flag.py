"""Same-current-compiler SSE2 versus opt-in AVX. Native x86 Linux only.

Correctness: retained scalar oracle (IEEE bits, aliasing, alignment, tails).
Timing: independent arrays, 1027/65539/4194307 elements, nonzero tails,
all elements checked after each run, strict separate operations, no fast math.
Two warmups and eleven alternating pairs; inner CLOCK_MONOTONIC boundary.
"""
import argparse
import gzip
import json
import pathlib
import platform
import random
import re

import measure_contract_consumers as common
from measure_passes import summary

DRIVER = r'''
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <math.h>
static double now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;}
#define DECL(T,S) \
extern void add##S(T*,const T*,const T*,unsigned long); \
extern void sub##S(T*,const T*,const T*,unsigned long); \
extern void mul##S(T*,const T*,const T*,unsigned long); \
extern void div##S(T*,const T*,const T*,unsigned long); \
static int test##S(int op,unsigned long n,unsigned long reps){ \
 T *a=malloc(n*sizeof(T)),*b=malloc(n*sizeof(T)),*d=malloc(n*sizeof(T)); \
 if(!a||!b||!d)return 3; \
 for(unsigned long i=0;i<n;i++){a[i]=(T)((i%1024)+1)/(T)32;b[i]=(T)((i%127)+1)/(T)16;} \
 void(*f[])(T*,const T*,const T*,unsigned long)={add##S,sub##S,mul##S,div##S}; \
 double start=now();for(unsigned long j=0;j<reps;j++)f[op](d,a,b,n);double elapsed=now()-start; \
 double checksum=0;for(unsigned long i=0;i<n;i++){volatile T x=a[i],y=b[i];T expected=op==0?x+y:op==1?x-y:op==2?x*y:x/y;if(d[i]!=expected)return 4;checksum+=d[i];} \
 printf("%.9f %.17g\n",elapsed,checksum);free(a);free(b);free(d);return 0;}
DECL(float,ps)
DECL(double,pd)
int main(int argc,char**argv){if(argc!=5)return 2;int ty=atoi(argv[1]),op=atoi(argv[2]);unsigned long n=strtoul(argv[3],0,10),r=strtoul(argv[4],0,10);return ty?testpd(op,n,r):testps(op,n,r);}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=pathlib.Path, required=True)
    parser.add_argument("--work", type=pathlib.Path, required=True)
    args = parser.parse_args()
    assert platform.system() == "Linux" and platform.machine() == "x86_64"
    args.work.mkdir(parents=True, exist_ok=False)
    try:
        guard = args.work / "guard.rs"
        guard.write_text('fn main(){assert!(std::is_x86_feature_detected!("avx"));println!("CPU+OS AVX: true");}\n')
        common.run(["rustc", guard, "-o", args.work / "guard"])
        common.run([args.work / "guard"])
        source = args.work / "kernels.c"
        lines = []
        for ty, suffix in [("float", "ps"), ("double", "pd")]:
            for name, expr in [("add", "a[i]+b[i]"), ("sub", "a[i]-b[i]"), ("mul", "a[i]*b[i]"), ("div", "a[i]/b[i]"), ("orderedadd", "(a[i]+b[i])+b[i]"), ("muladd", "a[i]*b[i]+a[i]")]:
                lines.append(f"void {name}{suffix}({ty}*d,const {ty}*a,const {ty}*b,unsigned long n){{for(unsigned long i=0;i<n;i++)d[i]={expr};}}")
        source.write_text("\n".join(lines) + "\n")
        driver = args.work / "driver.c"
        driver.write_text(DRIVER)
        oracle = common.ROOT / "docs/packed-avx-review-evidence/adversarial.c"
        binaries = {}
        code = {}
        for config in ["sse2", "avx"]:
            flags = [] if config == "sse2" else ["--target-features", "avx"]
            asm = args.work / f"{config}.s"
            obj = asm.with_suffix(".o")
            common.run([args.compiler, "cc", "--opt-tier", "full", "--isel", "none", *flags, source, "-o", asm])
            common.run(["clang", "-c", asm, "-o", obj])
            decoded = common.text(["objdump", "-d", obj])
            (args.work / f"{config}.dump").write_text(decoded + "\n")
            for suffix in ["ps", "pd"]:
                for op in ["add", "sub", "mul", "div"]:
                    assert re.search(r"\b" + ("v" if config == "avx" else "") + op + suffix + r"\b", decoded)
            assert not re.search(r"\b[yz]mm\d|\bv?f[nm]*m?add|\bv?f[nm]*m?sub", decoded)
            if config == "sse2":
                assert not re.search(r"\t[v][a-z0-9]+\s", decoded)
            correctness = args.work / f"oracle-{config}"
            common.run(["clang", "-O2", "-fno-vectorize", "-fno-slp-vectorize", "-ffp-contract=off", oracle, obj, "-o", correctness])
            common.run([correctness])
            exe = args.work / config
            common.run(["clang", "-O2", "-fno-vectorize", "-fno-slp-vectorize", "-ffp-contract=off", driver, obj, "-o", exe])
            binaries[config] = exe
            code[config] = dict(object_sha256=common.sha(obj), object_bytes=obj.stat().st_size, executable_sha256=common.sha(exe), executable_bytes=exe.stat().st_size, sections=common.text(["size", "-A", obj]))
        rows = []
        for ty in [0, 1]:
            for op, name in enumerate(["add", "sub", "mul", "div"]):
                for n, reps in [(1027, 200000), (65539, 2000), (4194307, 24)]:
                    samples = {"before": [], "after": []}
                    checksums = set()
                    for i in range(13):
                        for config in (["sse2", "avx"] if i % 2 == 0 else ["avx", "sse2"]):
                            p, _ = common.run([binaries[config], ty, op, n, reps])
                            seconds, checksum = p.stdout.decode().split()
                            checksums.add(checksum)
                            if i >= 2:
                                samples["before" if config == "sse2" else "after"].append(float(seconds) * 1000)
                    assert len(checksums) == 1
                    row = dict(type="f32x4" if ty == 0 else "f64x2", op=name, elements=n, repetitions=reps,
                               array_bytes=3*n*(4 if ty == 0 else 8), checksum=checksums.pop(), **summary(samples, random.Random(42)))
                    rows.append(row)
                    print(row["type"], name, n, row["after_over_before"], flush=True)
        result = dict(compiler=str(args.compiler), compiler_sha256=common.sha(args.compiler), code=code, runtime=rows,
                      harness_sha256=common.sha(__file__), input_sha256={str(p): common.sha(p) for p in [source, driver, oracle, guard]},
                      method="Same current compiler, Full, --isel none, AVX flag only; strict scalar Clang driver; two warmups, 11 alternating pairs. Inner wall timer excludes allocation, initialization and correctness check; arrays are independently allocated; all lengths have tails. Alias/IEEE/alignment correctness from retained adversarial oracle first.")
        (args.work / "avx.json").write_text(json.dumps(result, indent=2) + "\n")
    finally:
        with gzip.open(args.work / "commands.jsonl.gz", "wt") as f:
            for row in common.LOG:
                f.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
