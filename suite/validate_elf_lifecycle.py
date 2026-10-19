#!/usr/bin/env python3
"""Validate executable lifecycle dispatch against GNU ld on x86-64 Linux.

Uses a small explicit __libc_start_main adapter to isolate lifecycle support.
This is not a claim of installed CRT or compiler-driver compatibility.
All compiler/link/runtime commands run under the caller's workload lock.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keel', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--verify-only', action='store_true')
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('run on x86-64 Linux')
    if not (args.prepare_only or args.verify_only or args.keel):
        parser.error('--keel is required outside prepare/verify mode')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    records = []

    def run(argv, empty_path=False):
        result = subprocess.run([str(a) for a in argv], capture_output=True,
                                timeout=60, env=dict(os.environ, PATH='/nonexistent') if empty_path else None)
        records.append(dict(argv=[str(a) for a in argv], rc=result.returncode,
                            stdout=result.stdout.decode(errors='replace'), stderr=result.stderr.decode(errors='replace')))
        (out / ('verify-commands.json' if args.verify_only else 'commands.json')).write_text(json.dumps(records, indent=2)+'\n')
        return result

    def checked(argv, empty_path=False):
        r = run(argv, empty_path)
        if r.returncode:
            raise RuntimeError((argv, r.returncode, r.stderr))
        return r

    if not args.verify_only:
        (out/'start.s').write_text('''.text
.globl _start
_start:
 xor %ebp,%ebp
 mov %rdx,%r9
 pop %rsi
 mov %rsp,%rdx
 and $-16,%rsp
 push %rax
 push %rsp
 xor %r8d,%r8d
 xor %ecx,%ecx
 lea main(%rip),%rdi
 call *__libc_start_main@GOTPCREL(%rip)
 ud2
.section .note.GNU-stack,"",@progbits
''')
        (out/'head.s').write_text('''.section .rodata
init_text: .ascii "I"
fini_text: .ascii "F"
.section .init,"ax",@progbits
.globl _init
.hidden _init
_init:
 push %rax
 mov $1,%edi
 lea init_text(%rip),%rsi
 mov $1,%edx
 call write@PLT
.section .fini,"ax",@progbits
.globl _fini
.hidden _fini
_fini:
 push %rax
 mov $1,%edi
 lea fini_text(%rip),%rsi
 mov $1,%edx
 call write@PLT
.section .note.GNU-stack,"",@progbits
''')
        (out/'tail.s').write_text('''.text
.byte 0xcc,0xcc,0xcc
.section .init,"ax",@progbits
.p2align 4
 pop %rax
 ret
.section .fini,"ax",@progbits
.p2align 4
 pop %rax
 ret
.section .note.GNU-stack,"",@progbits
''')
        (out/'main.c').write_text('''extern long write(int,const void*,unsigned long);
static int ready;
static void pre(void){write(1,"P",1);}
__attribute__((section(".preinit_array"),used)) static void (*pre_entry)(void)=pre;
__attribute__((constructor(200))) static void second(void){ready=ready*10+2;write(1,"2",1);}
__attribute__((constructor(101))) static void first(void){ready=ready*10+1;write(1,"1",1);}
__attribute__((destructor(101))) static void last(void){write(1,"a",1);}
__attribute__((destructor(200))) static void before_last(void){write(1,"b",1);}
int main(int argc,char **argv){write(1,"M",1);return ready==12&&argc==2&&argv[1][0]=='x'?42:1;}
''')
        for name in ['start','head','tail']:
            checked(['clang','-c',out/(name+'.s'),'-o',out/(name+'.o')])
        libc = next(p for p in [Path('/usr/lib/libc.so.6'),Path('/lib/x86_64-linux-gnu/libc.so.6')] if p.is_file())
        # Retain the actual runtime interface for cross-link validation too.
        (out/'libc.so.6').write_bytes(libc.read_bytes())
        manifest = []
        for opt in ['O0','O2']:
            obj=out/('main-'+opt+'.o')
            checked(['clang','-'+opt,'-fPIC','-c',out/'main.c','-o',obj])
            names=['start.o','head.o',obj.name,'tail.o','libc.so.6']
            manifest.append(dict(name=opt,inputs=names,expected='PI12MbaF',status=42))
            checked(['ld','--eh-frame-hdr','--dynamic-linker','/lib64/ld-linux-x86-64.so.2',*[out/n for n in names],'-o',out/(opt+'.gnu')])
        (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
        if args.prepare_only:
            print('Prepared two lifecycle cases and GNU reference outputs')
            return
    manifest=json.loads((out/'manifest.json').read_text())
    for case in manifest:
        native=out/(case['name']+'.keel')
        if not args.verify_only:
            checked([args.keel.resolve(),'link','--target','x86_64-unknown-linux-gnu',*[out/n for n in case['inputs']],'-o',native],True)
        for suffix in ['keel','gnu']:
            exe=out/(case['name']+'.'+suffix)
            for _ in range(3):
                r=run([exe,'x'])
                assert (r.returncode,r.stdout,r.stderr)==(42,b'PI12MbaF',b''),(exe,r)
        # Keep all section bytes and relocations intact; neutralize only one
        # dispatch tag. Missing dispatch must change observable lifecycle work.
        for tag, expected, status in [(12,b'P12MbaF',42),(13,b'PI12Mba',42),
                                      (25,b'PIMbaF',1),(26,b'PI12MF',42),
                                      (32,b'I12MbaF',42)]:
            data=bytearray(native.read_bytes())
            phoff=int.from_bytes(data[32:40],'little')
            phnum=int.from_bytes(data[56:58],'little')
            found=False
            for i in range(phnum):
                ph=phoff+i*56
                if int.from_bytes(data[ph:ph+4],'little')!=2:
                    continue
                offset=int.from_bytes(data[ph+8:ph+16],'little')
                size=int.from_bytes(data[ph+32:ph+40],'little')
                for at in range(offset,offset+size,16):
                    if int.from_bytes(data[at:at+8],'little')==tag:
                        data[at:at+8]=(21).to_bytes(8,'little') # DT_DEBUG
                        data[at+8:at+16]=bytes(8)
                        found=True
                        break
            assert found,tag
            control=out/(case['name']+'.without-'+str(tag))
            control.write_bytes(data)
            control.chmod(0o755)
            r=run([control,'x'])
            assert (r.returncode,r.stdout,r.stderr)==(status,expected,b''),(control,r)
        dump=checked(['readelf','-dW',native]).stdout.decode()
        for tag in ['PREINIT_ARRAY','INIT_ARRAY','FINI_ARRAY','(INIT)','(FINI)']:
            assert tag in dump,(tag,dump)
        (out/(case['name']+'.dynamic.txt')).write_text(dump)
    (out/'report.json').write_text(json.dumps(dict(cases=manifest,result='passed',custom_startup=True),indent=2)+'\n')
    print('Two lifecycle cases passed native/GNU comparisons, three executions each')


if __name__ == '__main__':
    main()
