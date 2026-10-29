//! Fixed-register queries return explicit SSA values; hardware fences are not C11 fence aliases.
use crate::common;
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
use keel_ir::{EntityRef, Value};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const SOURCE: &str = r#"
func @query(i32,i32)->i128 {
block0(v0:i32,v1:i32):
 v2 = intrinsic x86_cpuid(v0,v1)
 return v2
}
func @query_live(ptr,i32,i32,i64)->i128 {
block0(v0:ptr,v1:i32,v2:i32,v3:i64):
 v4 = intrinsic x86_cpuid(v1,v2)
 store align 1 v3,v0
 return v4
}
func @unused_query(i32,i32) {
block0(v0:i32,v1:i32):
 v2 = intrinsic x86_cpuid(v0,v1)
 return
}
func @clock()->i64 {
block0:
 intrinsic x86_lfence()
 v0 = intrinsic x86_rdtsc()
 intrinsic x86_lfence()
 return v0
}
func @controls() {
block0:
 intrinsic x86_lfence()
 intrinsic x86_sfence()
 intrinsic x86_mfence()
 intrinsic x86_pause()
 return
}
"#;
struct ForcedRbx(AllocationTier);
impl RegisterAllocator for ForcedRbx {
    fn name(&self) -> &'static str {
        "cpu-control-live-rbx"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let allspill = AllocationInput {
            func: input.func,
            pools: &EMPTY,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        };
        let mut a = self.0.allocate(&allspill);
        if input.func.name == "query_live" {
            a.locs[0][Value::new(3).index()] = Some(Loc::Reg(3));
            if !a.callee_saved_used.contains(&3) {
                a.callee_saved_used.push(3);
            }
        }
        a
    }
}
fn compile(s: SelectionTier, a: &dyn RegisterAllocator, p: &Pools) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(SOURCE).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline).unwrap();
    let bytes =
        keel_asm::x86::assemble_program_object_with_features(&program, Features::NONE).unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_object_with_features(&text, Features::NONE).unwrap()
    );
    for (op, n) in [
        ("cpuid", 3),
        ("rdtsc", 1),
        ("lfence", 3),
        ("sfence", 1),
        ("mfence", 1),
        ("pause", 1),
    ] {
        assert_eq!(text.lines().filter(|l| l.trim() == op).count(), n, "{op}");
    }
    (bytes, text)
}
const C: &str = r#"
typedef unsigned int u32;typedef unsigned long long u64;typedef __uint128_t u128;
extern u128 query(u32,u32);extern u128 query_live(void*,u32,u32,u64);extern void unused_query(u32,u32);extern u64 clock(void);extern void controls(void);extern int puts(const char*);
static u128 reference(u32 leaf,u32 sub){u32 a,b,c,d;__asm__ volatile("cpuid":"=a"(a),"=b"(b),"=c"(c),"=d"(d):"0"(leaf),"2"(sub):"memory");return (u128)a|((u128)b<<32)|((u128)c<<64)|((u128)d<<96);}
static u64 stamp(void){u32 a,d;__asm__ volatile("lfence\n\trdtsc\n\tlfence":"=a"(a),"=d"(d)::"memory");return (u64)a|((u64)d<<32);}
extern int sched_getaffinity(int,__SIZE_TYPE__,void*);extern int sched_setaffinity(int,__SIZE_TYPE__,const void*);
static int pin_cpu(void){unsigned char allowed[128]={0},one[128]={0};if(sched_getaffinity(0,sizeof allowed,allowed))return 1;for(unsigned i=0;i<sizeof allowed;i++)if(allowed[i]){one[i]=(unsigned char)(allowed[i]&-(int)allowed[i]);return sched_setaffinity(0,sizeof one,one);}return 1;}
int main(void){if(pin_cpu())return 80;const u32 leaves[]={0,7,0x80000000};for(unsigned i=0;i<1024;i++){u32 leaf=leaves[i%3],sub=i%4;u64 output=0,live=0x1234567812345678ULL^((u64)i<<39)^i;u128 expected=reference(leaf,sub);if(query(leaf,sub)!=expected)return 10;if(query_live(&output,leaf,sub,live)!=expected||output!=live)return 11;unused_query(leaf,sub);u64 before=stamp(),t=clock(),after=stamp();if(t<before||t>after)return 12;controls();}puts("PASS CPU query results live RBX timestamps and controls");return 0;}
"#;
#[test]
fn queries_hardware_controls_and_live_registers() {
    let m = keel_ir::parse(SOURCE).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            compile(s, &a, &keel_codegen::x86_64::REGISTERS);
            compile(s, &a, &EMPTY);
            let (_, text) = compile(s, &ForcedRbx(a), &keel_codegen::x86_64::REGISTERS);
            assert!(text.contains("rbx"));
        }
    }
    let d = common::temp("cpu-control-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), C).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_cpu_queries_live_rbx_timestamps_and_fences() {
    let d = common::temp("cpu-control-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), C).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            let forced = ForcedRbx(a);
            for (allocator, pool) in [
                (
                    &a as &dyn RegisterAllocator,
                    &keel_codegen::x86_64::REGISTERS,
                ),
                (&a as &dyn RegisterAllocator, &EMPTY),
                (
                    &forced as &dyn RegisterAllocator,
                    &keel_codegen::x86_64::REGISTERS,
                ),
            ] {
                let (obj, _) = compile(s, allocator, pool);
                fs::write(d.join("test.o"), obj).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-march=x86-64"])
                        .arg(d.join("test.c"))
                        .arg(d.join("test.o"))
                        .arg("-o")
                        .arg(d.join("test")),
                );
                common::command(&mut Command::new(d.join("test")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
