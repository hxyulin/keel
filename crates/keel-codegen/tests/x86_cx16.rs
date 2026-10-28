//! Fixed-register CX16, including SysV i128 ABI and fault-time RBX unwind evidence.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::process::Command;
use std::{fs, path::PathBuf};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    callee_saved: &[3],
    ..EMPTY
};
const RBX: Pools = Pools {
    callee_saved: &[3, 12, 13, 14],
    ..EMPTY
};
fn source() -> String {
    let mut s = String::from(
        r#"
func @cas(ptr,i128,i128)->i128 {
block0(v0:ptr,v1:i128,v2:i128):
 v3 = intrinsic x86_cx16_cmpxchg16b(v0,v1,v2)
 return v3
}
func @alias(ptr,i128)->i128 {
block0(v0:ptr,v1:i128):
 v2 = intrinsic x86_cx16_cmpxchg16b(v0,v1,v1)
 return v2
}
func @dead(ptr,i128,i128) {
block0(v0:ptr,v1:i128,v2:i128):
 v3 = intrinsic x86_cx16_cmpxchg16b(v0,v1,v2)
 return
}
func @live(ptr,i128,i128,ptr,ptr,i64)->i128 {
 ss0 = stack_slot 64, align 32
block0(v0:ptr,v1:i128,v2:i128,v3:ptr,v4:ptr,v5:i64):
 v6 = stack_addr ss0
 v7 = intrinsic stack_alloc(v5)
 store v1,v6
 store v2,v7
"#,
    );
    // Keep many separately allocated scalar inputs live across the RMW.
    for i in 0..8 {
        s += &format!(
            " v{} = iconst.i64 {}\n v{} = ptradd v3,v{}\n v{} = load.i64 v{}\n",
            8 + 3 * i,
            8 * i,
            9 + 3 * i,
            8 + 3 * i,
            10 + 3 * i,
            9 + 3 * i
        );
    }
    s += " v32 = intrinsic x86_cx16_cmpxchg16b(v0,v1,v2)\n";
    for i in 0..8 {
        s += &format!(
            " v{} = ptradd v4,v{}\n store v{},v{}\n",
            33 + i,
            8 + 3 * i,
            10 + 3 * i,
            33 + i
        );
    }
    s += " v41 = load.i128 v6\n v42 = load.i128 v7\n v43 = iadd v41,v2\n v48 = iadd v43,v42\n v44 = iconst.i64 64\n v45 = ptradd v4,v44\n store v48,v45\n v46 = iconst.i64 80\n v47 = ptradd v4,v46\n store v0,v47\n return v32\n}\n";
    s
}
fn compile(
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline)?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline)?;
    assert_eq!(program.stats().text_bytes, 0);
    let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    assert_eq!(text.matches("lock cmpxchg16b").count(), 4);
    let lines: Vec<_> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.contains("lock cmpxchg16b") {
            assert!(
                lines[index + 1]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .starts_with(&["mov", "rbx,"])
            );
            assert!(
                !lines[index - 6..index + 2]
                    .iter()
                    .any(|s| { matches!(s.split_whitespace().next(), Some("push" | "pop")) })
            );
        }
    }

    assert!(
        text.lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["push", "rbx"])
    );
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with(".cfi_offset") && line.contains("3,"))
    );
    Ok((object, text))
}
fn harness() -> &'static str {
    r#"
#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(__linux__) && defined(__x86_64__)
#include <sys/mman.h>
#include <sys/wait.h>
#include <signal.h>
#include <unistd.h>
#include <pthread.h>
#include <unwind.h>
#include <cpuid.h>
typedef unsigned __int128 U;
extern U cas(U*,U,U), alias(U*,U), live(U*,U,U,uint64_t*,void*,uint64_t);
extern void dead(U*,U,U);
static U oracle(U*p,U expected,U desired) {
 uint64_t lo=expected,hi=expected>>64;
 __asm__ volatile("lock cmpxchg16b %0":"+m"(*p),"+a"(lo),"+d"(hi):"b"((uint64_t)desired),"c"((uint64_t)(desired>>64)):"memory","cc");
 return ((U)hi<<64)|lo;
}
#define CHECK(x) do{if(!(x)){fprintf(stderr,"CX16 line %d\n",__LINE__);exit(10);}}while(0)
// This wrapper supplies a known caller RBX and checks normal callee preservation.
// Its labeled return address also identifies the unwind frame without inspecting
// the OS signal register context. The unwinder must recover the caller's RBX.
extern U rbx_call(U*,U,U);
extern char rbx_return[];
__asm__(".text\n.globl rbx_call\n.type rbx_call,@function\nrbx_call:\n.cfi_startproc\n push %rbx\n.cfi_def_cfa_offset 16\n.cfi_offset 3,-16\n movabs $0x13579bdf2468ace0,%rbx\n call cas\n.globl rbx_return\nrbx_return:\n movabs $0x13579bdf2468ace0,%r11\n cmp %r11,%rbx\n jne 1f\n pop %rbx\n.cfi_def_cfa_offset 8\n.cfi_restore 3\n ret\n1: ud2\n.cfi_endproc\n.size rbx_call,.-rbx_call\n");
static _Unwind_Reason_Code trace(struct _Unwind_Context*c,void*arg){
 (void)arg;
 if(_Unwind_GetIP(c)==(uintptr_t)rbx_return){
  _exit(_Unwind_GetGR(c,3)==UINT64_C(0x13579bdf2468ace0)?0:71);
 }
 return _URC_NO_REASON;
}
static void fault_unwind(int sig){(void)sig;_Unwind_Backtrace(trace,0);_exit(72);}
static void fault(U*p,int unwind){
 pid_t child=fork();CHECK(child>=0);
 if(!child){
  if(unwind){signal(SIGSEGV,fault_unwind);signal(SIGBUS,fault_unwind);}
  rbx_call(p,0,((U)0xfedcba9876543210ULL<<64)|0xabcdefULL);_exit(73);
 }
 int status;CHECK(waitpid(child,&status,0)==child);
 if(unwind)CHECK(WIFEXITED(status)&&WEXITSTATUS(status)==0);
 else CHECK(WIFSIGNALED(status)&&(WTERMSIG(status)==SIGSEGV||WTERMSIG(status)==SIGBUS));
}
static _Alignas(16) U counter;
static void* worker(void*arg){
 (void)arg;U old=cas(&counter,0,0);
 for(unsigned i=0;i<4000;i++)for(;;){U got=cas(&counter,old,old+1);if(got==old){old++;break;}old=got;}
 return 0;
}
int main(void){
 unsigned ca,cb,cc,cd;
 if(!__get_cpuid(1,&ca,&cb,&cc,&cd)||!(cc&(1u<<13))){fputs("CX16 unavailable\n",stderr);return 77;}
 uint64_t rng=7;
 for(unsigned i=0;i<2048;i++){
  rng=rng*6364136223846793005ULL+1;U e=((U)rng<<64)|(rng^0x89abcdefULL);
  rng=rng*6364136223846793005ULL+1;U d=((U)rng<<64)|~rng;
  for(unsigned mode=0;mode<4;mode++){
   _Alignas(16) U p=mode==0?e:mode==1?e^((U)1<<127):mode==2?e^1:~e,q=p;
   U before=p,ref=oracle(&q,e,d),got=rbx_call(&p,e,d);
   CHECK(got==ref&&got==before&&p==q&&p==(before==e?d:before));
   p=before;CHECK(alias(&p,e)==before&&p==before);
   p=before;dead(&p,e,d);CHECK(p==(before==e?d:before));
   uint64_t inputs[8],out[11];for(unsigned j=0;j<8;j++)inputs[j]=rng+j*123456789;
   p=before;CHECK(live(&p,e,d,inputs,out,128)==before);CHECK(p==q);
   CHECK(!memcmp(inputs,out,64));U sum;memcpy(&sum,out+8,16);CHECK(sum==e+d+d);CHECK(out[10]==(uintptr_t)&p);
  }
 }
 long page=sysconf(_SC_PAGESIZE);CHECK(page>0);
 unsigned char*p=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);CHECK(p!=MAP_FAILED);
 CHECK(!mprotect(p+page,page,PROT_READ|PROT_WRITE));
 U*edge=(U*)(p+2*page-16);*edge=123;CHECK(cas(edge,123,456)==123&&*edge==456);
 U*start=(U*)(p+page);*start=789;CHECK(cas(start,789,987)==789&&*start==987);
 fault((U*)(p+2*page-8),0);fault((U*)(p+page+1),0);fault((U*)(p+2*page),0);
 fault((U*)(p+page+1),1);fault((U*)(p+2*page),1);
 CHECK(!mprotect(p+page,page,PROT_READ));fault(start,0);fault(start,1);
 CHECK(!munmap(p,3*page));
 pthread_t threads[4];for(unsigned i=0;i<4;i++)CHECK(!pthread_create(&threads[i],0,worker,0));
 for(unsigned i=0;i<4;i++)CHECK(!pthread_join(threads[i],0));CHECK(counter==16000);
 puts("CX16 ABI/scalar/LOCK/live-RBX/frame/fault/unwind/counter PASS");return 0;
}
#else
#error This harness requires native x86_64 Linux
#endif
"#
}
// The runner embeds the inventory digest, so changing the manifest alone cannot
// bless a partial or modified packet. Validate everything before compiling any case.
const RUNNER: &str = r#"#!/bin/sh
set -eu
exec python3 - "$0" "$@" <<'PYTHON'
import hashlib, itertools, json, os, pathlib, platform, shlex, struct, subprocess, sys
root = pathlib.Path(sys.argv[1]).resolve().parent
os.chdir(root)
def require(ok, message):
    if not ok:
        raise SystemExit("CX16 packet: " + message)
roster = [f"cx16-{s}-{a}-{p}.o" for s, a, p in itertools.product(
    ("Baseline", "Trees", "Full"), ("Fast", "Full"), ("normal", "tiny", "empty", "rbx"))]
expected = set(roster + [name[:-2] + ".s" for name in roster] + ["cx16.c", "cx16.ir"])
require(sys.argv[2:] in ([], ["--verify-only"]), "usage: run.sh [--verify-only]")
require(pathlib.Path("inventory.json").is_file(), "missing sealed inventory")
raw = pathlib.Path("inventory.json").read_bytes()
require(hashlib.sha256(raw).hexdigest() == "@INVENTORY_SHA256@", "inventory seal mismatch")
inventory = json.loads(raw)
require(isinstance(inventory, dict) and set(inventory) == expected, "inventory roster mismatch")
actual = {p.name for pattern in ("cx16-*.o", "cx16-*.s") for p in root.glob(pattern)}
require(actual == expected - {"cx16.c", "cx16.ir"}, "object/source roster mismatch (missing or unexpected case)")
for name in sorted(expected):
    path = pathlib.Path(name)
    require(path.is_file() and not path.is_symlink(), "missing/nonregular file " + name)
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == inventory[name], "hash mismatch: " + name)
    if name.endswith(".o"):
        # ELF64, little endian, current version, ET_REL, EM_X86_64; reject
        # truncated headers and bogus section tables before invoking the linker.
        require(len(data) >= 64 and data[:7] == b"\x7fELF\x02\x01\x01", "invalid ELF64: " + name)
        header = struct.unpack_from("<HHIQQQIHHHHHH", data, 16)
        require(header[0:3] == (1, 62, 1) and header[7] == 64 and header[10] == 64
                and header[11] > 0 and header[5] >= 64
                and header[5] + 64 * header[11] <= len(data), "invalid x86_64 relocatable ELF: " + name)
print("CX16 sealed packet verified: 24 ELF objects", flush=True)
if sys.argv[2:] == ["--verify-only"]:
    sys.exit(0)
require(platform.system() == "Linux" and platform.machine() == "x86_64", "requires native x86_64 Linux")
cc = shlex.split(os.environ.get("CC", "cc"))
require(bool(cc), "empty CC")
for name in roster:
    subprocess.run(cc + ["-O2", "-g", "-mcx16", "-pthread", "-fasynchronous-unwind-tables",
                         "cx16.c", name, "-o", "cx16-run"], check=True)
    subprocess.run(["./cx16-run"], check=True)
PYTHON
"#;

fn write_packet_seal(directory: &std::path::Path) {
    fs::write(directory.join("run.sh"), RUNNER).unwrap();
    common::command(
        Command::new("python3")
            .arg("-c")
            .arg(
                r#"
import hashlib, itertools, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
roster = [f"cx16-{s}-{a}-{p}.o" for s,a,p in itertools.product(
    ("Baseline", "Trees", "Full"), ("Fast", "Full"), ("normal", "tiny", "empty", "rbx"))]
names = roster + [n[:-2] + ".s" for n in roster] + ["cx16.c", "cx16.ir"]
inventory = {n: hashlib.sha256((root / n).read_bytes()).hexdigest() for n in sorted(names)}
raw = (json.dumps(inventory, indent=2, sort_keys=True) + "\n").encode()
(root / "inventory.json").write_bytes(raw)
runner = root / "run.sh"
runner.write_text(runner.read_text().replace("@INVENTORY_SHA256@", hashlib.sha256(raw).hexdigest()))
"#,
            )
            .arg(directory),
    );
}

fn seal_packet(directory: &std::path::Path) {
    write_packet_seal(directory);
    common::command(
        Command::new("sh")
            .arg(directory.join("run.sh"))
            .arg("--verify-only"),
    );
}
#[test]
fn typed_features_effects_and_foreign_rejection() {
    let id = Intrinsic::X86Cx16Cmpxchg16b;
    assert_eq!(Intrinsic::from_name("x86_cx16_cmpxchg16b"), Some(id));
    assert!(!id.info().pure);
    let inst = InstData::Intrinsic { id, args: vec![] };
    assert!(inst.is_pinned());
    assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
    for f in [Features::NONE, Features::for_level(CpuLevel::V1)] {
        assert!(
            compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f)
                .unwrap_err()
                .message
                .contains("CX16")
        );
    }
    let mut f = Features::NONE;
    f.cx16 = true;
    compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).unwrap();
    let mut f = Features::for_level(CpuLevel::V2);
    f.cx16 = false;
    assert!(compile(SelectionTier::Baseline, AllocationTier::Fast, &TINY, f).is_err());
    for (from, to) in [
        ("v0,v1,v2", "v1,v1,v2"),
        ("ptr,i128,i128", "ptr,i64,i128"),
        ("->i128", "->i64"),
    ] {
        let bad = source().replace(from, to);
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let m = keel_ir::parse(&source()).unwrap();
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
}
#[test]
fn cx16_fast_leaf_and_pool_boundary() {
    let text = source().replace(
        "func @dead(ptr,i128,i128) {",
        "func @dead(ptr,i128,i128) cc(fast) local {",
    );
    let module = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::for_level(CpuLevel::V2);
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, &EMPTY, &pipeline).unwrap();
    let dead = text
        .split(".Ldead_begin:")
        .nth(1)
        .unwrap()
        .split(".Ldead_end:")
        .next()
        .unwrap();
    assert!(
        dead.lines()
            .any(|s| s.split_whitespace().collect::<Vec<_>>() == ["push", "rbp"])
    );
    assert!(
        dead.lines()
            .any(|s| s.split_whitespace().collect::<Vec<_>>() == ["push", "rbx"])
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let reserved = Pools {
                caller_saved: &[11],
                ..EMPTY
            };
            keel_codegen::x86_64::compile_with_pipeline(&module, &reserved, &pipeline)
        }))
        .is_err()
    );
}
#[test]
fn cx16_allocation_frame_parity_export() {
    let export = std::env::var_os("KEEL_X86_CX16_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("cx16.ir"), source()).unwrap();
        fs::write(d.join("cx16.c"), harness()).unwrap();
        // Invalidate any prior seal before starting a new export.
        for name in ["run.sh", "inventory.json"] {
            let path = d.join(name);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (name, p) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &EMPTY),
                ("rbx", &RBX),
            ] {
                let (b, text) = compile(s, a, p, Features::for_level(CpuLevel::V2)).unwrap();
                if let Some(d) = &export {
                    let name = format!("cx16-{s:?}-{a:?}-{name}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
    if let Some(d) = &export {
        seal_packet(d);
    }
}

#[test]
fn runner_rejects_incomplete_and_unsealed_packets() {
    let d = common::temp("cx16-runner-negative");
    fs::create_dir_all(&d).unwrap();
    let (object, text) = compile(
        SelectionTier::Baseline,
        AllocationTier::Full,
        &EMPTY,
        Features::for_level(CpuLevel::V2),
    )
    .unwrap();
    fs::write(d.join("cx16.c"), harness()).unwrap();
    fs::write(d.join("cx16.ir"), source()).unwrap();
    for s in ["Baseline", "Trees", "Full"] {
        for a in ["Fast", "Full"] {
            for p in ["normal", "tiny", "empty", "rbx"] {
                fs::write(d.join(format!("cx16-{s}-{a}-{p}.o")), &object).unwrap();
                fs::write(d.join(format!("cx16-{s}-{a}-{p}.s")), &text).unwrap();
            }
        }
    }
    seal_packet(&d); // Positive control: the complete, sealed fixture passes.
    let reject = |reason: &str| {
        let output = Command::new("sh")
            .arg(d.join("run.sh"))
            .arg("--verify-only")
            .output()
            .unwrap();
        assert!(!output.status.success(), "runner accepted {reason}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(reason),
            "{:?}",
            output
        );
    };
    let missing = d.join("cx16-Full-Full-rbx.o");
    fs::remove_file(&missing).unwrap();
    reject("roster mismatch");
    // Exercise the actual default entry point too: it must reject the missing
    // case before the host gate or any native compiler/harness invocation.
    let output = Command::new("sh").arg(d.join("run.sh")).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("roster mismatch"));
    fs::write(&missing, &object).unwrap();
    fs::write(d.join("cx16-extra.o"), &object).unwrap();
    reject("roster mismatch");
    fs::remove_file(d.join("cx16-extra.o")).unwrap();
    fs::write(&missing, b"not ELF").unwrap();
    reject("hash mismatch");
    let manifest = fs::read(d.join("inventory.json")).unwrap();
    fs::write(d.join("inventory.json"), b"{}").unwrap();
    reject("inventory seal mismatch");
    fs::write(d.join("inventory.json"), manifest).unwrap();
    // A matching hash is insufficient: reject non-ELF even if freshly sealed.
    write_packet_seal(&d);
    reject("invalid ELF64");
    // Also reject a valid ELF header whose machine is not x86_64.
    let mut foreign = object.clone();
    foreign[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(&missing, foreign).unwrap();
    write_packet_seal(&d);
    reject("invalid x86_64 relocatable ELF");
    fs::write(&missing, &object).unwrap();
    write_packet_seal(&d);
    common::command(
        Command::new("sh")
            .arg(d.join("run.sh"))
            .arg("--verify-only"),
    );
    fs::remove_dir_all(d).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_cx16_oracles() {
    if !std::is_x86_feature_detected!("cmpxchg16b") {
        return;
    }
    let d = common::temp("cx16-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("cx16.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY, &RBX] {
                let (b, _) = compile(s, a, p, Features::for_level(CpuLevel::V2)).unwrap();
                fs::write(d.join("cx16.o"), b).unwrap();
                common::command(
                    Command::new("cc")
                        .args([
                            "-O2",
                            "-g",
                            "-mcx16",
                            "-pthread",
                            "-fasynchronous-unwind-tables",
                        ])
                        .arg(d.join("cx16.c"))
                        .arg(d.join("cx16.o"))
                        .arg("-o")
                        .arg(d.join("cx16")),
                );
                common::command(&mut Command::new(d.join("cx16")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
