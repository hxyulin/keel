use keel_codegen::{CodegenPipeline, Pools, SelectionTier, regalloc::AllocationTier};

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
const MASK: &str = "func @mask(i64,i64) -> i64 cc(c) {
block0(v0:i64,v1:i64):
 v2 = iconst.i64 0
 v3 = icmp.eq v0,v2
 v4 = uext.i64 v3
 v5 = ineg v4
 v6 = icmp.eq v1,v2
 v7 = select v6,v5,v0
 v8 = xor v7,v1
 return v8
}";
fn shift_source(bits: u32, shared: bool) -> String {
    format!(
        "func @shift{bits}(i64,i64) -> i64 cc(c) {{\nblock0(v0:i64,v1:i64):\n v2 = iconst.i64 {}\n v3 = shl v0,v2\n v4 = sshr v3,v2\n v5 = xor v4,{}\n v6 = xor v5,v1\n return v6\n}}",
        64 - bits,
        if shared { "v3" } else { "v0" }
    )
}
fn compile(
    source: &str,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(source).unwrap();
    compile_module(&module, &sel, alloc, pools)
}
fn compile_module(
    module: &keel_ir::Module,
    sel: &dyn keel_codegen::InstructionSelector,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(module).unwrap();
    let pipeline = CodegenPipeline {
        selector: sel,
        allocator: &alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(keel_asm::x86::assemble_object(&text).unwrap(), object);
    (text, object)
}
#[test]
fn integer_idiom_shapes_and_shared_shift_inputs() {
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &ONE, &EMPTY] {
                let text = compile(MASK, sel, alloc, pools).0;
                assert!(text.contains("\tsbb\t"), "{text}");
                assert!(!text.contains("\tsete\t"), "{text}");
                for bits in [8, 16, 32] {
                    for shared in [false, true] {
                        let text = compile(&shift_source(bits, shared), sel, alloc, pools).0;
                        assert_eq!(
                            text.contains(if bits == 32 {
                                "\tmovsxd\t"
                            } else {
                                "\tmovsx\t"
                            }),
                            !shared,
                            "{text}"
                        );
                        assert_eq!(text.contains("\tsar\t"), shared, "{text}");
                        assert_eq!(text.contains("\tshl\t"), shared, "{text}");
                    }
                }
            }
        }
    }
}
#[test]
fn integer_idiom_exclusions_preserve_materialization() {
    let negatives = [
        MASK.replace("icmp.eq v0", "icmp.ne v0"),
        MASK.replace("icmp.eq v0,v2", "icmp.eq v0,v1"),
        MASK.replace("uext.i64", "sext.i64"),
        MASK.replace("v6 =", "v9 = uext.i64 v3\n v6 =")
            .replace("xor v7,v1", "xor v7,v9"),
        MASK.replace("v6 =", "v9 = xor v4,v1\n v6 =")
            .replace("xor v7,v1", "xor v7,v9"),
        MASK.replace("v5 =", "fence seq_cst\n v5 ="),
        MASK.replace("v4 =", "jump block1\nblock1:\n v4 ="),
        format!(
            "file f0 = \"idioms.c\"\n{}",
            MASK.replace("ineg v4", "ineg v4 !f0:2:1")
        ),
        MASK.replace(
            "v5 =",
            "x86_rflags_region terminal \"save; reapply; normal\"()\n v5 =",
        ),
    ];
    for source in negatives {
        let text = compile(&source, SelectionTier::Full, AllocationTier::Full, &EMPTY).0;
        assert!(!text.contains("\tsbb\t"), "{source}\n{text}");
    }
    for count in [0, 1, 31, 63, 64] {
        let source =
            shift_source(8, false).replace("iconst.i64 56", &format!("iconst.i64 {count}"));
        let text = compile(&source, SelectionTier::Full, AllocationTier::Full, &EMPTY).0;
        assert!(!text.contains("\tmovsx\t"), "{text}");
    }
    // Counts have the IR's modulo-64 semantics; the input may itself be the
    // folded immediate value, which must still get an allocation location.
    for source in [
        shift_source(8, false).replace("iconst.i64 56", "iconst.i64 120"),
        shift_source(8, false).replace("shl v0,v2", "shl v2,v2"),
    ] {
        let text = compile(&source, SelectionTier::Full, AllocationTier::Full, &EMPTY).0;
        assert!(text.contains("\tmovsx\t"), "{text}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_integer_idioms_shared_operands_spills_and_flag_consumers() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-integer-idioms-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut source = MASK.to_owned();
    for bits in [8, 16, 32] {
        source.push_str(&shift_source(bits, false));
        source.push_str(
            &shift_source(bits, true)
                .replace(&format!("@shift{bits}("), &format!("@shift{bits}_shared(")),
        );
    }
    source.push_str(
        &MASK
            .replace("@mask(", "@shared_mask(")
            .replace("v6 =", "v9 = uext.i64 v3\n v6 =")
            .replace("xor v7,v1", "xor v7,v9"),
    );
    source.push_str("func @negate(i64,i64) -> i64 cc(c) {\nblock0(v0:i64,v1:i64):\n v2 = bnot v0\n v3 = iconst.i64 1\n v4 = iadd v2,v3\n v5 = icmp.ult v0,v1\n v6 = select v5,v4,v1\n return v6\n}\n");
    let mut module = keel_ir::parse(&source).unwrap();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    let c = r#"
#include <stdint.h>
#include <assert.h>
extern uint64_t mask(uint64_t,uint64_t), shared_mask(uint64_t,uint64_t), negate(uint64_t,uint64_t);
extern uint64_t shift8(uint64_t,uint64_t),shift16(uint64_t,uint64_t),shift32(uint64_t,uint64_t);
extern uint64_t shift8_shared(uint64_t,uint64_t),shift16_shared(uint64_t,uint64_t),shift32_shared(uint64_t,uint64_t);
static uint64_t extend(uint64_t x,unsigned n) { uint64_t m=(UINT64_C(1)<<n)-1; x&=m; return x&(UINT64_C(1)<<(n-1))?x|~m:x; }
int main(void) {
 uint64_t edge[]={0,1,127,128,255,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,UINT64_C(0x8000000000000000),UINT64_MAX};
 for(unsigned i=0;i<sizeof(edge)/sizeof(edge[0]);i++) for(unsigned j=0;j<sizeof(edge)/sizeof(edge[0]);j++) {
  uint64_t x=edge[i],y=edge[j],m=x==0?UINT64_MAX:0;
  assert(negate(x,y)==(x<y?UINT64_C(0)-x:y));
  assert(mask(x,y)==((y==0?m:x)^y));
  assert(shared_mask(x,y)==((y==0?m:x)^(uint64_t)(x==0)));
  assert(shift8(x,y)==(extend(x,8)^x^y));
  assert(shift8_shared(x,y)==(extend(x,8)^(x<<56)^y));
  assert(shift16(x,y)==(extend(x,16)^x^y));
  assert(shift16_shared(x,y)==(extend(x,16)^(x<<48)^y));
  assert(shift32(x,y)==(extend(x,32)^x^y));
  assert(shift32_shared(x,y)==(extend(x,32)^(x<<32)^y));
 }
 return 0;
}
"#;
    fs::write(dir.join("oracle.c"), c).unwrap();
    let status = Command::new("cc")
        .args(["-O2", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .status()
        .unwrap();
    assert!(status.success());
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &ONE, &EMPTY] {
                let (_, object) = compile_module(&module, &sel, alloc, pools);
                fs::write(dir.join("kernels.o"), object).unwrap();
                let out = Command::new("cc")
                    .arg(dir.join("oracle.o"))
                    .arg(dir.join("kernels.o"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{out:?}");
                let out = Command::new(dir.join("run")).output().unwrap();
                assert!(out.status.success(), "{sel:?} {alloc:?}: {out:?}");
            }
        }
    }
}

#[test]
fn aliased_immediate_leaves_remain_live_at_the_selected_root() {
    use keel_ir::{EntityRef, Value};
    let source = shift_source(8, false)
        .replace("v3 =", "v7 = iconst.i64 56\n v3 =")
        .replace("shl v0,v2", "shl v7,v2")
        .replace("sshr v3,v2", "sshr v3,v7");
    let mut module = keel_ir::parse(&source).unwrap();
    module.functions[0].replace_all_uses(Value::new(7), Value::new(2));
    let text = compile_module(&module, &SelectionTier::Full, AllocationTier::Full, &EMPTY).0;
    assert!(text.contains("\tmovsx\t"), "{text}");
}

struct Constrained(u8);
impl keel_codegen::InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "integer-idiom-constraint"
    }
    fn select(
        &self,
        f: &keel_ir::Function,
        _: keel_codegen::select::Target,
        _: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        use keel_codegen::regalloc::{Constraint, OperandKind, ValuePart};
        let mut selection = keel_codegen::select::Selection::default();
        let inst = f
            .layout_insts()
            .map(|(_, i)| i)
            .find(|&i| match self.0 {
                0 => matches!(f.inst_data(i), keel_ir::InstData::Icmp { .. }),
                1 => matches!(f.inst_data(i), keel_ir::InstData::Convert { .. }),
                _ => matches!(f.inst_data(i), keel_ir::InstData::Unary { .. }),
            })
            .unwrap();
        selection.set_constraint(
            inst,
            ValuePart::low(f.first_result(inst)),
            OperandKind::Def,
            Constraint::Any,
        );
        selection
    }
}
#[test]
fn custom_constraints_prevent_zero_mask_covering() {
    let module = keel_ir::parse(MASK).unwrap();
    for node in 0..3 {
        let text = compile_module(&module, &Constrained(node), AllocationTier::Full, &EMPTY).0;
        assert!(!text.contains("\tsbb\t"), "{text}");
    }
}
