//! Allocation boundary constraints must be enforced before either emitter runs.
use keel_codegen::regalloc::{
    Allocation, AllocationInput, AllocationTier, Constraint, Loc, OperandKind, RegisterAllocator,
    ValuePart,
};
use keel_codegen::select::{Selection, Target as SelectionTarget};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier, Target};
use keel_ir::{EntityRef, Function, InstData, Value};

const SOURCE: &str = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iadd v0, v1
 return v2
}";

#[test]
fn abi_preferences_preserve_call_operands_results_and_narrow_contracts() {
    let module = keel_ir::parse(
        "decl @opaque(i64,i64,i64,i64,i64,i64,i64,i64,i64) -> i64
func @roundtrip(i64,i64) -> i64 {
block0(v0:i64,v1:i64):
 v2 = iconst.i64 7
 v3 = call @opaque(v1,v0,v2,v0,v1,v2,v0,v1,v2)
 v4 = iadd v3,v0
 v5 = call @opaque(v0,v1,v3,v4,v2,v1,v0,v3,v4)
 v6 = iadd v5,v3
 v7 = xor v6,v1
 return v7
}
decl @opaque8(i8 uext) -> i8 uext
func @narrow_roundtrip(i64) -> i8 uext {
block0(v0:i64):
 v1 = trunc.i8 v0
 v2 = call @opaque8(v1)
 v3 = iconst.i8 1
 v4 = iadd v2,v3
 return v4
}
decl @opaque16(i16 sext) -> i16 sext
func @signed_roundtrip(i64) -> i16 sext {
block0(v0:i64):
 v1 = trunc.i16 v0
 v2 = call @opaque16(v1)
 v3 = iconst.i16 16385
 v4 = iadd v2,v3
 return v4
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = keel_test_support::TempDir::new("abi-register-preferences");
    std::fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
uint64_t roundtrip(uint64_t, uint64_t);
uint8_t narrow_roundtrip(uint64_t);
int16_t signed_roundtrip(uint64_t);
uint8_t opaque8(uint8_t x) { return x<100 ? x*3+17 : x/3+19; }
int16_t opaque16(int16_t x) {
    uint32_t bits=(uint32_t)(x<0 ? x/7 : x*3)&65535;
    return bits>=32768 ? (int32_t)bits-65536 : (int32_t)bits;
}
static uint64_t combine(const volatile uint64_t *v) {
    return (v[0]+v[1]*3)^(v[2]+v[3]*5)^(v[4]+v[5]*7)^(v[6]+v[7]*11+v[8]);
}
uint64_t opaque(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,
                uint64_t f,uint64_t g,uint64_t h,uint64_t i) {
    volatile uint64_t v[9]={a,b,c,d,e,f,g,h,i};
#if defined(__aarch64__)
    __asm__ volatile("mov x0,#19; mov x1,#23; mov x2,#29; mov x3,#31;"
        "mov x4,#37; mov x5,#41; mov x6,#43; mov x7,#47; mov x8,#53;"
        "mov x9,#59; mov x10,#61; mov x11,#67; mov x12,#71; mov x13,#73;"
        "mov x14,#79; mov x15,#83; mov x16,#89; mov x17,#97"
        ::: "x0","x1","x2","x3","x4","x5","x6","x7","x8","x9",
            "x10","x11","x12","x13","x14","x15","x16","x17","memory");
#elif defined(__x86_64__)
    __asm__ volatile("mov $19,%%rax; mov $23,%%rcx; mov $29,%%rdx;"
        "mov $31,%%rdi; mov $37,%%rsi; mov $41,%%r8; mov $43,%%r9;"
        "mov $47,%%r10; mov $53,%%r11"
        ::: "rax","rcx","rdx","rdi","rsi","r8","r9","r10","r11","memory");
#endif
    return combine(v);
}
int main(void) {
    uint64_t seed=7;
    for (unsigned n=0;n<256;n++) {
        seed=seed*UINT64_C(6364136223846793005)+1;
        uint64_t a=seed,b=seed^(seed>>17);
        uint64_t first[9]={b,a,7,a,b,7,a,b,7};
        uint64_t x=combine(first),y=x+a;
        uint64_t second[9]={a,b,x,y,7,b,a,x,y};
        if (roundtrip(a,b)!=((combine(second)+x)^b)) return 1;
        uint32_t byte=a&255;
        uint32_t want8=((byte<100 ? byte*3+17 : byte/3+19)+1)&255;
        if ((uint64_t)narrow_roundtrip(a)!=want8) return 2;
        int32_t word=(int32_t)((a&65535)^32768)-32768;
        uint32_t bits=(uint32_t)((word<0 ? word/7 : word*3)+16385)&65535;
        int32_t want16=bits>=32768 ? (int32_t)bits-65536 : (int32_t)bits;
        if ((int64_t)signed_roundtrip(a)!=want16) return 3;
    }
    return 0;
}
"#,
    )
    .unwrap();
    for target in Target::NATIVE {
        for tier in [AllocationTier::Bounded, AllocationTier::Compact] {
            let allocator = keel_codegen::regalloc::CallSiteAllocator { base: tier };
            let request = keel_codegen::Request::new(
                target,
                CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &allocator,
                },
            );
            let object =
                keel_asm::assemble_source_for(target, &request.text(&module).unwrap()).unwrap();
            assert_eq!(
                object,
                keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap()
            );
            if Some(target) != Target::host() {
                continue;
            }
            std::fs::write(dir.join("generated.o"), object).unwrap();
            let link = std::process::Command::new("clang")
                .arg("-O2")
                .arg(dir.join("oracle.c"))
                .arg(dir.join("generated.o"))
                .arg("-o")
                .arg(dir.join("oracle"))
                .output()
                .unwrap();
            assert!(link.status.success(), "{target:?}/{tier:?}: {link:?}");
            let run = std::process::Command::new(dir.join("oracle"))
                .output()
                .unwrap();
            assert!(run.status.success(), "{target:?}/{tier:?}: {run:?}");
        }
    }
}

struct Constrained;
impl InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "test-constraints"
    }
    fn select(
        &self,
        func: &Function,
        target: SelectionTarget,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> Selection {
        let mut selection = SelectionTier::Baseline.select(func, target, folds);
        let regs = match target {
            SelectionTarget::Aarch64 => keel_codegen::aarch64::REGISTERS,
            SelectionTarget::X86 => keel_codegen::x86_64::REGISTERS,
        };
        let inst = func.block_insts(func.entry_block().unwrap())[0];
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(0)),
            OperandKind::Use,
            Constraint::Fixed(Loc::Reg(regs.caller_saved[0])),
        );
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(1)),
            OperandKind::Use,
            Constraint::Register,
        );
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(2)),
            OperandKind::Def,
            Constraint::Reuse(ValuePart::low(Value::new(0))),
        );
        selection
    }
}
struct Allocator {
    invalid: bool,
}
impl RegisterAllocator for Allocator {
    fn name(&self) -> &'static str {
        "full"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        // A built-in name does not opt a custom allocator into wider pools.
        assert!(
            input.pools.caller_saved == keel_codegen::aarch64::REGISTERS.caller_saved
                || input.pools.caller_saved == keel_codegen::x86_64::REGISTERS.caller_saved
        );
        let mut alloc = AllocationTier::Full.allocate(input);
        for (i, loc) in alloc.locs[0].iter_mut().enumerate() {
            *loc = Some(Loc::Stack(i as u32));
        }
        alloc.spill_slots = input.func.num_values() as u32;
        let inst = input.func.block_insts(input.func.entry_block().unwrap())[0];
        let a = Loc::Reg(input.pools.caller_saved[0]);
        let b = Loc::Reg(input.pools.caller_saved[1]);
        let edit = alloc.edits.instruction_mut(inst);
        edit.uses = vec![
            (ValuePart::low(Value::new(0)), a),
            (ValuePart::low(Value::new(1)), b),
        ];
        edit.defs = vec![(
            ValuePart::low(Value::new(2)),
            if self.invalid { b } else { a },
        )];
        edit.before = vec![(a, Loc::Stack(0)), (b, Loc::Stack(1))];
        edit.after = vec![(Loc::Stack(2), a)];
        alloc
    }
}
#[test]
fn fixed_and_reused_operand_locations_reach_both_emitters() {
    let module = keel_ir::parse(SOURCE).unwrap();
    for target in Target::NATIVE {
        let assembly = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &Constrained,
                allocator: &Allocator { invalid: false },
            },
        )
        .unwrap();
        assert!(!assembly.is_empty());
        let error = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &Constrained,
                allocator: &Allocator { invalid: true },
            },
        )
        .unwrap_err();
        assert!(
            error.message.contains("unsatisfied allocation constraint"),
            "{error}"
        );
        if Some(target) == Target::host() {
            let dir =
                std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("allocation-constraints");
            std::fs::create_dir_all(&dir).unwrap();
            let asm = dir.join("f.s");
            let driver = dir.join("driver.c");
            let exe = dir.join("run");
            std::fs::write(&asm, assembly).unwrap();
            std::fs::write(
                &driver,
                "extern long f(long,long); int main(void) { return f(17,25) != 42; }",
            )
            .unwrap();
            let output = std::process::Command::new("clang")
                .args([&asm, &driver])
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(std::process::Command::new(&exe).status().unwrap().success());
        }
    }
}

#[test]
fn spilled_edge_values_reuse_free_homes_but_keep_live_inputs_distinct() {
    use keel_codegen::regalloc::Pools;
    use object::{Object, ObjectSection};
    use std::{cell::RefCell, collections::BTreeMap};

    struct Observed {
        tier: AllocationTier,
        homes: RefCell<BTreeMap<String, Vec<Option<Loc>>>>,
    }
    impl RegisterAllocator for Observed {
        fn name(&self) -> &'static str {
            "spill-affinity-oracle"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let allocation = self.tier.allocate(input);
            self.homes
                .borrow_mut()
                .insert(input.func.name.clone(), allocation.locs[0].clone());
            allocation
        }
    }
    let module = keel_ir::parse(
        "func @choose(i32, i64, i64) -> i64 {
block0(v0:i32, v1:i64, v2:i64):
 brif v0, block1(v1,v2), block2(v1,v2)
block1(v3:i64,v4:i64):
 v5 = iadd v3,v4
 return v5
block2(v6:i64,v7:i64):
 v8 = isub v6,v7
 return v8
}
func @overlap(i32, i64, i64) -> i64 {
block0(v0:i32,v1:i64,v2:i64):
 brif v0, block1(v1), block1(v2)
block1(v3:i64):
 v4 = xor v3,v1
 return v4
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let dir = keel_test_support::TempDir::new("spill-affinity-oracle");
    std::fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
uint64_t choose(int, uint64_t, uint64_t), overlap(int, uint64_t, uint64_t);
int main(void) {
    const uint64_t values[] = {0,1,17,UINT64_MAX,UINT64_C(1)<<63};
    for (unsigned i=0;i<5;++i) for (unsigned j=0;j<5;++j) for (int route=0;route<2;++route) {
        uint64_t a=values[i], b=values[j];
        if (choose(route,a,b)!=(route?a+b:a-b)) return 1;
        if (overlap(route,a,b)!=(route?0:b^a)) return 2;
    }
    return 0;
}
"#,
    )
    .unwrap();
    for target in Target::NATIVE {
        let mut sizes = Vec::new();
        for tier in [
            AllocationTier::Full,
            AllocationTier::Compact,
            AllocationTier::Bounded,
        ] {
            let observed = Observed {
                tier,
                homes: RefCell::new(BTreeMap::new()),
            };
            let request = keel_codegen::Request::new(
                target,
                CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &observed,
                },
            )
            .pools(&empty);
            let text = request.text(&module).unwrap();
            let bytes = keel_asm::assemble_source_for(target, &text).unwrap();
            assert_eq!(
                bytes,
                keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap()
            );
            if tier == AllocationTier::Compact {
                let homes = observed.homes.borrow();
                let choose = &homes["choose"];
                for (param, arg) in [(3, 1), (4, 2), (6, 1), (7, 2)] {
                    assert_eq!(
                        choose[param], choose[arg],
                        "{target:?}/{tier:?}: {choose:?}"
                    );
                }
                let overlap = &homes["overlap"];
                assert_ne!(overlap[3], overlap[1], "live input lost: {overlap:?}");
            }
            let object = object::File::parse(bytes.as_slice()).unwrap();
            sizes.push(
                object
                    .section_by_name(if target == Target::Aarch64AppleDarwin {
                        "__text"
                    } else {
                        ".text"
                    })
                    .unwrap()
                    .size(),
            );
            if Some(target) == Target::host() {
                std::fs::write(dir.join("generated.o"), bytes).unwrap();
                let link = std::process::Command::new("clang")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("generated.o"))
                    .arg("-o")
                    .arg(dir.join("oracle"))
                    .output()
                    .unwrap();
                assert!(
                    link.status.success(),
                    "{}",
                    String::from_utf8_lossy(&link.stderr)
                );
                let run = std::process::Command::new(dir.join("oracle"))
                    .output()
                    .unwrap();
                assert!(run.status.success(), "{target:?}/{tier:?}: {run:?}");
            }
        }
        assert!(sizes[1] < sizes[0], "{target:?}: {sizes:?}");
        assert_eq!(sizes[0], sizes[2], "{target:?}: {sizes:?}");
    }
}
