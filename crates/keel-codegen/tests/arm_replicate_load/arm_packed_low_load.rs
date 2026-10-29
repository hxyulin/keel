//! Exact low-half vector loads retain the original access and selection contracts.
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::new();
    for from in [8, 16, 32] {
        let to = from * 2;
        for op in ["swiden_low", "uwiden_low"] {
            s += &format!(
                "func @{op}{from}(ptr,ptr) {{
block0(v0: ptr,v1: ptr):
 v2 = iconst.i64 8
 v3 = ptradd v0,v2
 v4 = load.i64 v3
 v5 = vsplat.i64x2 v4
 v6 = bitcast.i{from}x{lanes} v5
 v7 = {op}.i{to}x{wide_lanes} v6
 store v7,v1
 return
}}\n",
                lanes = 128 / from,
                wide_lanes = 128 / to
            );
        }
    }
    s
}
fn compile(src: &str, sel: SelectionTier, alloc: AllocationTier, spill: bool) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let p = CodegenPipeline {
        selector: &sel,
        allocator: &alloc,
    };
    let pools = if spill {
        &EMPTY
    } else {
        &keel_codegen::aarch64::REGISTERS
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &p).unwrap();
    let program = keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &p).unwrap();
    let obj = assemble_program_object(&program, TargetFeatures::default()).unwrap();
    assert_eq!(assemble_object(&text).unwrap(), obj);
    (text, obj)
}
#[test]
fn exact_loads_and_shared_volatile_rejections() {
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                let text = compile(&source(), sel, alloc, spill).0;
                assert_eq!(text.matches("ldr\td").count(), 6, "{text}");
                assert!(!text.contains("ld1r\t"), "{text}");
                for src in [
                    source().replace("load.i64 ", "load.i64 volatile "),
                    source().replace(" store v7,v1", " store v5,v1\n store v7,v1"),
                    source().replace(" v5 = vsplat", " store v1,v1\n v5 = vsplat"),
                ] {
                    assert!(!compile(&src, sel, alloc, spill).0.contains("ldr\td"));
                }
            }
        }
    }
}
#[test]
fn constraints_and_debug_boundaries_are_preserved() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Selector(usize);
    impl InstructionSelector for Selector {
        fn name(&self) -> &'static str {
            "low-load-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            let inst = f.layout_insts().nth(2 + self.0).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(inst).for_each_operand(|v| args.push(v));
            s.set_constraint(
                inst,
                ValuePart::low(f.resolve(args[0])),
                OperandKind::Use,
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..4 {
        let m = keel_ir::parse(&source()).unwrap();
        let selector = Selector(at);
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let text =
            keel_codegen::aarch64::compile_with_pipeline(&m, &keel_codegen::aarch64::REGISTERS, &p)
                .unwrap();
        assert!(!text.contains("ldr\td"), "{text}");
    }
    for at in 0..4 {
        let mut m = keel_ir::parse(&source()).unwrap();
        m.files.push("load.c".into());
        for f in &mut m.functions {
            let chain: Vec<_> = f.layout_insts().skip(2).take(4).map(|(_, i)| i).collect();
            for (index, i) in chain.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: if index == at { 11 } else { 10 },
                        col: 1,
                        scope: None,
                    }),
                );
            }
        }
        assert!(
            !keel_codegen::aarch64::compile(&m)
                .unwrap()
                .contains("ldr\td")
        );
    }
}
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_guarded_unaligned_and_alias_oracles() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-low-load-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut checks = String::new();
    for from in [8, 16, 32] {
        let to = from * 2;
        for (op, scalar) in [
            ("swiden_low", format!("int{from}_t")),
            ("uwiden_low", format!("uint{from}_t")),
        ] {
            c += &format!("extern void {op}{from}(const void*,void*);\n");
            checks += &format!(
                "for(unsigned pattern=0;pattern<257;pattern++)for(unsigned offset=0;offset<3;offset++){{ unsigned char *src=end-8-offset, original[8],out[18],expected[16]; for(unsigned i=0;i<8;i++)src[i]=(unsigned char)(pattern+i*31);memcpy(original,src,8);memset(out,0xa5,18);for(unsigned i=0;i<{lanes};i++){{{scalar} v;memcpy(&v,original+i*sizeof v,sizeof v);uint{to}_t e=(uint{to}_t)v;memcpy(expected+i*sizeof e,&e,sizeof e);}} {op}{from}(src-8,out+1);if(out[0]!=0xa5||out[17]!=0xa5||memcmp(out+1,expected,16))return 2;unsigned char alias[32];memcpy(alias+8,original,8);{op}{from}(alias,alias+8);if(memcmp(alias+8,expected,16))return 3;}}",
                lanes = 128 / to
            );
        }
    }
    c += &format!(
        "int main(void){{size_t n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED||mprotect(p+n,n,PROT_NONE))return 1;unsigned char*end=p+n;{checks}return 0;}}"
    );
    fs::write(dir.join("oracle.c"), c).unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                fs::write(dir.join("test.o"), compile(&source(), sel, alloc, spill).1).unwrap();
                let build = Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(
                    build.status.success(),
                    "{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                let result = Command::new(dir.join("test")).output().unwrap();
                assert!(
                    result.status.success(),
                    "{sel:?} {alloc:?} {spill}: {:?}",
                    result.status
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
