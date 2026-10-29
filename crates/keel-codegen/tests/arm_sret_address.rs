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
const SOURCE: &str = include_str!("fixtures/sret_address.keel");

fn compile(
    m: &keel_ir::Module,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &sel,
        allocator: &alloc,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap(), "{text}");
    (text, object)
}

fn after_call<'a>(text: &'a str, function: &str, callee: &str) -> &'a str {
    let body = text.split(&format!("_{function}:")).nth(1).unwrap();
    body.split(&format!("bl\t_{callee}"))
        .nth(1)
        .unwrap()
        .trim_start()
}

#[test]
fn fixed_destinations_recompute_addresses_and_dynamic_destinations_keep_the_save() {
    let m = keel_ir::parse(SOURCE).unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                let (text, _) = compile(&m, sel, alloc, pools);
                for (f, callee, base) in [
                    ("fixed_pair", "make_pair", "sp"),
                    ("fixed_hfa", "make_hfa", "sp"),
                    ("fixed_nine", "make_nine", "sp"),
                    ("dynamic", "make_pair", "x19"),
                    ("overlap", "increment", "sp"),
                ] {
                    assert!(
                        after_call(&text, f, callee).starts_with(&format!("add\tx16, {base},")),
                        "{text}"
                    );
                }
                assert!(
                    after_call(&text, "external", "make_pair").starts_with("ldr\tx16,"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn inline_assembly_and_replaced_stack_definitions_retain_the_save() {
    use keel_ir::EntityRef;
    let original = keel_ir::parse(SOURCE).unwrap();
    let src = "decl @make_pair(ptr sret {i64,i64},i64) cc(c)\nfunc @aliased(ptr,i64)->i64 cc(c) {\n ss0 = stack_slot 16, align 8\nblock0(v0:ptr,v1:i64):\n v2 = stack_addr ss0\n call @make_pair(v2,v1)\n v3 = load.i64 v2\n return v3\n}";
    let mut m = keel_ir::parse(src).unwrap();
    let f = &mut m.functions[0];
    f.replace_all_uses(keel_ir::Value::new(2), keel_ir::Value::new(0));
    let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        after_call(&text, "aliased", "make_pair").starts_with("ldr\tx16,"),
        "{text}"
    );
    let mut m =
        keel_ir::parse(&src.replace(" call @make_pair", " asm \"\", \"\"()\n call @make_pair"))
            .unwrap();
    let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        after_call(&text, "aliased", "make_pair").starts_with("ldr\tx16,"),
        "{text}"
    );
    // A debug source location does not change the fixed destination identity.
    m = original;
    m.files.push("sret.c".into());
    let f = &mut m.functions[0];
    let call = f
        .layout_insts()
        .find_map(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Call { .. }).then_some(i))
        .unwrap();
    f.set_inst_loc(
        call,
        Some(keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 10,
            col: 1,
            scope: None,
        }),
    );
    let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        after_call(&text, "fixed_pair", "make_pair").starts_with("add\tx16,"),
        "{text}"
    );
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_register_results_dynamic_frames_overlap_and_indirect_calls() {
    use std::{fs, process::Command};
    let m = keel_ir::parse(SOURCE).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-sret-address-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("fixtures/sret_address.c"),
    )
    .unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                let (_, obj) = compile(&m, sel, alloc, pools);
                fs::write(dir.join("test.o"), obj).unwrap();
                let status = Command::new("/usr/bin/clang")
                    .arg("-O2")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .status()
                    .unwrap();
                assert!(status.success());
                let result = Command::new(dir.join("run")).output().unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
