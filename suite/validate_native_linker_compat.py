#!/usr/bin/env python3
"""Signed arm64 TLS/weak/debug differential probes; all artifacts stay in --out."""
import argparse
import json
from pathlib import Path
import platform
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    keel, out = args.keel.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = {'platform': platform.platform(), 'tests': {}}

    def run(*cmd):
        proc = subprocess.run(list(map(str, cmd)), capture_output=True)
        with (out / 'commands.log').open('a') as log:
            log.write(f'{cmd!r}: {proc.returncode}\n{proc.stdout.decode(errors="replace")}\n{proc.stderr.decode(errors="replace")}\n')
        if proc.returncode:
            raise RuntimeError(f'{cmd}: {proc.returncode}\n{proc.stderr.decode(errors="replace")}')
        return proc.stdout

    results['clang'] = run('clang', '--version').decode().splitlines()[0]
    results['lldb'] = run('lldb', '--version').decode().splitlines()[0]
    results['macos'] = run('sw_vers').decode()
    sdk = Path(run('xcrun', '--show-sdk-path').decode().strip())
    # Overlay only interfaces: never modify the installed SDK or suite cache.
    root = out / 'sdk'
    lib = root / 'usr/lib'
    lib.mkdir(parents=True, exist_ok=True)
    for path in (sdk / 'usr/lib').iterdir():
        target = lib / path.name
        if not target.exists():
            target.symlink_to(path)

    def compile(name, source, producer='clang', flags=()):
        src, obj = out / (name + '.c'), out / (name + '.o')
        src.write_text(source)
        if producer == 'clang':
            run('clang', *flags, '-c', src, '-o', obj)
        else:
            run(keel, 'cc', *flags, '--emit-object', src, '-o', obj)
        return obj

    def pair(name, objects, libs=(), apple_flags=(), debug=False):
        native, apple = out / name, out / (name + '.apple')
        flags = ['--dsym', str(native) + '.dSYM'] if debug else []
        run(keel, 'link', '--sysroot', root, *objects, *libs, *flags, '-o', native)
        run('clang', *objects, *libs, *apple_flags, '-o', apple)
        expected = run(apple)
        for _ in range(5):
            assert run(native) == expected, name
        for exe in [native, apple]:
            run('codesign', '--verify', '--strict', exe)
            (out / (exe.name + '.load.txt')).write_bytes(run('otool', '-l', exe))
            (out / (exe.name + '.bind.txt')).write_bytes(run('xcrun', 'llvm-objdump', '--macho', '--bind', '--weak-bind', '--exports-trie', exe))
        results['tests'][name] = {'native_launches': 5, 'apple_matches': True, 'stdout': expected.decode(), 'signed': True}
        return native

    tls = '''#include <pthread.h>
#include <stdio.h>
#include <stdint.h>
__thread int initialized = 17;
__thread int zero;
__thread long aligned __attribute__((aligned(64))) = 91;
extern int other(void);
static void *worker(void *arg) {
  int id = (int)(intptr_t)arg;
  if (initialized != 17 || zero != 0 || aligned != 91 || other() != 31) return (void*)1;
  initialized += id; zero = id; aligned += id;
  for (int i=0;i<10000;i++) if (initialized != 17+id || zero != id || aligned != 91+id) return (void*)2;
  return 0;
}
int main(void) {
  pthread_t a,b; void *ra,*rb;
  initialized=123; zero=456;
  if (pthread_create(&a,0,worker,(void*)1) || pthread_create(&b,0,worker,(void*)2)) return 3;
  pthread_join(a,&ra); pthread_join(b,&rb);
  if (ra || rb || initialized!=123 || zero!=456 || aligned!=91 || other()!=31) return 4;
  puts("TLS isolated"); return 0;
}
'''
    for producer in ['clang', 'keel']:
        main_obj = compile('tls-' + producer, tls, producer)
        other = compile('tls-other-' + producer, '__thread int more=31; __thread int morezero; int other(void){return more+morezero;}\n', producer)
        pair('tls-' + producer, [main_obj, other])
        archive = out / ('tls-' + producer + '.a')
        run('ar', 'rcs', archive, other)
        pair('tls-archive-' + producer, [main_obj, archive])

    weak_a = compile('weak-a', '__attribute__((weak)) int shared=17; __attribute__((weak)) int f(void){return shared;} int a(void){return f();}\n')
    weak_b = compile('weak-b', '__attribute__((weak)) int shared=17; __attribute__((weak)) int f(void){return shared;} int b(void){return f();}\n')
    strong = compile('strong', 'int shared=29; int f(void){return shared;}\n')
    for value, extra in [(17, []), (29, [strong])]:
        main_obj = compile('weak-main-' + str(value), f'extern int f(void),a(void),b(void); int main(void){{return f()!={value} || a()!={value} || b()!={value};}}\n')
        for order, objects in [('forward', [weak_a, weak_b, *extra]), ('reverse', [*extra, weak_b, weak_a])]:
            pair(f'weak-{value}-{order}', [main_obj, *objects])
    missing = compile('weak-missing', 'extern int absent(void) __attribute__((weak_import)); extern int puts(const char*) __attribute__((weak_import)); int main(void){return absent!=0 || puts==0;}\n')
    pair('weak-missing', [missing], apple_flags=['-Wl,-U,_absent'])

    def dylib(name, source, symbols, weak=(), tls=()):
        obj = compile(name, source)
        path = out / ('lib' + name + '.dylib')
        run('clang', '-dynamiclib', obj, '-Wl,-install_name,' + str(path), '-o', path)
        (lib / ('lib' + name + '.tbd')).write_text(f'''--- !tapi-tbd
tbd-version: 4
targets: [arm64-macos]
install-name: '{path}'
exports:
  - targets: [arm64-macos]
    symbols: {json.dumps(list(symbols))}
    weak-symbols: {json.dumps(list(weak))}
    thread-local-symbols: {json.dumps(list(tls))}
''')
        return path

    tls_lib = dylib('compattls', '__thread int imported_tls=71; int read_imported(void){return imported_tls;}\n', ['_read_imported'], tls=['_imported_tls'])
    imported = compile('tls-imported', '''#include <pthread.h>
extern __thread int imported_tls; extern int read_imported(void);
static void *worker(void *unused){if(imported_tls!=71 || read_imported()!=71)return (void*)1; imported_tls=91; return (void*)(long)(read_imported()!=91);}
int main(void){pthread_t thread; void *result; imported_tls=81; if(pthread_create(&thread,0,worker,0))return 1; pthread_join(thread,&result);return result!=0 || imported_tls!=81 || read_imported()!=81;}
''')
    native = out / 'tls-imported'
    apple = out / 'tls-imported.apple'
    run(keel, 'link', '--sysroot', root, imported, '-lcompattls', '-o', native)
    run('clang', imported, tls_lib, '-o', apple)
    for exe in [native, apple]:
        run('codesign', '--verify', '--strict', exe)
        for _ in range(5): run(exe)
    results['tests']['tls-imported'] = {'native_launches': 5, 'apple_matches': True, 'signed': True}

    init = compile('tls-initializers', '''#include <pthread.h>
__thread int initialized=17; __thread int calls;
void thread_init(void){initialized+=26; calls++;}
static void *worker(void *unused){return (void*)(long)(initialized!=43 || calls!=1 || initialized!=43);}
int main(void){pthread_t thread; void *result; if(initialized!=43 || calls!=1)return 1; initialized=99; if(pthread_create(&thread,0,worker,0))return 2;pthread_join(thread,&result);return result!=0 || initialized!=99 || calls!=1;}
''')
    init_asm = out / 'tls-init.s'
    init_ptr = out / 'tls-init.o'
    init_asm.write_text('.section __DATA,__thread_init,thread_local_init_function_pointers\n.p2align 3\n.quad _thread_init\n')
    run('clang', '-c', init_asm, '-o', init_ptr)
    rejected = subprocess.run([str(keel), 'link', '--sysroot', str(root), str(init), str(init_ptr), '-o', str(out / 'legacy-init-rejected')], capture_output=True)
    assert rejected.returncode != 0 and b'legacy TLS initializer' in rejected.stderr
    # Current Apple ld downgrades this legacy section to S_REGULAR and dyld
    # does not call it. Preserve a precise diagnostic, never silently omit it.
    apple_legacy = out / 'tls-legacy-init.apple'
    run('clang', init, init_ptr, '-o', apple_legacy)
    run('codesign', '--verify', '--strict', apple_legacy)
    legacy_result = subprocess.run([str(apple_legacy)], capture_output=True)
    assert legacy_result.returncode == 1
    (out / 'tls-legacy-init.apple.sections.txt').write_bytes(run('xcrun', 'llvm-objdump', '--macho', '--private-headers', apple_legacy))
    results['tests']['legacy-tls-initializers'] = {'precise_rejection': True, 'apple_probe_returncode': legacy_result.returncode, 'apple_does_not_run_legacy_callback': True}
    cpp = out / 'tls-dynamic.cpp'
    cpp.write_text('''#include <pthread.h>
static int make_value(){return 43;}
thread_local int value=make_value();
static void *worker(void*){if(value!=43)return (void*)1;value=91;return (void*)(long)(value!=91);}
int main(){pthread_t thread;void *result;if(value!=43)return 1;value=99;if(pthread_create(&thread,0,worker,0))return 2;pthread_join(thread,&result);return result!=0 || value!=99;}
''')
    obj = out / 'tls-dynamic.o'
    run('clang++', '-c', cpp, '-o', obj)
    pair('tls-dynamic', [obj], libs=['-lc++'])

    weak_lib = dylib('compatweak', '__attribute__((weak)) int f(void){return 17;} int call_f(void){return f();} void *get_f(void){return f;}\n', ['_call_f', '_get_f'], ['_f'])
    strong_lib = dylib('compatstrong', 'int f(void){return 29;} int force_strong(void){return 0;}\n', ['_f', '_force_strong'])
    for name, definition, libraries in [
        ('dynamic-import', '', [weak_lib]),
        ('dynamic-weak', '__attribute__((weak)) int f(void){return 23;}', [weak_lib]),
        ('dynamic-strong', 'int f(void){return 29;}', [weak_lib]),
        ('dynamic-hidden', '__attribute__((weak,visibility("hidden"))) int f(void){return 23;}', [weak_lib]),
        ('dynamic-upgrade', '', [weak_lib, strong_lib]),
        ('dynamic-weak-upgrade', '__attribute__((weak)) int f(void){return 23;}', [weak_lib, strong_lib]),
    ]:
        src = '#include <stdio.h>\nextern int f(void),call_f(void); extern void *get_f(void);\n' + definition + '\nint main(void){printf("%d %d %d\\n", f(),call_f(),get_f()==(void*)f); return 0;}\n'
        obj = compile(name, src)
        native_libs = ['-lcompatweak'] + (['-lcompatstrong'] if len(libraries) == 2 else [])
        # Apple takes actual dylibs; native consumes only the explicit TBD interfaces.
        native = out / name
        apple = out / (name + '.apple')
        run(keel, 'link', '--sysroot', root, obj, *native_libs, '-o', native)
        run('clang', obj, *libraries, '-o', apple)
        expected = run(apple)
        for _ in range(5):
            assert run(native) == expected, name
        for exe in [native, apple]: run('codesign', '--verify', '--strict', exe)
        (out / (name + '.bind.txt')).write_bytes(run('xcrun', 'llvm-objdump', '--macho', '--bind', '--weak-bind', '--exports-trie', native))
        results['tests'][name] = {'native_launches': 5, 'apple_matches': True, 'stdout': expected.decode(), 'signed': True}

    data_lib = dylib('compatdata', '__attribute__((weak)) int shared=17; int read_shared(void){return shared;} int *get_shared(void){return &shared;}\n', ['_read_shared', '_get_shared'], ['_shared'])
    for name, definition in [('import', ''), ('weak', '__attribute__((weak)) int shared=23;'), ('strong', 'int shared=29;')]:
        obj = compile('dynamic-data-' + name, '#include <stdio.h>\nextern int shared,read_shared(void);extern int *get_shared(void);\n' + definition + '\nint main(void){printf("%d %d %d\\n",shared,read_shared(),get_shared()==&shared);return 0;}\n')
        native, apple = out / ('dynamic-data-' + name), out / ('dynamic-data-' + name + '.apple')
        run(keel, 'link', '--sysroot', root, obj, '-lcompatdata', '-o', native)
        run('clang', obj, data_lib, '-o', apple)
        expected = run(apple)
        for _ in range(5): assert run(native) == expected
        for exe in [native, apple]: run('codesign', '--verify', '--strict', exe)
        results['tests'][native.name] = {'native_launches': 5, 'apple_matches': True, 'signed': True, 'stdout': expected.decode()}

    debug_source = '''static int twice(int value) {
  int local = value * 2;
  return local;
}
int helper(void) { return twice(21); }
'''
    for producer in ['clang', 'keel']:
        obj = compile('debug-' + producer, debug_source, producer, ['-g'])
        caller = compile('debug-caller-' + producer, 'extern int helper(void); int main(void){return helper()!=42;}\n', producer, ['-g'])
        archive = out / ('debug-' + producer + '.a')
        run('ar', 'rcs', archive, obj)
        mapped = out / ('debug-map-' + producer)
        run(keel, 'link', '--sysroot', root, caller, archive, '--debug-map', str(mapped) + '.objects', '-o', mapped)
        output = run('lldb', '-b', mapped, '-o', f'breakpoint set --file debug-{producer}.c --line 3', '-o', 'run', '-o', 'frame variable', '-o', 'bt', '-o', 'continue')
        (out / (mapped.name + '.lldb.txt')).write_bytes(output)
        assert b'local = 42' in output and b'value = 21' in output and b'`main' in output
        results['tests'][mapped.name] = {'lldb_line_locals_backtrace_debug_map': True}
        exe = pair('debug-' + producer, [caller, archive], debug=True)
        output = run('lldb', '-b', exe, '-o', f'breakpoint set --file debug-{producer}.c --line 3', '-o', 'run', '-o', 'frame variable', '-o', 'bt', '-o', 'continue')
        (out / (exe.name + '.lldb.txt')).write_bytes(output)
        assert b'local = 42' in output and b'value = 21' in output and b'stop reason = breakpoint' in output and b'`main' in output, output.decode()
        run('dwarfdump', '--verify', str(exe) + '.dSYM')
        # Prove the dSYM is standalone, not accidentally relying on the OSO files.
        shutil.rmtree(exe.with_suffix('.debug-objects'))
        output = run('lldb', '-b', exe, '-o', f'breakpoint set --file debug-{producer}.c --line 3', '-o', 'run', '-o', 'frame variable', '-o', 'bt', '-o', 'continue')
        assert b'local = 42' in output and b'`main' in output, output.decode()
        results['tests'][exe.name]['lldb_line_locals_backtrace_standalone_dsym'] = True

    (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
