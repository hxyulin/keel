use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn vector_values_compile_on_both_native_targets() {
    for source in [
        "func @identity(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n return v0\n}",
        "func @local(ptr) {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n store v1, v0\n return\n}",
        "decl @external(f32x4) -> f32x4",
        "func @extract(ptr) -> i32 {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n v2 = vextract v1, 0\n return v2\n}",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
            keel_codegen::compile(&module, target).unwrap();
            let p = keel_codegen::compile_program(&module, target).unwrap();
            assert_eq!(p.stats().text_bytes, 0);
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Fast,
            };
            keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
            let p =
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
            assert_eq!(p.stats().text_bytes, 0);
        }
    }
}

#[test]
fn vector_aggregate_wrappers_and_memory_layouts_have_sysv_classification() {
    for source in [
        "decl @byval(ptr byval {f32x4})",
        "decl @sret(ptr sret {i64x2})",
        "decl @nested(ptr byval {[2 x i8x16]})",
    ] {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
        keel_codegen::compile_program(&m, Target::X86_64LinuxGnu).unwrap();
    }
}

#[test]
fn d_shapes_use_arm_exact_width_transport() {
    for source in [
        "func @load(ptr) -> i8x8 {\nblock0(v0: ptr):\n v1 = load.i8x8 v0\n return v1\n}",
        "decl @external(f32x2) -> f32x2",
        "decl @aggregate(ptr byval {i16x4})",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
        keel_codegen::compile_program(&module, Target::Aarch64AppleDarwin).unwrap();
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn literal_header_maps_preserve_overlap_guards_negative_starts_and_tails() {
    use std::{fs, process::Command};
    let mut m = keel_ir::parse(
        "func @header_map(ptr,ptr,i64) cc(c) {
block0(v0: ptr,v1: ptr,v2: i64):
 jump block1(v2)
block1(v3: i64):
 v4 = iconst.i64 17
 v5 = iconst.i32 7
 v6 = icmp.slt v3,v4
 brif v6,block2,block3
block2:
 v7 = iconst.i64 2
 v8 = shl v3,v7
 v9 = ptradd v0,v8
 v10 = ptradd v1,v8
 v11 = load.i32 v9
 v12 = load.i32 v10
 v13 = iadd v11,v12
 v14 = xor v13,v5
 store v14,v9
 v15 = iconst.i64 1
 v16 = iadd v3,v15
 jump block1(v16)
block3:
 return
}",
    )
    .unwrap();
    keel_opt::run_passes(&mut m, &["loop-vectorize"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .layout_insts()
            .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
    );
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("literal-header-map");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("fixtures/literal_header_map.c"),
    )
    .unwrap();
    let output = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(dir.join("driver.c"))
        .arg("-o")
        .arg(dir.join("driver.o"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let target = if cfg!(target_arch = "x86_64") {
        Target::X86_64LinuxGnu
    } else {
        Target::Aarch64AppleDarwin
    };
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &allocation,
        };
        let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
        let program = keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
        let object = if cfg!(target_arch = "x86_64") {
            let object = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
            object
        } else {
            let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
            object
        };
        fs::write(dir.join("test.o"), object).unwrap();
        let output = Command::new("clang")
            .arg(dir.join("driver.o"))
            .arg(dir.join("test.o"))
            .arg("-o")
            .arg(dir.join("test"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let output = Command::new(dir.join("test")).output().unwrap();
        assert!(output.status.success(), "{allocation:?}: {output:?}");
    }
}
