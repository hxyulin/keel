#!/usr/bin/env python3
"""Check GNU readers, source debugging and unwinding through native Keel code."""
import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--compiler', type=Path, required=True)
parser.add_argument('--work-dir', type=Path, required=True)
args = parser.parse_args()
compiler = args.compiler.resolve()
work = args.work_dir.resolve()
work.mkdir(parents=True, exist_ok=True)


def run(argv, log):
    result = subprocess.run([str(a) for a in argv], cwd=work, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=30)
    (work / log).write_bytes(result.stdout)
    if result.returncode:
        raise RuntimeError(f'{argv}: exit {result.returncode}: {result.stdout.decode()}')
    return result.stdout.decode()


(work / 'probe.c').write_text('''int probe(int x) {
    int answer = x + 1;
    return answer;
}
int main(void) { return probe(41) != 42; }
''')
run([compiler, 'cc', '-g', '--emit-object', 'probe.c', '-o', 'probe.o'], 'compile.log')
run(['clang', 'probe.o', '-o', 'probe'], 'link.log')
run(['./probe'], 'execution.log')
for mode in ['--debug-dump=info', '--debug-dump=decodedline', '--debug-dump=frames', '-Wr', '-Ws']:
    text = run(['readelf', mode, 'probe.o'], 'readelf-' + mode.replace('/', '_').replace('=', '-') + '.log')
    if 'Warning:' in text or 'Error:' in text:
        raise RuntimeError(text)
gdb = run(['gdb', '-batch', '-ex', 'break probe.c:3', '-ex', 'run', '-ex', 'info locals',
           '-ex', 'bt', './probe'], 'gdb.log')
if 'answer = 42' not in gdb or 'probe.c:3' not in gdb or 'main' not in gdb:
    raise RuntimeError(gdb)
(work / 'frame.keel').write_text('''decl @walk() -> i32
func @keel_frame() -> i32 {
block0:
 v0 = call @walk()
 v1 = iconst.i32 1
 v2 = iadd v0, v1
 return v2
}
''')
(work / 'driver.c').write_text('''#include <unwind.h>
#include <stdio.h>
static _Unwind_Reason_Code count(struct _Unwind_Context *ctx, void *p) {
    (void)ctx; ++*(int *)p; return _URC_NO_REASON;
}
int walk(void) { int n = 0; _Unwind_Backtrace(count, &n); return n; }
extern int keel_frame(void);
int main(void) { int n = keel_frame(); printf("frames: %d\\n", n); return n < 5; }
''')
run([compiler, 'compile', '--emit-object', 'frame.keel', '-o', 'frame.o'], 'frame-compile.log')
run(['clang', '-g', '-fno-omit-frame-pointer', 'driver.c', 'frame.o', '-o', 'unwind'], 'frame-link.log')
run(['./unwind'], 'unwind.log')
print('PASS ELF/DWARF readers, GDB source line/locals/backtrace, libgcc unwind through Keel frame')
