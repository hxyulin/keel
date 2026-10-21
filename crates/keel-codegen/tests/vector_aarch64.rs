use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use std::{fs, process::Command};
// Runtime-only shared helpers are unused when cross-checking emission.
#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
#[path = "common/vector.rs"]
mod vector_common;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use vector_common::harness;
use vector_common::{command, fixture, temp};
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};
fn compile(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    tiny: bool,
) -> (String, keel_asm_ir::Program) {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if matches!(selection, SelectionTier::Full) {
        keel_opt::optimize(&mut module).unwrap();
    }
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let pools = if tiny {
        &TINY
    } else {
        &keel_codegen::aarch64::REGISTERS
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let typed = assemble_program_object(&program, TargetFeatures::default())
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    let parsed = assemble_object(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(typed, parsed, "typed/source Mach-O differ");
    (text, program)
}
#[test]
fn neon_operations_emit_zero_text_and_identical_objects() {
    let source = fixture();
    for (selection, allocation, tiny) in [
        (SelectionTier::Baseline, AllocationTier::Full, false),
        (SelectionTier::Full, AllocationTier::Full, true),
        (SelectionTier::Full, AllocationTier::Fast, true),
    ] {
        let (text, _) = compile(&source, selection, allocation, tiny);
        for instruction in [
            "cmeq\tv",
            "cmgt\tv",
            "cmge\tv",
            "cmhi\tv",
            "cmhs\tv",
            "bsl\tv",
            "fadd\tv",
            "dup\tv",
            "fmul\tv",
            "fdiv\tv",
            "fmin\tv",
            "fmax\tv",
            "frintn\tv",
            "mul\tv",
            "cnt\tv",
            "clz\tv",
            "rev64\tv",
            "ldr\tq",
            "str\tq",
        ] {
            assert!(text.contains(instruction), "missing {instruction}");
        }
    }
    let (aggregates, _) = aggregate_fixture();
    compile(&aggregates, SelectionTier::Full, AllocationTier::Full, true);
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn vector_operations_execute_unaligned_edges_masks_and_spills() {
    let dir = temp("arm-runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness()).unwrap();
    let source = fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for tiny in [false, true] {
                let (_, program) = compile(&source, selection, allocation, tiny);
                let object = dir.join("test.o");
                fs::write(
                    &object,
                    assemble_program_object(&program, TargetFeatures::default()).unwrap(),
                )
                .unwrap();
                let exe = dir.join("test");
                command(
                    Command::new("clang")
                        .args(["-O2", "-ffp-contract=off"])
                        .arg(&c)
                        .arg(&object)
                        .arg("-o")
                        .arg(&exe),
                );
                command(&mut Command::new(exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn apple_vector_abi_calls_varargs_cycles_and_partial_register_preservation_execute() {
    let (extra, harness) = vector_common::abi_fixture(true);
    let source = fixture() + &extra;
    let dir = temp("arm-abi");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let (_, program) = compile(&source, SelectionTier::Full, allocation, tiny);
            let object = dir.join("test.o");
            fs::write(
                &object,
                assemble_program_object(&program, TargetFeatures::default()).unwrap(),
            )
            .unwrap();
            let exe = dir.join("test");
            command(
                Command::new("clang")
                    .args(["-O2", "-ffp-contract=off"])
                    .arg(&c)
                    .arg(&object)
                    .arg("-o")
                    .arg(&exe),
            );
            command(&mut Command::new(exe));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn native_neon_instruction_encodings_match_clang() {
    use keel_ir::{Type, VectorType};
    use object::{Object, ObjectSection};
    let mut source = String::new();
    for shape in VectorType::ALL {
        source += &format!(
            "func @splat_{shape}({},ptr) {{\nblock0(v0: {},v1: ptr):\n v2 = vsplat.{shape} v0\n store v2,v1\n return\n}}\n",
            shape.lane_type(),
            shape.lane_type()
        );
        for op in vector_common::operations(shape) {
            let binary: Result<keel_ir::VectorBinaryOp, _> = op.parse();
            let keep = if shape.lane_type().is_float() {
                matches!(
                    op,
                    "vfadd"
                        | "vfsub"
                        | "vfmul"
                        | "vfdiv"
                        | "vfmin"
                        | "vfmax"
                        | "vfneg"
                        | "vfabs"
                        | "vfsqrt"
                        | "vfceil"
                        | "vffloor"
                        | "vftrunc"
                        | "vfnearest"
                )
            } else {
                matches!(
                    op,
                    "viadd" | "visub" | "vand" | "vor" | "vxor" | "vineg" | "vbnot" | "vbswap"
                ) || op == "vimul" && shape != VectorType::I64x2
                    || op == "vclz" && shape != VectorType::I64x2
                    || op == "vpopcnt" && shape.lane_type() == Type::I8
            };
            if !keep {
                continue;
            }
            source += &format!(
                "func @{}_{}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} v0\n v4 = load.{shape} v1\n v5 = {op} v3{}\n store v5,v2\n return\n}}\n",
                shape,
                op,
                if binary.is_ok() { ", v4" } else { "" }
            );
        }
    }
    let (text, program) = compile(
        &source,
        SelectionTier::Baseline,
        AllocationTier::Full,
        false,
    );
    let dir = temp("neon-clang");
    let input = dir.join("test.s");
    let output = dir.join("reference.o");
    fs::write(&input, text).unwrap();
    command(
        Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(input)
            .arg("-o")
            .arg(&output),
    );
    let bytes = fs::read(output).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let reference = file.section_by_name("__text").unwrap().data().unwrap();
    assert_eq!(
        keel_asm::assemble_program(&program, TargetFeatures::default())
            .unwrap()
            .text,
        reference
    );
    fs::remove_dir_all(dir).unwrap();
}

fn aggregate_fixture() -> (String, String) {
    use keel_ir::{Type, VectorType};
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut main = String::from("int main(void){\n");
    for shape in VectorType::ALL {
        let lane = shape.lane_type();
        let cty = if lane.is_float() {
            if lane == Type::F32 {
                "float".into()
            } else {
                "double".into()
            }
        } else {
            format!("uint{}_t", lane.bytes().unwrap() * 8)
        };
        let op = if lane.is_float() { "vfadd" } else { "viadd" };
        c += &format!("typedef {cty} V_{shape} __attribute__((vector_size(16)));\n");
        for count in 1..=5 {
            let layout = format!(
                "{{{}}}",
                std::iter::repeat_n(shape.to_string(), count)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let sig = format!("ptr sret {layout},ptr byval {layout}");
            let tag = format!("{shape}_{count}");
            c += &format!(
                "typedef struct {{V_{shape} v[{count}];}} H_{tag};\nextern H_{tag} twice_{tag}(H_{tag});\nextern void call_{tag}(void*,void*);\nH_{tag} clang_{tag}(H_{tag} a){{for(unsigned i=0;i<{count};i++)a.v[i]+=a.v[i];return a;}}\n"
            );
            ir += &format!(
                "decl @clang_{tag}({sig})\nfunc @twice_{tag}({sig}) {{\nblock0(v0: ptr,v1: ptr):\n"
            );
            for k in 0..count {
                let n = 2 + 5 * k;
                ir += &format!(
                    " v{n} = iconst.i64 {}\n v{} = ptradd v1,v{n}\n v{} = load.{shape} v{}\n v{} = {op} v{},v{}\n v{} = ptradd v0,v{n}\n store v{},v{}\n",
                    16 * k,
                    n + 1,
                    n + 2,
                    n + 1,
                    n + 3,
                    n + 2,
                    n + 2,
                    n + 4,
                    n + 3,
                    n + 4
                );
            }
            ir += " return\n}\n";
            ir += &format!(
                "func @call_{tag}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n call @clang_{tag}(v1,v0)\n return\n}}\n"
            );
            main += &format!(
                "{{H_{tag} a,b,r,out;for(unsigned j=0;j<{count};j++)for(unsigned i=0;i<{};i++)a.v[j][i]=({cty})(i+3*j+1);b=a;for(unsigned j=0;j<{count};j++)b.v[j]+=b.v[j];r=twice_{tag}(a);if(memcmp(&r,&b,sizeof(r)))return 21;call_{tag}(&a,&out);if(memcmp(&out,&b,sizeof(out)))return 22;}}\n",
                shape.lanes()
            );
        }
        // Seven named vectors leave only v7; the two-vector aggregate must
        // go to the stack and close the SIMD argument bank for the tail vector.
        let seven = std::iter::repeat_n(shape.to_string(), 7)
            .collect::<Vec<_>>()
            .join(", ");
        let params = (0..7)
            .map(|i| format!("v{i}: {shape}"))
            .collect::<Vec<_>>()
            .join(", ");
        ir += &format!(
            "func @overflow_{shape}({seven},ptr byval {{{shape},{shape}}},{shape}) -> {shape} {{\nblock0({params},v7: ptr,v8: {shape}):\n v9 = load.{shape} v7\n v10 = {op} v9,v8\n return v10\n}}\n"
        );
        c += &format!(
            "extern V_{shape} overflow_{shape}({},H_{shape}_2,V_{shape});\n",
            std::iter::repeat_n(format!("V_{shape}"), 7)
                .collect::<Vec<_>>()
                .join(", ")
        );
        main += &format!(
            "{{H_{shape}_2 h;V_{shape} a,b,r,e;for(unsigned i=0;i<{};i++){{a[i]=({cty})(i+1);b[i]=({cty})(i+7);}}h.v[0]=b;h.v[1]=a;r=overflow_{shape}(a,a,a,a,a,a,a,h,b);e=b+b;if(memcmp(&r,&e,16))return 23;}}\n",
            shape.lanes()
        );
    }
    // The ABI considers 128-bit short vectors homogeneous even when their
    // element types differ. All pieces still transfer as Q registers.
    ir += "func @mixed(ptr sret {f32x4,i32x4},ptr byval {f32x4,i32x4}) {\nblock0(v0: ptr,v1: ptr):\n v2 = load.f32x4 v1\n store v2,v0\n v3 = iconst.i64 16\n v4 = ptradd v0,v3\n v5 = ptradd v1,v3\n v6 = load.i32x4 v5\n store v6,v4\n return\n}\n";
    c += "typedef struct {V_f32x4 f;V_i32x4 i;} Mixed;extern Mixed mixed(Mixed);\n";
    main += "{Mixed a={{1,2,3,4},{5,6,7,8}};Mixed b=mixed(a);if(memcmp(&a,&b,sizeof(a)))return 24;}\nreturn 0;}\n";
    c += &main;
    (ir, c)
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn homogeneous_vector_aggregates_and_indirect_structs_interoperate_with_clang() {
    let (source, harness) = aggregate_fixture();
    let dir = temp("arm-aggregates");
    let input = dir.join("test.c");
    fs::write(&input, harness).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let (_, p) = compile(&source, SelectionTier::Full, allocation, tiny);
            let object = dir.join("test.o");
            fs::write(
                &object,
                assemble_program_object(&p, TargetFeatures::default()).unwrap(),
            )
            .unwrap();
            let exe = dir.join("test");
            command(
                Command::new("clang")
                    .args(["-O2", "-ffp-contract=off"])
                    .arg(&input)
                    .arg(&object)
                    .arg("-o")
                    .arg(&exe),
            );
            command(&mut Command::new(exe));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn unaligned_large_vector_spills_survive_dynamic_stack_and_hva_calls() {
    let mut source = String::from(
        "decl @clobber(ptr sret {i32x4,i32x4},ptr byval {i32x4,i32x4})\nfunc @large(ptr,ptr,ptr) {\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = iconst.i64 512\n v4 = intrinsic stack_alloc(v3)\n",
    );
    for i in 0..20 {
        let n = 5 + 3 * i;
        source += &format!(
            " v{n} = iconst.i64 {}\n v{} = ptradd v0,v{n}\n v{} = load.i32x4 v{}\n",
            16 * i,
            n + 1,
            n + 2,
            n + 1
        );
    }
    source += " store v7,v4\n call @clobber(v2,v0)\n";
    let mut acc = "v7".to_string();
    for i in 1..20 {
        source += &format!(" v{} = viadd {acc},v{}\n", 70 + i, 7 + 3 * i);
        acc = format!("v{}", 70 + i);
    }
    source += &format!(" store {acc},v1\n return\n}}\n");
    let types = std::iter::repeat_n("i32x4", 20)
        .collect::<Vec<_>>()
        .join(",");
    let params = (3..23)
        .map(|i| format!("v{i}: i32x4"))
        .collect::<Vec<_>>()
        .join(",");
    source += &format!(
        "func @large_params(ptr,ptr,ptr,{types}) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,{params}):\n call @clobber(v2,v0)\n"
    );
    let mut acc = "v3".to_string();
    for i in 4..23 {
        source += &format!(" v{} = viadd {acc},v{i}\n", 100 + i);
        acc = format!("v{}", 100 + i);
    }
    source += &format!(" store {acc},v1\n return\n}}\n");
    let c_types = std::iter::repeat_n("V", 20).collect::<Vec<_>>().join(",");
    let c_args = (0..20)
        .map(|i| format!("a[{i}]"))
        .collect::<Vec<_>>()
        .join(",");
    let c = format!(
        "#include <stdint.h>\n#include <string.h>\ntypedef uint32_t V __attribute__((vector_size(16)));typedef struct{{V a,b;}}H;H clobber(H a){{return a;}}extern void large(const void*,void*,void*);extern void large_params(const void*,void*,void*,{c_types});int main(void){{V a[20],r,e={{0}};H out;for(unsigned j=0;j<20;j++){{for(unsigned i=0;i<4;i++)a[j][i]=17*j+i;e+=a[j];}}large(a,&r,&out);if(memcmp(&e,&r,16)||memcmp(a,&out,32))return 1;large_params(a,&r,&out,{c_args});return memcmp(&e,&r,16)||memcmp(a,&out,32);}}"
    );
    let dir = temp("arm-large-spill");
    let input = dir.join("test.c");
    fs::write(&input, c).unwrap();
    for tiny in [false, true] {
        let (text, p) = compile(&source, SelectionTier::Full, AllocationTier::Full, tiny);
        if !tiny {
            assert!(
                text.contains("ldur\tq") || text.contains("add\tx17, x19"),
                "unaligned vector spill path should be exercised"
            );
        }
        let object = dir.join("test.o");
        fs::write(
            &object,
            assemble_program_object(&p, TargetFeatures::default()).unwrap(),
        )
        .unwrap();
        let exe = dir.join("test");
        command(
            Command::new("clang")
                .arg("-O2")
                .arg(&input)
                .arg(&object)
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    }
    fs::remove_dir_all(dir).unwrap();
}
