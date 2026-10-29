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
const SOURCE: &str = include_str!("../fixtures/sret_address.keel");

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
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object(&text).unwrap(),
        "{text}"
    );
    (text, object)
}

/// The instruction after `function`'s call to `callee`.
fn after_call<'a>(text: &'a str, function: &str, callee: &str) -> &'a str {
    let body = text.split(&format!("\n\"{function}\":")).nth(1).unwrap();
    let mut lines = body.lines().map(str::trim);
    lines
        .find(|l| l.starts_with("call\t") && l.contains(callee))
        .unwrap();
    lines.next().unwrap()
}

#[test]
fn fixed_destinations_recompute_addresses_and_pointer_arguments_keep_the_save() {
    let m = keel_ir::parse(SOURCE).unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &keel_codegen::x86_64::REGISTERS] {
                let (text, _) = compile(&m, sel, alloc, pools);
                for (f, callee) in [
                    ("fixed_pair", "make_pair"),
                    ("fixed_hfa", "make_hfa"),
                    ("fixed_nine", "make_nine"),
                    ("dynamic", "make_pair"),
                    ("overlap", "increment"),
                ] {
                    assert!(
                        after_call(&text, f, callee).starts_with("lea\tr11, ["),
                        "{f}: {text}"
                    );
                }
                assert!(
                    after_call(&text, "external", "make_pair").starts_with("mov\tr11, qword ptr ["),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn inline_assembly_and_replaced_stack_definitions_keep_the_save() {
    use keel_ir::EntityRef;
    let src = "decl @make_pair(ptr sret {i64,i64},i64) cc(c)\nfunc @aliased(ptr,i64)->i64 cc(c) {\n ss0 = stack_slot 16, align 8\nblock0(v0:ptr,v1:i64):\n v2 = stack_addr ss0\n call @make_pair(v2,v1)\n v3 = load.i64 v2\n return v3\n}";
    let mut m = keel_ir::parse(src).unwrap();
    m.functions[0].replace_all_uses(keel_ir::Value::new(2), keel_ir::Value::new(0));
    let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        after_call(&text, "aliased", "make_pair").starts_with("mov\tr11, qword ptr ["),
        "{text}"
    );
    let m = keel_ir::parse(&src.replace(" call @make_pair", " asm \"\", \"\"()\n call @make_pair"))
        .unwrap();
    let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        after_call(&text, "aliased", "make_pair").starts_with("mov\tr11, qword ptr ["),
        "{text}"
    );
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn native_register_results_dynamic_frames_overlap_and_indirect_calls() {
    use std::{fs, process::Command};
    let m = keel_ir::parse(SOURCE).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-x86-sret-address-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("../fixtures/sret_address.c"),
    )
    .unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &keel_codegen::x86_64::REGISTERS] {
                let (_, obj) = compile(&m, sel, alloc, pools);
                fs::write(dir.join("test.o"), obj).unwrap();
                let result = Command::new("clang")
                    .args(["-O2", "oracle.c", "test.o", "-o", "run"])
                    .current_dir(&dir)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
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
