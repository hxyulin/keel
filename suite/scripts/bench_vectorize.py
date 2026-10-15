#!/usr/bin/env python3
"""Compare the same Keel optimizer with/without loop-vectorize, plus Clang.

Run on the target host: python3 suite/scripts/bench_vectorize.py --keel target/release/keel
Reports warm CLI compilation time separately from executable kernel runtime.
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import tempfile
import time

PIPELINE = "simplify-cfg,canonicalize,instcombine,simplify-cfg,gvn,dce,gcm"
KERNEL = "void map(float*d,const float*a,const float*b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i]*b[i]+a[i];}\n"
HARNESS = r"""
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
extern void map(float*,const float*,const float*,unsigned long);
int main(int argc,char**argv){
 unsigned long n=strtoul(argv[1],0,10),reps=strtoul(argv[2],0,10);
 float *d=malloc(n*4),*a=malloc(n*4),*b=malloc(n*4);
 if(!a||!b||!d)return 1;
 for(unsigned long i=0;i<n;i++){a[i]=(float)(i%31);b[i]=(float)(i%7);}
 for(unsigned i=0;i<10;i++)map(d,a,b,n);
 struct timespec s,e;clock_gettime(CLOCK_MONOTONIC,&s);
 for(unsigned long i=0;i<reps;i++)map(d,a,b,n);
 clock_gettime(CLOCK_MONOTONIC,&e);
 for(unsigned long i=0;i<n;i++)if(d[i]!=a[i]*b[i]+a[i])return 2;
 printf("%.9f\n",(e.tv_sec-s.tv_sec)+(e.tv_nsec-s.tv_nsec)*1e-9);
 free(a);free(b);free(d);return 0;
}
"""

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", default="target/release/keel")
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--count", type=int, default=32768)
    parser.add_argument("--iterations", type=int, default=10000)
    parser.add_argument("--output")
    args = parser.parse_args()
    if min(args.samples, args.count, args.iterations) <= 0:
        parser.error("samples, count and iterations must be positive")
    keel = str(pathlib.Path(args.keel).resolve())
    with tempfile.TemporaryDirectory(prefix="keel-vectorize-bench-") as temporary:
        root = pathlib.Path(temporary)
        (root / "kernel.c").write_text(KERNEL)
        (root / "harness.c").write_text(HARNESS)
        commands = {
            "scalar": [keel, "cc", "-O", "-p", PIPELINE, "--emit-object", "kernel.c", "-o", "scalar.o"],
            "vector": [keel, "cc", "-O", "--emit-object", "kernel.c", "-o", "vector.o"],
            "clang": ["clang", "-O2", "-ffp-contract=off", "-c", "kernel.c", "-o", "clang.o"],
        }
        compile_samples = {name: [] for name in commands}
        runtime_samples = {name: [] for name in commands}
        for command in commands.values():
            subprocess.run(command, cwd=root, check=True, capture_output=True)
        for sample in range(args.samples):
            names = list(commands)
            names = names[sample % 3:] + names[:sample % 3]
            for name in names:
                start = time.perf_counter()
                subprocess.run(commands[name], cwd=root, check=True, capture_output=True)
                compile_samples[name].append((time.perf_counter() - start) * 1000)
        for name in commands:
            subprocess.run(["clang", "-O2", "-ffp-contract=off", "harness.c", f"{name}.o", "-o", name], cwd=root, check=True, capture_output=True)
        for sample in range(args.samples):
            names = list(commands)
            names = names[sample % 3:] + names[:sample % 3]
            for name in names:
                result = subprocess.run([str(root / name), str(args.count), str(args.iterations)], check=True, capture_output=True, text=True)
                runtime_samples[name].append(float(result.stdout))
        report = {
            "kernel": KERNEL.strip(), "elements": args.count, "iterations": args.iterations,
            "samples": args.samples, "compile_cli_ms": compile_samples,
            "runtime_seconds": runtime_samples,
            "compile_cli_median_ms": {name: statistics.median(v) for name, v in compile_samples.items()},
            "runtime_median_seconds": {name: statistics.median(v) for name, v in runtime_samples.items()},
            "keel_vector_speedup_over_scalar": statistics.median(runtime_samples["scalar"]) / statistics.median(runtime_samples["vector"]),
            "note": "One contiguous float map microbenchmark; timings are not whole-application speedups. Compilation includes process startup and preprocessing.",
        }
        output = json.dumps(report, indent=2) + "\n"
        print(output, end="")
        if args.output:
            pathlib.Path(args.output).write_text(output)

if __name__ == "__main__":
    main()
