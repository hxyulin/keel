//! The compare is emitted at the branch, after intervening definitions/reloads.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "common/native_oracle.rs"]
mod native_oracle;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use native_oracle::PreparedOracle;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source(bits: u32, cc: &str, gap: &str) -> String {
    format!(
        "func @f(i{bits}, i{bits}) -> i64 {{\n ss0 = stack_slot 8, align 8\nblock0(v0: i{bits}, v1: i{bits}):\n v2 = icmp.{cc} v0, v1\n {gap}\n brif v2, block1, block2\nblock1:\n v20 = iconst.i64 37\n return v20\nblock2:\n v21 = iconst.i64 91\n return v21\n}}"
    )
}
const GAP: &str = "v3 = stack_addr ss0\nv4 = iconst.i64 37\nv5 = inttoptr.ptr v4";

fn compile(
    src: &str,
    tier: SelectionTier,
    allocation: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &tier,
        allocator: allocation,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let object =
        keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_object(&text).unwrap_or_else(|e| panic!("{e}\n{text}")),
        "{text}"
    );
    (text, object)
}

#[test]
fn compare_only_inputs_stay_live_across_the_span_in_all_tiers() {
    for bits in [32, 64] {
        for cc in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            for tier in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                        let (text, _) = compile(&source(bits, cc, GAP), tier, &allocation, pools);
                        assert!(!text.contains("\tcset\t"), "{text}");
                        assert!(text.contains("\tcmp\t"), "{text}");
                    }
                }
            }
        }
    }
}

#[test]
fn nonadjacent_narrow_compares_and_effects_keep_boolean_materialization() {
    for bits in [8, 16] {
        let (text, _) = compile(
            &source(bits, "eq", GAP),
            SelectionTier::Full,
            &AllocationTier::Full,
            &EMPTY,
        );
        assert!(text.contains("\tcset\t"), "{text}");
    }
    for gap in [
        "v3 = iadd v0, v1",
        "v3 = udiv v0, v1",
        "v3 = stack_addr ss0\nv4 = load.i64 v3",
        "v3 = stack_addr ss0\nstore v0, v3",
        "call @clobber()",
        "v3 = iconst.i64 1\nv4 = iconst.i64 2\nv5 = iconst.i64 3\nv6 = iconst.i64 4\nv7 = iconst.i64 5",
    ] {
        let src = format!("decl @clobber()\n{}", source(64, "eq", gap));
        let (text, _) = compile(&src, SelectionTier::Full, &AllocationTier::Full, &EMPTY);
        assert!(text.contains("\tcset\t"), "{gap}\n{text}");
    }
    let src = source(64, "eq", "v3 = uext.i64 v2");
    let (text, _) = compile(&src, SelectionTier::Full, &AllocationTier::Full, &EMPTY);
    assert!(text.contains("\tcset\t"), "{text}");
}

#[test]
fn shared_inputs_edges_and_duplicate_operands_have_valid_allocations() {
    let src = source(64, "eq", GAP)
        .replace("brif v2, block1, block2", "brif v2, block1(v0, v1, v5), block2")
        .replace("block1:\n v20 = iconst.i64 37", "block1(v6: i64, v7: i64, v8: ptr):\n v9 = ptrtoint.i64 v8\n v10 = xor v6, v7\n v20 = iadd v9, v10");
    for src in [&src, &src.replace("icmp.eq v0, v1", "icmp.eq v0, v0")] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                let (text, _) = compile(src, SelectionTier::Full, &allocation, pools);
                assert!(!text.contains("\tcset\t"), "{text}");
            }
        }
    }
}

#[cfg(feature = "experimental-regalloc")]
#[test]
fn spans_rematerialize_constants_without_losing_compare_uses() {
    use keel_codegen::regalloc::RematerializingAllocator;
    for base in [AllocationTier::Fast, AllocationTier::Full] {
        let allocation = RematerializingAllocator {
            base: &base,
            prefer_spilling: true,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        };
        for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
            let (text, _) = compile(
                &source(64, "eq", GAP),
                SelectionTier::Full,
                &allocation,
                pools,
            );
            assert!(!text.contains("\tcset\t"), "{text}");
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_delayed_compares_preserve_predicates_with_spills() {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n");
    let mut main = String::from("int main(void) {\n");
    let mut case = 0;
    for bits in [32, 64] {
        for (cc, operator, signed) in [
            ("eq", "==", false),
            ("ne", "!=", false),
            ("slt", "<", true),
            ("sge", ">=", true),
            ("ult", "<", false),
            ("uge", ">=", false),
        ] {
            let ty = if signed {
                format!("int{bits}_t")
            } else {
                format!("uint{bits}_t")
            };
            let name = format!("f{bits}_{cc}");
            ir += &source(bits, cc, GAP).replace("@f(", &format!("@{name}("));
            ir += "\n";
            c += &format!("extern uint64_t {name}(uint{bits}_t,uint{bits}_t);\n");
            case += 1;
            main += &format!(
                " {{\n uint64_t x=0x123456789abcdef0ULL,y=0;\n for(int i=0;i<1000;i++) {{\n x^=x<<13;x^=x>>7;x^=x<<17;y^=y<<13;y^=y>>7;y^=y<<17;y+=i;\n uint{bits}_t a=(uint{bits}_t)x,b=(i%3)?(uint{bits}_t)y:a;\n if({name}(a,b)!=((({ty})a {operator} ({ty})b)?37:91)) return {case};\n }}\n }}\n"
            );
        }
    }
    c += &main;
    c += "return 0;\n}\n";
    // One image per allocation and pool runs all twelve predicates.
    let oracle = PreparedOracle::new("arm-compare-spans", &c, &["-O2"]);
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool, pools) in [
            ("empty", &EMPTY),
            ("normal", &keel_codegen::aarch64::REGISTERS),
        ] {
            let (_, object) = compile(&ir, SelectionTier::Full, &allocation, pools);
            oracle.run(&object, &format!("{allocation:?} {pool}"));
        }
    }
}
