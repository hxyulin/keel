//! Canonical sign masks and closed positive64 clamps; portable Linux ELF+C oracles.
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use std::{fs, path::PathBuf};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
fn source(name: &str, cc: &str, literal: i64, swapped: bool, root: &str) -> String {
    let operands = if swapped { "v3,v4" } else { "v4,v3" };
    format!(
        "func @{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i64 {literal}\n v3 = vsplat.i64x2 v2\n v4 = load.i64x2 align 1 v0\n v5 = vicmp.{cc} {operands}\n{root} store align 1 v6,v1\n v7 = iconst.i64 16\n v8 = ptradd v1,v7\n store align 1 v4,v8\n return\n}}\n"
    )
}
fn compile(
    m: &keel_ir::Module,
    sel: &dyn keel_codegen::InstructionSelector,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    compile_allocator(m, sel, &alloc, pools)
}
fn compile_allocator(
    m: &keel_ir::Module,
    sel: &dyn keel_codegen::InstructionSelector,
    alloc: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector: sel,
        allocator: alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = assemble_program_object(&program).unwrap();
    assert_eq!(assemble_object(&text).unwrap(), object);
    (text, object)
}
fn rows() -> [(&'static str, &'static str, i64, bool); 6] {
    [
        ("slt", "sgt", 0, false),
        ("sge", "sle", 0, true),
        ("uge", "ule", i64::MIN, false),
        ("ult", "ugt", i64::MIN, true),
        ("ugt", "ult", i64::MAX, false),
        ("ule", "uge", i64::MAX, true),
    ]
}
#[test]
fn canonical_predicates_and_strict_positive_fallback() {
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY] {
                for (cc, rev, k, complement) in rows() {
                    for swapped in [false, true] {
                        let src = source(
                            "sign",
                            if swapped { rev } else { cc },
                            k,
                            swapped,
                            " v6 = vor v5,v5\n",
                        );
                        let m = keel_ir::parse(&src).unwrap();
                        let text = compile(&m, &sel, alloc, pools).0;
                        assert_eq!(text.matches("psrad\t").count(), 1, "{text}");
                        assert_eq!(text.matches("pcmpgtd\t").count(), 0, "{text}");
                        assert_eq!(
                            text.matches("pcmpeqd\t").count(),
                            usize::from(complement),
                            "{text}"
                        );
                    }
                }
                for (cc, k) in [
                    ("sgt", 0),
                    ("sle", 0),
                    ("eq", 0),
                    ("ne", 0),
                    ("slt", 1),
                    ("ugt", -1),
                ] {
                    let m = keel_ir::parse(&source("fallback", cc, k, false, " v6 = vor v5,v5\n"))
                        .unwrap();
                    assert!(!compile(&m, &sel, alloc, pools).0.contains("psrad\t"));
                }
            }
        }
    }
}
#[test]
fn literal_proofs_require_canonical_fresh_definitions() {
    // Keep old literal instructions in the layout, replacing their results
    // with parameters. Neither old definition proves its replacement literal.
    let src = source("proof", "slt", 0, false, " v6 = vor v5,v5\n")
        .replace("(ptr,ptr)", "(ptr,ptr,i64,i64x2)")
        .replace("v1:ptr):", "v1:ptr,v20:i64,v21:i64x2):");
    let original = keel_ir::parse(&src).unwrap();
    for (index, param) in [(0, 2), (1, 3), (3, 3)] {
        let mut m = original.clone();
        let f = &mut m.functions[0];
        let inst = f.layout_insts().nth(index).unwrap().1;
        let p = f.block_params(f.entry_block().unwrap())[param];
        f.replace_all_uses(f.first_result(inst), p);
        assert!(
            !compile(&m, &SelectionTier::Full, AllocationTier::Full, &EMPTY)
                .0
                .contains("psrad\t")
        );
    }
    let src = source("proof", "slt", 0, false, " v6 = vor v5,v5\n")
        .replace(" v3 =", " v20 = iadd v2,v2\n v3 =")
        .replace("vsplat.i64x2 v2", "vsplat.i64x2 v20");
    assert!(
        !compile(
            &keel_ir::parse(&src).unwrap(),
            &SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY
        )
        .0
        .contains("psrad\t")
    );
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = include_str!("x86_sign/oracle.c").to_string();
    let mut calls = String::new();
    for (cc, rev, k, complement) in rows() {
        for swapped in [false, true] {
            let name = format!("sign_{cc}_{}", u8::from(swapped));
            ir += &source(
                &name,
                if swapped { rev } else { cc },
                k,
                swapped,
                " v6 = vor v5,v5\n",
            );
            c += &format!("extern void {name}(const void*,void*);\n");
            calls += &format!("if(check({name},{},0))return 2;\n", u8::from(complement));
        }
    }
    for (name, root, mode) in [
        ("clamp_and", " v6 = vand v5,v4\n", 2),
        ("clamp_commute", " v6 = vand v4,v5\n", 2),
        ("clamp_select", " v6 = vselect v5,v4,v3\n", 2),
        ("clamp_sum", " v22 = vand v5,v4\n v6 = viadd v22,v4\n", 4),
        ("clamp_external", " v6 = vand v5,v4\n", 3),
    ] {
        let mut src = source(name, "sgt", 0, false, root);
        if mode == 3 {
            src = src.replace(
                " return",
                " v9 = iconst.i64 32\n v10 = ptradd v1,v9\n store align 1 v5,v10\n return",
            );
        }
        ir += &src;
        c += &format!("extern void {name}(const void*,void*);\n");
        calls += &format!("if(check({name},0,{mode}))return 4;\n");
    }
    // Strict-positive observable masks retain canonical false at zero.
    ir += &source("strict", "sgt", 0, false, " v6 = vor v5,v5\n");
    c += "extern void strict(const void*,void*);\n";
    calls += "if(check(strict,0,1))return 3;\n";
    c += &format!(
        "int main(void){{if(setup())return 1;{calls}munmap(region,2*page);puts(\"sign64 masks/live/alias/guard/unaligned/extrema/random passed\");return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn export_linux_oracles() {
    let dir = std::env::var_os("KEEL_SIGN64_EVIDENCE").map(PathBuf::from);
    let (ir, c) = fixture();
    if let Some(dir) = &dir {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.keel"), &ir).unwrap();
        fs::write(
            dir.join("run_native.py"),
            include_str!("x86_sign/run_native.py"),
        )
        .unwrap();
    }
    let mut count = 0;
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &EMPTY),
            ] {
                let m = keel_ir::parse(&ir).unwrap();
                let (text, object) = compile(&m, &sel, alloc, pools);
                if let Some(dir) = &dir {
                    let stem = format!("{sel:?}-{alloc:?}-{pool}");
                    fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                    fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                    fs::write(dir.join(format!("{stem}.c")), &c).unwrap();
                }
                count += 1;
            }
        }
    }
    let (controlled_ir, controlled_c) = controlled_fixture();
    let m = keel_ir::parse(&controlled_ir).unwrap();
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let (text, obj) =
            compile_allocator(&m, &sel, &Controlled, &keel_codegen::x86_64::REGISTERS);
        assert_eq!(text.matches("psrad\t").count(), 15);
        if let Some(dir) = &dir {
            let stem = format!("controlled-{sel:?}");
            fs::write(dir.join(format!("{stem}.s")), text).unwrap();
            fs::write(dir.join(format!("{stem}.o")), obj).unwrap();
            fs::write(dir.join(format!("{stem}.c")), &controlled_c).unwrap();
        }
        count += 1;
    }
    if let (Some(dir), Some(input)) = (&dir, std::env::var_os("KEEL_SIGN64_KERNEL_IR")) {
        let src = fs::read_to_string(input).unwrap();
        let m = keel_ir::parse(&src).unwrap();
        let (text, object) = compile(
            &m,
            &SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
        );
        fs::write(dir.join("kernel.s"), text).unwrap();
        fs::write(dir.join("kernel.o"), object).unwrap();
        fs::write(dir.join("kernel.c"), include_str!("x86_sign/kernel.c")).unwrap();
        count += 1;
    }
    if let Some(dir) = &dir {
        fs::write(dir.join("expected-count.json"), format!("{count}\n")).unwrap();
    }
}

#[test]
fn closed_clamps_across_tiers_and_rejections() {
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY] {
                for root in [
                    " v6 = vand v5,v4\n",
                    " v6 = vand v4,v5\n",
                    " v6 = vselect v5,v4,v3\n",
                ] {
                    let src = source("clamp", "sgt", 0, false, root);
                    let text = compile(&keel_ir::parse(&src).unwrap(), &sel, alloc, pools).0;
                    assert_eq!(text.matches("psrad\t").count(), 1, "{text}");
                    assert_eq!(text.matches("pcmpgtd\t").count(), 0);
                    assert_eq!(text.matches("andnps\t").count(), 1);
                    for reject in [
                        src.replace("iconst.i64 0", "iconst.i64 1"),
                        src.replace(" return", " store v5,v1\n return"),
                        src.replace(" v6 =", " v20 = iconst.i64 4\n v6 ="),
                    ] {
                        assert!(
                            !compile(&keel_ir::parse(&reject).unwrap(), &sel, alloc, pools)
                                .0
                                .contains("psrad\t"),
                            "{reject}"
                        );
                    }
                }
            }
        }
    }
}

struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "sign64-controlled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut allocation = AllocationTier::Full.allocate(input);
        let r = |i| Loc::FReg(input.pools.float_caller_saved[i]);
        let (x, y) = match input.func.name.rsplit('_').next().unwrap() {
            "alias" => (r(0), r(0)),
            "distinct" => (r(0), r(1)),
            "spillinput" => (Loc::Stack(0), r(1)),
            "spillresult" => (r(0), Loc::Stack(2)),
            "allspill" => (Loc::Stack(0), Loc::Stack(2)),
            _ => unreachable!(),
        };
        allocation.spill_slots = 4;
        allocation.locs[0][0] = Some(x);
        allocation.locs[0][4] = Some(y);
        allocation
    }
}
fn controlled_fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = include_str!("x86_sign/oracle.c").to_string();
    c += "typedef uint64_t V __attribute__((vector_size(16)));\n";
    let mut calls = String::new();
    for mode in ["sign", "and", "select"] {
        for layout in ["alias", "distinct", "spillinput", "spillresult", "allspill"] {
            let name = format!("controlled_{mode}_{layout}");
            let (cc, root) = match mode {
                "sign" => ("slt", "vor v3,v3"),
                "and" => ("sgt", "vand v3,v0"),
                "select" => ("sgt", "vselect v3,v0,v2"),
                _ => unreachable!(),
            };
            ir += &format!(
                "func @{name}(i64x2) -> i64x2 {{\nblock0(v0:i64x2):\n v1 = iconst.i64 0\n v2 = vsplat.i64x2 v1\n v3 = vicmp.{cc} v0,v2\n v4 = {root}\n return v4\n}}\n"
            );
            c += &format!(
                "extern V {name}(V); static void call_{name}(const void*p,void*out){{V x,y;memcpy(&x,p,16);y={name}(x);memcpy(out,&y,16);memcpy((char*)out+16,&x,16);}}\n"
            );
            calls += &format!(
                "if(check(call_{name},0,{}))return 2;\n",
                if mode == "sign" { 0 } else { 2 }
            );
        }
    }
    c += &format!("int main(void){{if(setup())return 1;{calls}munmap(region,2*page);return 0;}}\n");
    (ir, c)
}
