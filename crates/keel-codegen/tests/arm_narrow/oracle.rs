use std::{fs, process::Command};
pub const EMPTY: keel_codegen::regalloc::Pools = keel_codegen::regalloc::Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
pub fn compile(
    m: &keel_ir::Module,
    selector: &dyn keel_codegen::InstructionSelector,
    allocator: &dyn keel_codegen::regalloc::RegisterAllocator,
    pools: &keel_codegen::regalloc::Pools,
) -> (String, Vec<u8>) {
    let pipeline = keel_codegen::CodegenPipeline {
        selector,
        allocator,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let p = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let obj = keel_asm::assemble_program_object(&p, Default::default()).unwrap();
    assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
    (text, obj)
}
pub fn oracle(stem: &str, text: &str, obj: &[u8], c: &str) {
    use object::{Object, ObjectSection};
    let dir = std::env::current_dir()
        .unwrap()
        .join("../../target/arm-narrow-20261021/oracles")
        .join(stem);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("code.s"), text).unwrap();
    fs::write(dir.join("code.o"), obj).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    let run = |cmd: &mut Command| {
        let o = cmd.output().unwrap();
        assert!(o.status.success(), "{cmd:?}: {o:?}");
    };
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(dir.join("code.s"))
        .arg("-o")
        .arg(dir.join("clang.o")));
    let reference = fs::read(dir.join("clang.o")).unwrap();
    let actual = object::File::parse(obj).unwrap();
    let expected = object::File::parse(reference.as_slice()).unwrap();
    assert_eq!(
        actual.section_by_name("__text").unwrap().data().unwrap(),
        expected.section_by_name("__text").unwrap().data().unwrap()
    );
    let native = cfg!(all(target_arch = "aarch64", target_os = "macos"));
    if native {
        run(Command::new("clang")
            .args(["-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(dir.join("oracle.c"))
            .arg(dir.join("code.o"))
            .arg("-o")
            .arg(dir.join("native")));
        run(&mut Command::new(dir.join("native")));
    }
    fs::write(
        dir.join("result.json"),
        format!(
            "{{\"typed_text_equal\":true,\"clang_text_equal\":true,\"native_executed\":{native}}}\n"
        ),
    )
    .unwrap();
}
