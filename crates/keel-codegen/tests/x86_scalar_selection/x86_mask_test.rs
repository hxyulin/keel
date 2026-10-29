//! Literal i32 AND/zero tests: register-only, ZF-only, allocation-visible leaves.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, InstructionSelector, SelectionTier,
    regalloc::{AllocationTier, Constraint, OperandKind, Pools, ValuePart},
    select::{Selection, Target},
    x86_features::FeatureSelector,
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[7],
    ..EMPTY
};
const MASKS: [i32; 7] = [0, -1, i32::MIN, 0x55555555, -1431655766, 15, 0x12345678];
fn source(name: &str, mask: i32, cc: &str, consumer: &str, live: bool) -> String {
    let tail = match consumer {
        "bool" => "v6 = uext.i32 v5\n jump block3(v6)".to_string(),
        "select" => "v6 = select v5, v1, v2\n jump block3(v6)".to_string(),
        "branch" => "brif v5, block1, block2\nblock1:\n jump block3(v1)\nblock2:\n jump block3(v2)"
            .to_string(),
        _ => unreachable!(),
    };
    format!(
        "func @{name}(i32, i32, i32) -> i32 cc(c) {{
block0(v0: i32, v1: i32, v2: i32):
 v3 = iconst.i32 {mask}
 v4 = and v0, v3
 v9 = iconst.i32 0
 v5 = icmp.{cc} v4, v9
 {tail}
block3(v7: i32):
 {}
}}\n",
        if live {
            "v8 = xor v7, v0\n return v8"
        } else {
            "return v7"
        }
    )
}
fn compile(
    src: &str,
    s: &dyn InstructionSelector,
    a: AllocationTier,
    pools: &Pools,
    features: Features,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    compile_module(&m, s, a, pools, features)
}
fn compile_module(
    m: &keel_ir::Module,
    s: &dyn InstructionSelector,
    a: AllocationTier,
    pools: &Pools,
    features: Features,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    let selector = FeatureSelector { inner: s, features };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &p).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &p).unwrap();
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(m, pools, &p, &mut |p| {
        chunks.items.extend(p.items)
    })
    .unwrap();
    for p in [&program, &chunks] {
        assert_eq!(p.stats().text_bytes, 0);
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        obj,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    assert_eq!(
        obj,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
        "{text}"
    );
    (text, obj)
}
fn tests(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| {
            l.starts_with("\ttest\t")
                && !l.contains("ptr")
                && l.rsplit_once(", ")
                    .is_some_and(|(_, rhs)| rhs.parse::<i64>().is_ok())
        })
        .collect()
}
#[test]
fn masks_consumers_live_sources_and_all_pipelines() {
    for mask in MASKS {
        for cc in ["eq", "ne"] {
            for consumer in ["bool", "branch", "select"] {
                for live in [false, true] {
                    let src = source("f", mask, cc, consumer, live);
                    for s in [
                        SelectionTier::Baseline,
                        SelectionTier::Trees,
                        SelectionTier::Full,
                    ] {
                        for a in [AllocationTier::Fast, AllocationTier::Full] {
                            for pools in [&EMPTY, &ONE, &keel_codegen::x86_64::REGISTERS] {
                                for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
                                    let (text, _) =
                                        compile(&src, &s, a, pools, Features::for_level(level));
                                    assert_eq!(tests(&text).len(), 1, "{s:?} {a:?}: {text}");
                                    assert!(!text.contains("\tand\t"), "{text}");
                                    let test = tests(&text)[0];
                                    assert!(
                                        test.contains('e') || test.contains("d,"),
                                        "i32 register: {test}"
                                    );
                                    if consumer == "select" {
                                        assert!(text.contains("\tcmov"), "{text}");
                                    }
                                    if consumer != "bool" {
                                        assert!(!text.contains("\tset"), "{text}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
struct Constrained(bool);
impl InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "mask-constraint-negative"
    }
    fn select(
        &self,
        f: &keel_ir::Function,
        _: Target,
        _: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> Selection {
        let mut plan = Selection::default();
        let inst = f
            .layout_insts()
            .map(|(_, i)| i)
            .find(|&i| {
                if self.0 {
                    matches!(f.inst_data(i), keel_ir::InstData::Icmp { .. })
                } else {
                    matches!(
                        f.inst_data(i),
                        keel_ir::InstData::Binary {
                            op: keel_ir::BinaryOp::And,
                            ..
                        }
                    )
                }
            })
            .unwrap();
        plan.set_constraint(
            inst,
            ValuePart::low(f.first_result(inst)),
            OperandKind::Def,
            Constraint::Any,
        );
        plan
    }
}
#[test]
fn exclusions_keep_and_and_no_immediate_test() {
    let src = source("f", 15, "ne", "bool", false);
    let negatives = [
        src.replace("icmp.ne v4, v9", "icmp.ne v4, v3"),
        src.replace("icmp.ne v4, v9", "icmp.ne v9, v4"),
        src.replace("icmp.ne", "icmp.slt"),
        src.replace("icmp.ne", "icmp.sle"),
        src.replace("icmp.ne", "icmp.sgt"),
        src.replace("icmp.ne", "icmp.sge"),
        src.replace("icmp.ne", "icmp.ult"),
        src.replace("icmp.ne", "icmp.ule"),
        src.replace("icmp.ne", "icmp.uge"),
        src.replace("icmp.ne", "icmp.ugt"),
        src.replace("and v0, v3", "and v0, v1"),
        src.replace("i32", "i64"),
        src.replace("v6 = uext.i32 v5", "v10 = uext.i32 v5\n v6 = xor v10, v4"),
        src.replace("v9 = iconst", "v11 = iadd v1, v2\n v9 = iconst"),
        src.replace("v9 = iconst", "fence seq_cst\n v9 = iconst"),
        src.replace("v9 = iconst", "intrinsic x86_pause()\n v9 = iconst"),
        src.replace(
            "v9 = iconst",
            "v11 = intrinsic x86_rflags_read()\n v9 = iconst",
        ),
        src.replace("block0(", "ss0 = stack_slot 4, align 4\nblock0(")
            .replace("v3 = iconst", "v12 = stack_addr ss0\n v3 = iconst")
            .replace("v9 = iconst", "store v0, v12\n v9 = iconst"),
        src.replace("block0(", "ss0 = stack_slot 4, align 4\nblock0(")
            .replace("v3 = iconst", "v12 = stack_addr ss0\n v3 = iconst")
            .replace("v9 = iconst", "v11 = load.i32 v12\n v9 = iconst"),
        format!(
            "decl @barrier()\n{}",
            src.replace("v9 = iconst", "call @barrier()\n v9 = iconst")
        ),
        src.replace("v9 = iconst", "jump block4\nblock4:\n v9 = iconst"),
    ];
    for src in negatives {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let (text, _) = compile(
                &src,
                &s,
                AllocationTier::Full,
                &EMPTY,
                Features::for_level(CpuLevel::V1),
            );
            assert!(text.contains("\tand\t"), "{text}");
            assert!(tests(&text).is_empty(), "{text}");
        }
    }
    for s in [Constrained(false), Constrained(true)] {
        let (text, _) = compile(
            &src,
            &s,
            AllocationTier::Full,
            &EMPTY,
            Features::for_level(CpuLevel::V1),
        );
        assert!(text.contains("\tand\t"), "{text}");
        assert!(tests(&text).is_empty(), "{text}");
    }
    // Source locations/debug variables conservatively retain materialization.
    let src = format!(
        "file f0 = \"mask.c\"\n{}",
        src.replace("and v0, v3", "and v0, v3 !f0:2:1")
    );
    let (text, _) = compile(
        &src,
        &SelectionTier::Full,
        AllocationTier::Full,
        &EMPTY,
        Features::for_level(CpuLevel::V1),
    );
    assert!(tests(&text).is_empty(), "{text}");
}
#[test]
fn commuted_mask_and_shared_condition_keep_exact_register_recipe() {
    let src = source("f", 15, "eq", "select", true)
        .replace("and v0, v3", "and v3, v0")
        .replace(
            "v6 = select v5, v1, v2",
            "v6 = select v5, v1, v2\n v10 = uext.i32 v5",
        )
        .replace("v8 = xor v7, v0", "v11 = xor v7, v10\n v8 = xor v11, v0");
    for a in [AllocationTier::Fast, AllocationTier::Full] {
        let (text, _) = compile(
            &src,
            &SelectionTier::Full,
            a,
            &EMPTY,
            Features::for_level(CpuLevel::V1),
        );
        assert_eq!(tests(&text).len(), 1, "{text}");
        assert!(text.contains("\tsete\t"), "{text}");
    }
}

fn signed_byte_source(name: &str, cc: &str, consumer: &str) -> String {
    source(name, i32::MIN, cc, consumer, true)
        .replace("(i32, i32, i32)", "(ptr, i32, i32)")
        .replace("v0: i32", "v0: ptr")
        .replace(
            "v3 = iconst",
            "v20 = load.i8 align 1 v0\n v21 = sext.i32 v20\n v3 = iconst",
        )
        .replace("and v0, v3", "and v21, v3")
        .replace("xor v7, v0", "xor v7, v21")
}

#[test]
fn signed_byte_cover_and_mask_test_compose_at_all_roots() {
    for consumer in ["bool", "branch", "select"] {
        let src = signed_byte_source("f", "eq", consumer);
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&EMPTY, &ONE, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) = compile(&src, &s, a, pools, Features::for_level(CpuLevel::V1));
                    assert_eq!(tests(&text).len(), 1, "{text}");
                    assert!(!text.contains("\tand\t"), "{text}");
                    assert_eq!(
                        text.lines()
                            .filter(|l| l.starts_with("\tmovsx\t") && l.contains("byte ptr"))
                            .count(),
                        1,
                        "{text}"
                    );
                }
            }
        }
    }
}

// Real SSA aliases differ from two operands assigned the same physical register.
fn alias_source(m: &mut keel_ir::Module) {
    for f in &mut m.functions {
        let Some((inst, value)) = f.layout_insts().find_map(|(_, i)| {
            matches!(
                f.inst_data(i),
                keel_ir::InstData::Iconst { imm: 314159, .. }
            )
            .then(|| (i, f.first_result(i)))
        }) else {
            continue;
        };
        let source = f.block_params(f.entry_block().unwrap())[0];
        f.replace_all_uses(value, source);
        f.remove_inst(inst);
    }
}

#[test]
fn resolved_source_aliases_transfer_to_each_allocation_root() {
    for consumer in ["bool", "branch", "select"] {
        let src = source("f", i32::MIN, "eq", consumer, true)
            .replace("v3 = iconst", "v20 = iconst.i32 314159\n v3 = iconst")
            .replace("and v0, v3", "and v3, v20");
        let mut m = keel_ir::parse(&src).unwrap();
        alias_source(&mut m);
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&EMPTY, &ONE, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) =
                        compile_module(&m, &s, a, pools, Features::for_level(CpuLevel::V1));
                    assert_eq!(tests(&text).len(), 1, "{text}");
                    assert!(!text.contains("\tand\t"), "{text}");
                    assert_eq!(text.contains("\tset"), consumer == "bool", "{text}");
                }
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_gcc_clang_oracles_calls_aliases_and_guarded_exact_loads() {
    use std::{fs, process::Command};
    let dir = std::path::PathBuf::from(format!(
        "/tmp/keel-mask-pilot3-native-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::from("decl @clobber()\n");
    let mut c = String::from(
        r#"
#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <sys/prctl.h>
#include <signal.h>
#include <unistd.h>
#include <assert.h>
__attribute__((noinline)) void clobber(void) {
 __asm__ volatile("mov $85, %%eax; mov $85, %%ecx; mov $85, %%edx; mov $85, %%esi; mov $85, %%edi; mov $85, %%r8d; mov $85, %%r9d; mov $85, %%r10d; mov $85, %%r11d" ::: "rax","rcx","rdx","rsi","rdi","r8","r9","r10","r11","cc","memory");
}
static uint32_t state=0x87654321;
static uint32_t random32(void) { state ^= state<<13; state ^= state>>17; state ^= state<<5; return state; }
"#,
    );
    let mut checks = String::new();
    for (mi, mask) in MASKS.into_iter().enumerate() {
        for cc in ["eq", "ne"] {
            for consumer in ["bool", "branch", "select"] {
                for live in [false, true] {
                    let name = format!("f_{mi}_{cc}_{consumer}_{}", u8::from(live));
                    ir += &source(&name, mask, cc, consumer, live)
                        .replace("v3 = iconst", "call @clobber()\n v3 = iconst");
                    c += &format!("extern uint32_t {name}(uint32_t,uint32_t,uint32_t);\n");
                    checks += &format!(
                        "{{ uint32_t b=((x & UINT32_C({})) {} 0); uint32_t e={}; {} assert({name}(x,y,z)==e); }}\n",
                        mask as u32,
                        if cc == "eq" { "==" } else { "!=" },
                        if consumer == "bool" { "b" } else { "b?y:z" },
                        if live { "e ^= x;" } else { "" }
                    );
                }
            }
        }
    }
    for consumer in ["bool", "branch", "select"] {
        let name = format!("resolved_alias_{consumer}");
        ir += &source(&name, i32::MIN, "eq", consumer, true)
            .replace("v3 = iconst", "v20 = iconst.i32 314159\n v3 = iconst")
            .replace("and v0, v3", "and v3, v20");
        c += &format!("extern uint32_t {name}(uint32_t,uint32_t,uint32_t);\n");
        checks += &format!(
            "assert({name}(x,y,z)==(({} )^x));\n",
            if consumer == "bool" {
                "!(x&UINT32_C(0x80000000))"
            } else {
                "(x&UINT32_C(0x80000000))?z:y"
            }
        );
    }
    for cc in ["eq", "ne"] {
        for consumer in ["bool", "branch", "select"] {
            let name = format!("combined_{cc}_{consumer}");
            ir += &signed_byte_source(&name, cc, consumer);
            c += &format!("extern uint32_t {name}(void*,uint32_t,uint32_t);\n");
            checks += &format!(
                "{{ int8_t byte=(int8_t)x; uint32_t sx=(uint32_t)(int32_t)byte; uint32_t b=((sx&UINT32_C(0x80000000)) {} 0); assert({name}(&byte,y,z)==(({})^sx)); }}\n",
                if cc == "eq" { "==" } else { "!=" },
                if consumer == "bool" { "b" } else { "b?y:z" }
            );
        }
    }
    for (name, kernel, expected) in [
        (
            "alias_arm",
            source("alias_arm", 15, "ne", "select", true)
                .replace("select v5, v1, v2", "select v5, v0, v2"),
            "((b?x:z)^x)",
        ),
        (
            "shared_cond",
            source("shared_cond", 15, "ne", "select", true)
                .replace(
                    "v6 = select v5, v1, v2",
                    "v6 = select v5, v1, v2\n v10 = uext.i32 v5",
                )
                .replace("v8 = xor v7, v0", "v11 = xor v7, v10\n v8 = xor v11, v0"),
            "((b?y:z)^b^x)",
        ),
        (
            "capture_call",
            source("capture_call", 15, "ne", "select", true)
                .replace("v6 = select", "call @clobber()\n v6 = select"),
            "((b?y:z)^x)",
        ),
    ] {
        ir += &kernel;
        c += &format!("extern uint32_t {name}(uint32_t,uint32_t,uint32_t);\n");
        checks += &format!("{{ uint32_t b=!!(x&15); assert({name}(x,y,z)=={expected}); }}\n");
    }
    for (tag, ty, ext, width) in [
        ("byte", "i8", "uext.i32", 1),
        ("signed_byte", "i8", "sext.i32", 1),
        ("word", "i16", "uext.i32", 2),
        ("dword", "i32", "", 4),
    ] {
        ir += &format!(
            "func @mem_{tag}(ptr,ptr,ptr) -> i32 cc(c) {{
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = iconst.i32 1
 store volatile v3, v2
 v4 = load.{ty} align 1 v0
 {}
 call @clobber()
 v6 = iconst.i32 -1431655766
 v7 = and {}, v6
 v8 = iconst.i32 0
 v9 = icmp.ne v7, v8
 v10 = uext.i32 v9
 store align 1 {}, v1
 v11 = iconst.i32 2
 store volatile v11, v2
 v12 = xor v10, {}
 return v12
}}\n",
            if ext.is_empty() {
                String::new()
            } else {
                format!("v5 = {ext} v4")
            },
            if ext.is_empty() { "v4" } else { "v5" },
            if ext.is_empty() { "v4" } else { "v5" },
            if ext.is_empty() { "v4" } else { "v5" }
        );
        c += &format!("extern uint32_t mem_{tag}(void*,void*,void*);\n");
        checks += &format!(
            r#"{{ unsigned char buf[24], out[24]; uint32_t marker=0, v=0, got; memset(buf,0x5b,sizeof buf); memset(out,0xa7,sizeof out); memcpy(buf+5,&x,{width}); memcpy(&v,buf+5,{width}); {} assert(mem_{tag}(buf+5,out+3,&marker)==(v ^ !!(v & UINT32_C(0xaaaaaaaa)))); memcpy(&got,out+3,4); assert(got==v && marker==2); for(unsigned j=0;j<24;j++) if(j<3||j>=7) assert(out[j]==0xa7); marker=0; assert(mem_{tag}(buf+5,buf+5,&marker)==(v ^ !!(v & UINT32_C(0xaaaaaaaa)))); }}
"#,
            if tag == "signed_byte" {
                "v=(uint32_t)(int32_t)(int8_t)v;"
            } else {
                ""
            }
        );
    }
    // RLIMIT_CORE does not stop a core_pattern pipe such as Ubuntu's apport,
    // which takes about a second per deliberate fault. Non-dumpable processes
    // skip it.
    c += "int main(void) { struct rlimit lim={0,0}; assert(!setrlimit(RLIMIT_CORE,&lim)); assert(!prctl(PR_SET_DUMPABLE,0)); uint32_t edge[]={0,1,0x7fffffff,0x80000000,0xffffffff,0x55555555,0xaaaaaaaa}; for(unsigned i=0;i<263;i++){uint32_t x=i<7?edge[i]:random32(),y=random32(),z=random32();\n";
    c += &checks;
    c += "}\nlong n=sysconf(_SC_PAGESIZE); unsigned char *m=mmap(0,3*n,PROT_NONE,MAP_SHARED|MAP_ANONYMOUS,-1,0); assert(m!=MAP_FAILED); assert(!mprotect(m+n,n,PROT_READ|PROT_WRITE)); uint32_t *marker=mmap(0,n,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0); assert(marker!=MAP_FAILED);\n";
    for (tag, width) in [("byte", 1), ("signed_byte", 1), ("word", 2), ("dword", 4)] {
        c += &format!(
            r#"for(unsigned edge=0;edge<4;edge++) {{ unsigned char *p=edge==0?m+n:edge==1?m+2*n-{width}:edge==2?m+n-1:m+2*n-{width}+1; *marker=0; uint32_t out=0;
if(edge<2) {{ memset(p,0x83,{width}); mem_{tag}(p,&out,marker); assert(*marker==2); }} else {{ pid_t pid=fork(); assert(pid>=0); if(!pid) {{ mem_{tag}(p,&out,marker); _exit(0); }} int s; assert(waitpid(pid,&s,0)==pid); assert(WIFSIGNALED(s)&&WTERMSIG(s)==SIGSEGV); assert(*marker==1); }} }}
"#
        );
    }
    c += "assert(!munmap(m,3*n)); assert(!munmap(marker,n)); return 0; }\n";
    fs::write(dir.join("oracle.c"), &c).unwrap();
    for cc in ["gcc", "clang"] {
        let out = Command::new(cc)
            .args(["-O2", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join(format!("{cc}.o")))
            .output()
            .unwrap();
        assert!(out.status.success(), "{cc}: {out:?}");
    }
    let mut module = keel_ir::parse(&ir).unwrap();
    alias_source(&mut module);
    fs::write(dir.join("kernels.ir"), module.to_string()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (pi, pools) in [&EMPTY, &ONE, &keel_codegen::x86_64::REGISTERS]
                .into_iter()
                .enumerate()
            {
                for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
                    let (text, obj) =
                        compile_module(&module, &s, a, pools, Features::for_level(level));
                    assert_eq!(tests(&text).len(), 100, "{text}");
                    let tag = format!("{s:?}-{a:?}-{pi}-{level:?}");
                    fs::write(dir.join(format!("{tag}.o")), obj).unwrap();
                    fs::write(dir.join(format!("{tag}.s")), text).unwrap();
                    for cc in ["gcc", "clang"] {
                        let out = Command::new(cc)
                            .arg(dir.join(format!("{cc}.o")))
                            .arg(dir.join(format!("{tag}.o")))
                            .arg("-o")
                            .arg(dir.join("oracle"))
                            .output()
                            .unwrap();
                        assert!(out.status.success(), "{tag} {cc}: {out:?}");
                        let out = Command::new(dir.join("oracle")).output().unwrap();
                        assert!(out.status.success(), "{tag} {cc}: {out:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn aliased_select_arm_nonadjacent_consumer_and_constant_source() {
    for src in [
        source("f", 15, "ne", "select", true).replace("select v5, v1, v2", "select v5, v0, v2"),
        source("f", 15, "eq", "select", true)
            .replace("v6 = select", "v11 = iadd v1, v2\n v6 = select"),
        source("f", 15, "eq", "branch", false)
            .replace(
                "v3 = iconst.i32 15",
                "v11 = iconst.i32 0\n v3 = iconst.i32 15",
            )
            .replace("and v0, v3", "and v11, v3")
            .replace("icmp.eq v4, v9", "icmp.eq v4, v11"),
    ] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&EMPTY, &ONE, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) = compile(&src, &s, a, pools, Features::for_level(CpuLevel::V1));
                    assert_eq!(tests(&text).len(), 1, "{text}");
                }
            }
        }
    }
    for ty in ["i8", "i16"] {
        let src = source("f", 15, "eq", "bool", false).replace("i32", ty);
        let src = if ty == "i8" {
            src.replace("v6 = uext.i8 v5\n jump block3(v6)", "jump block3(v5)")
        } else {
            src
        };
        let src = src
            .replace(&format!("-> {ty}"), "-> i32")
            .replace("return v7", "v12 = uext.i32 v7\n return v12");
        let (text, _) = compile(
            &src,
            &SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY,
            Features::for_level(CpuLevel::V1),
        );
        assert!(tests(&text).is_empty(), "{text}");
        assert!(text.contains("\tand\t"), "{text}");
    }
    // ARM selection/emission still materializes its original AND/CMP recipe.
    let m = keel_ir::parse(&source("arm", 15, "ne", "bool", true)).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let text = keel_codegen::aarch64::compile(&m).unwrap();
    assert!(text.contains("\tand\t"), "{text}");
}
