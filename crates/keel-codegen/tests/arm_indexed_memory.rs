//! Indexed accesses keep the effect in place and expose both address leaves
//! before allocation, including when every leaf needs a scratch reload.
use keel_codegen::{
    CodegenPipeline, OptionsSelector, SelectionOptions,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::EntityRef;

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn vector_constant_offsets_keep_effects_and_have_typed_object_parity() {
    for shape in keel_ir::VectorType::ALL {
        let src = format!(
            "func @f(ptr,{shape}) -> {shape} cc(c) {{
block0(v0: ptr,v1: {shape}):
 v2 = iconst.i64 16
 v3 = ptradd v0,v2
 store v1,v3
 v4 = ptradd v0,v2
 v5 = load.{shape} v4
 return v5
}}"
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                let (text, _) = compile(
                    &src,
                    SelectionOptions {
                        trees: true,
                        folded_operands: true,
                        register_hints: true,
                    },
                    allocation,
                    pools,
                );
                assert_eq!(
                    text.lines().filter(|line| line.contains(", #16]")).count(),
                    2,
                    "{text}"
                );
                assert!(
                    !text
                        .lines()
                        .any(|line| line.contains("\tadd\t") && line.ends_with("#16")),
                    "{text}"
                );
            }
        }
    }
}

fn source(ty: &str, shift: i64) -> String {
    format!(
        "func @f(ptr, i64, {ty}) -> {ty}{} cc(c) {{
block0(v0: ptr, v1: i64, v2: {ty}):
 v3 = iconst.i64 {shift}
 v4 = shl v1, v3
 v5 = ptradd v0, v4
 store v2, v5
 v6 = ptradd v0, v4
 v7 = load.{ty} v6
 return v7
}}",
        if matches!(ty, "i8" | "i16") {
            " uext"
        } else {
            ""
        }
    )
}

fn compile(
    src: &str,
    options: SelectionOptions,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &OptionsSelector(options),
        allocator: &alloc,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap(), "{text}");
    (text, object)
}

#[test]
fn scalar_widths_all_selection_options_and_allocators_have_object_parity() {
    for (ty, scale) in [
        ("i8", 0),
        ("i16", 1),
        ("i32", 2),
        ("i64", 3),
        ("f32", 2),
        ("f64", 3),
        ("ptr", 3),
    ] {
        // Separate shifts give each access a single-use producer to consume.
        for shift in [scale, scale + 64, scale - 64] {
            let src = source(ty, shift)
                .replace("v6 = ptradd v0, v4", "v8 = shl v1, v3\n v6 = ptradd v0, v8");
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                    for bits in 0..8 {
                        let options = SelectionOptions {
                            trees: bits & 1 != 0,
                            folded_operands: bits & 2 != 0,
                            register_hints: bits & 4 != 0,
                        };
                        let (text, _) = compile(&src, options, allocation, pools);
                        let indexed = text
                            .lines()
                            .filter(|l| l.contains("[x") && l.contains(", lsl #"))
                            .count();
                        assert_eq!(indexed, if options.trees { 2 } else { 0 }, "{text}");
                        if options.trees {
                            assert!(!text.contains("\tlsl\t"), "{text}");
                            assert!(text.contains(&format!(", lsl #{scale}]")), "{text}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_shifts_stay_materialized_but_address_adds_fold() {
    let (text, _) = compile(
        &source("i64", 3),
        SelectionOptions::ALL,
        AllocationTier::Full,
        &SPILLED,
    );
    assert!(text.contains("\tlsl\t"), "{text}");
    assert_eq!(text.matches(", lsl #0]").count(), 2, "{text}");
}

#[test]
fn selected_operands_include_both_leaves_and_repeated_store_value() {
    use keel_codegen::select::{Selection, Target};
    let src = source("i64", 3).replace("store v2, v5", "store v1, v5");
    let m = keel_ir::parse(&src).unwrap();
    let f = &m.functions[0];
    let block = f.blocks().next().unwrap();
    let insts = f.block_insts(block);
    let params = f.block_params(block);
    let plan = Selection::compute_with_options(f, Target::Aarch64, SelectionOptions::ALL);
    assert!(plan.covered(insts[2]));
    // The shift has two uses, so it remains, while both ptradds disappear.
    assert!(!plan.covered(insts[1]));
    assert_eq!(
        plan.operands(f, insts[3]),
        vec![params[1], params[0], f.first_result(insts[1])]
    );
}

#[test]
fn shared_addresses_cross_block_addresses_and_atomics_remain_materialized() {
    let original = source("i64", 3);
    for src in [
        original
            .replace("v6 = ptradd v0, v4\n ", "")
            .replace("load.i64 v6", "load.i64 v5"),
        original.replace("store v2, v5", "jump block1\nblock1:\n store v2, v5"),
        original.replace("load.i64", "atomic_load.i64 acquire"),
    ] {
        let m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let (text, _) = compile(&src, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
        // At least one address must still be explicitly calculated.
        assert!(text.contains("\tadd\t"), "{text}");
    }
}

#[test]
fn debug_boundaries_and_nonmatching_scale_preserve_the_shift() {
    let original =
        source("i64", 2).replace("v6 = ptradd v0, v4", "v8 = shl v1, v3\n v6 = ptradd v0, v8");
    let (text, _) = compile(
        &original,
        SelectionOptions::ALL,
        AllocationTier::Full,
        &SPILLED,
    );
    assert_eq!(text.matches("\tlsl\t").count(), 2, "{text}");
    for (located_address, located_load, expected) in [
        (true, false, false),
        (true, true, true),
        (false, true, true),
    ] {
        let src = source("i64", 3);
        let mut m = keel_ir::parse(&src).unwrap();
        let f = &mut m.functions[0];
        let block = f.blocks().next().unwrap();
        let insts = f.block_insts(block).to_vec();
        let loc = keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 2,
            col: 1,
            scope: None,
        };
        if located_address {
            f.set_inst_loc(insts[4], Some(loc));
        }
        if located_load {
            f.set_inst_loc(insts[5], Some(loc));
        }
        let plan = keel_codegen::select::Selection::compute_with_options(
            f,
            keel_codegen::select::Target::Aarch64,
            SelectionOptions::ALL,
        );
        assert_eq!(plan.covered(insts[4]), expected);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_load_store_oracle_including_spilled_address_and_value() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/runtime-codegen-20261020/native-indexed-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut src = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut body = String::from("int main(void) {\n");
    for (ty, cty, scale) in [
        ("i8", "uint8_t", 0),
        ("i16", "uint16_t", 1),
        ("i32", "uint32_t", 2),
        ("i64", "uint64_t", 3),
        ("f32", "float", 2),
        ("f64", "double", 3),
        ("ptr", "void*", 3),
    ] {
        let name = format!("f_{ty}");
        let ir = source(ty, scale)
            .replace("@f(", &format!("@{name}("))
            .replace("v6 = ptradd v0, v4", "v8 = shl v1, v3\n v6 = ptradd v0, v8");
        src.push_str(&ir);
        src.push('\n');
        c.push_str(&format!("extern {cty} {name}(void*, int64_t, {cty});\n"));
        let value = match ty {
            "ptr" => "(void*)&a[0]",
            "f32" | "f64" => "(i * 1.25 - 80.5)",
            _ => "(UINT64_C(0xfedcba9876543210) ^ ((uint64_t)i * 123456789))",
        };
        body.push_str(&format!("{{ {cty} a[257]; memset(a, 0x55, sizeof(a)); for (int i=-128;i<=128;i++) {{ {cty} v=({cty}){value}; {cty} got={name}(&a[128], i, v); if (memcmp(&got,&v,sizeof(v)) || memcmp(&a[i+128],&v,sizeof(v))) return 1; }} }}\n"));
    }
    body.push_str("return 0; }\n");
    c.push_str(&body);
    // Address leaves must survive a call between their original definition
    // and the memory effect. The store also reads the index as its value.
    src.push_str(&format!(
        "decl @clobber()\n{}",
        source("i64", 3)
            .replace("@f(", "@crossing(")
            .replace("store v2, v5", "call @clobber()\n store v1, v5")
    ));
    c = c.replace("int main(void)", "extern uint64_t crossing(void*, int64_t, uint64_t);\nvoid clobber(void) { __asm__ volatile(\"\" ::: \"x9\", \"x10\", \"x11\", \"x12\", \"x13\", \"x14\", \"x15\"); }\nint main(void)");
    c = c.replace("return 0; }", "{ uint64_t a[257]; for(int i=-128;i<=128;i++) { uint64_t got=crossing(&a[128],i,0); if(got!=(uint64_t)(int64_t)i || a[i+128]!=got) return 2; } } return 0; }");
    fs::write(dir.join("driver.c"), c).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
            let (text, object) = compile(&src, SelectionOptions::ALL, allocation, pools);
            fs::write(dir.join("test.s"), text).unwrap();
            let output = Command::new("clang")
                .arg("-c")
                .arg(dir.join("test.s"))
                .arg("-o")
                .arg(dir.join("clang.o"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            use object::{Object, ObjectSection};
            let clang_object = fs::read(dir.join("clang.o")).unwrap();
            let text_bytes = |bytes: &[u8]| {
                object::File::parse(bytes)
                    .unwrap()
                    .section_by_name("__text")
                    .unwrap()
                    .data()
                    .unwrap()
                    .to_vec()
            };
            assert_eq!(text_bytes(&object), text_bytes(&clang_object));
            fs::write(dir.join("test.o"), object).unwrap();
            let output = Command::new("clang")
                .arg("-O2")
                .arg(dir.join("driver.c"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(dir.join("test")).status().unwrap().success());
        }
    }
}
