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
    for bits in [32, 64] {
        for order in [0, 1] {
            let compare = " v3 = icmp.slt v0,v2\n".to_string();
            let negate = " v4 = ineg v0\n".to_string();
            s += &format!(
                "func @abs{bits}_{order}(i{bits},ptr)->i{bits} {{\nblock0(v0:i{bits},v1:ptr):\n v2 = iconst.i{bits} 0\n{}{} v5 = select v3,v4,v0\n store v0,v1\n return v5\n}}\n",
                if order == 0 { &compare } else { &negate },
                if order == 0 { &negate } else { &compare }
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
fn exact_shapes_and_shared_negatives() {
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                let text = compile(&source(), sel, alloc, spill).0;
                assert_eq!(text.matches("csneg\t").count(), 4, "{text}");
                assert!(!text.contains("cset\t"), "{text}");
                for s in [
                    source().replace(" store v0,v1", " store v3,v1\n store v0,v1"),
                    source().replace(" store v0,v1", " store v4,v1\n store v0,v1"),
                    source()
                        .replace("iconst.i32 0", "iconst.i32 1")
                        .replace("iconst.i64 0", "iconst.i64 1"),
                    source().replace("select v3,v4,v0", "select v3,v0,v4"),
                ] {
                    assert!(!compile(&s, sel, alloc, spill).0.contains("csneg\t"));
                }
            }
        }
    }
}
#[test]
fn custom_constraints_and_source_locations_keep_original_boundaries() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct C(usize);
    impl InstructionSelector for C {
        fn name(&self) -> &'static str {
            "abs-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            fold: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, fold);
            let i = f.layout_insts().nth(self.0 + 1).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(i).for_each_operand(|v| args.push(v));
            s.set_constraint(
                i,
                ValuePart::low(f.resolve(args[0])),
                OperandKind::Use,
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..3 {
        let m = keel_ir::parse(&source()).unwrap();
        let selector = C(at);
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            !keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &p
            )
            .unwrap()
            .contains("csneg\t")
        );
    }
    for at in 0..3 {
        let mut m = keel_ir::parse(&source()).unwrap();
        m.files.push("abs.c".into());
        for f in &mut m.functions {
            let chain: Vec<_> = f.layout_insts().skip(1).take(3).map(|(_, i)| i).collect();
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
                .contains("csneg\t")
        );
    }
}
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_min_wrapping_live_source_and_all_spills() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-abs-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut c = String::from("#include <stdint.h>\n");
    let mut checks = String::new();
    for bits in [32, 64] {
        for order in [0, 1] {
            c += &format!("extern uint{bits}_t abs{bits}_{order}(uint{bits}_t,void*);\n");
            checks += &format!(
                "{{uint{bits}_t v=(uint{bits}_t)x,live=0;uint{bits}_t want=(v>>({bits}-1))?(uint{bits}_t)(0-v):v;uint{bits}_t got=abs{bits}_{order}(v,&live);if(got!=want||live!=v)return {order}+1;}}\n"
            );
        }
    }
    c += &format!(
        "int main(void){{uint64_t state=0x123456789abcdefULL;for(unsigned i=0;i<10000;i++){{state^=state<<13;state^=state>>7;state^=state<<17;uint64_t x=i==0?0:i==1?UINT64_MAX:i==2?0x8000000000000000ULL:i==3?0x80000000ULL:state;{checks}}}return 0;}}"
    );
    fs::write(dir.join("oracle.c"), c).unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for spill in [false, true] {
                fs::write(dir.join("test.o"), compile(&source(), sel, alloc, spill).1).unwrap();
                let p = Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
                assert!(
                    Command::new(dir.join("test")).status().unwrap().success(),
                    "{sel:?}/{alloc:?}/{spill}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
