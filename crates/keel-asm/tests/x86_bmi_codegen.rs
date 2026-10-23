use keel_asm::x86::{assemble_object_with_features, assemble_program_object_with_features};
use keel_asm_ir::{Program, x86::Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier, x86_features::FeatureSelector,
};
const BMI: Features = Features {
    avx: false,
    bmi1: true,
    bmi2: true,
    ..Features::legacy()
};

fn fixture() -> String {
    let mut source = String::new();
    for bits in [8, 16, 32, 64] {
        let ty = format!("i{bits}");
        for op in ["shl", "ushr", "sshr", "rotl", "rotr"] {
            for mode in ["plain", "live", "same", "constant"] {
                source += &format!(
                    "func @{op}{bits}_{mode}(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n"
                );
                let (a, b) = if bits == 64 {
                    ("v0", "v1")
                } else {
                    source += &format!(" v2 = trunc.{ty} v0\n v3 = trunc.{ty} v1\n");
                    ("v2", "v3")
                };
                if mode == "constant" {
                    source += &format!(" v4 = iconst.{ty} 255\n");
                }
                let b = match mode {
                    "same" => a,
                    "constant" => "v4",
                    _ => b,
                };
                source += &format!(" v5 = {op} {a}, {b}\n");
                let result = if mode == "live" {
                    source += &format!(" v6 = xor v5, {a}\n v7 = xor v6, {b}\n");
                    "v7"
                } else {
                    "v5"
                };
                if bits == 64 {
                    source += &format!(" return {result}\n}}\n");
                } else {
                    source += &format!(" v8 = uext.i64 {result}\n return v8\n}}\n");
                }
            }
        }
        source += &format!("func @ctz{bits}(i64) -> i64 {{\nblock0(v0: i64):\n");
        if bits != 64 {
            source += &format!(" v1 = trunc.{ty} v0\n");
        }
        source += &format!(" v2 = ctz {}\n", if bits == 64 { "v0" } else { "v1" });
        if bits == 64 {
            source += " return v2\n}\n";
        } else {
            source += " v3 = uext.i64 v2\n return v3\n}\n";
        }
    }
    // Twenty live values force reloads, spilled destinations, and input aliases.
    source += "func @pressure(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n";
    for n in 0..20 {
        source += &format!(
            " v{} = iconst.i64 {}\n v{} = iadd v0, v{}\n",
            2 + 2 * n,
            n + 1,
            3 + 2 * n,
            2 + 2 * n
        );
    }
    source += " v42 = shl v0, v1\n";
    for n in 0..20 {
        source += &format!(" v{} = xor v{}, v{}\n", 43 + n, 42 + n, 3 + 2 * n);
    }
    source += " return v62\n}\n";
    source
}

fn compile(
    source: &str,
    features: Features,
    selection: SelectionTier,
    allocation: AllocationTier,
) -> (String, Program) {
    let module = keel_ir::parse(source).unwrap();
    let selector = FeatureSelector {
        inner: &selection,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    let text =
        keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline).unwrap();
    let program =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    assert_eq!(
        assemble_object_with_features(&text, features).unwrap(),
        assemble_program_object_with_features(&program, features).unwrap()
    );
    (text, program)
}

#[test]
fn bmi_selection_and_zero_text_byte_equal_outputs() {
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let source = fixture();
            let (baseline, _) = compile(&source, Features::default(), selection, allocation);
            let (text, _) = compile(&source, BMI, selection, allocation);
            for op in ["shlx", "shrx", "sarx", "rorx", "tzcnt"] {
                assert!(text.contains(op), "missing {op}");
                assert!(!baseline.contains(op), "baseline selected {op}");
            }
            let module = keel_ir::parse(&source).unwrap();
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            assert_eq!(
                baseline,
                keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
                    .unwrap()
            );
            // Full-width shifts of register arguments need neither a count
            // copy to ECX nor a destination copy for two-address constraints.
            let body = text
                .split("\"shl64_plain\":")
                .nth(1)
                .unwrap()
                .split(".size")
                .next()
                .unwrap();
            assert!(body.contains("shlx"), "{body}");
            assert!(!body.contains("ecx") && !body.contains("rcx"), "{body}");
        }
    }
}

#[test]
fn bmi_existing_integer_fixtures_stay_structured() {
    for name in ["unary_ops", "arith", "known_bits", "canonical_constants"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../keel-codegen/tests/programs/{name}.keel"));
        let source = std::fs::read_to_string(path).unwrap();
        compile(&source, BMI, SelectionTier::Full, AllocationTier::Full);
    }
}

fn runtime_harness() -> String {
    let mut harness = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    for bits in [8, 16, 32, 64] {
        for op in ["shl", "ushr", "sshr", "rotl", "rotr"] {
            for mode in ["plain", "live", "same", "constant"] {
                harness += &format!("extern uint64_t {op}{bits}_{mode}(uint64_t,uint64_t);\n");
            }
        }
        harness += &format!("extern uint64_t ctz{bits}(uint64_t);\n");
    }
    harness += "extern uint64_t pressure(uint64_t,uint64_t);\nint main(void) {\nuint64_t values[]={0,1,2,0x80,0x8000,0x80000000,0x8000000000000000ULL,~0ULL,0x76543210fedcba98ULL};\nuint64_t counts[]={0,1,7,8,15,16,31,32,63,64,65,127,128,255,256,~0ULL};\n";
    for bits in [8, 16, 32, 64] {
        let mask = if bits == 64 {
            "~0ULL".to_string()
        } else {
            format!("((1ULL<<{bits})-1)")
        };
        harness += &format!(
            "for(unsigned i=0;i<sizeof(values)/8;i++) {{ uint64_t a=values[i]&{mask}; unsigned tz=0; uint64_t t=a; while(t && !(t&1)) {{ tz++; t>>=1; }} if(!a) tz={bits}; if(ctz{bits}(values[i])!=tz) return 1;\n"
        );
        for (op, expr) in [
            ("shl", "a<<c".to_string()),
            ("ushr", "a>>c".to_string()),
            ("sshr", format!("(uint64_t)((int{bits}_t)a>>c)")),
            ("rotl", format!("(a<<c)|(a>>(({bits}-c)&({bits}-1)))")),
            ("rotr", format!("(a>>c)|(a<<(({bits}-c)&({bits}-1)))")),
        ] {
            for mode in ["plain", "live", "same", "constant"] {
                let count = match mode {
                    "same" => "a",
                    "constant" => "255",
                    _ => "counts[j]",
                };
                harness += &format!(
                    "for(unsigned j=0;j<sizeof(counts)/8;j++) {{ unsigned c=({count})&({bits}-1); uint64_t expected=({expr})&{mask};\n"
                );
                if mode == "live" {
                    harness += &format!("expected=(expected^a^counts[j])&{mask};\n");
                }
                harness += &format!(
                    "if({op}{bits}_{mode}(values[i],counts[j])!=expected) {{ fprintf(stderr,\"{op}{bits}_{mode} i=%u j=%u\\n\",i,j); return 2; }} }}\n"
                );
            }
        }
        harness += "}\n";
    }
    harness += "for(unsigned i=0;i<sizeof(values)/8;i++) for(unsigned j=0;j<sizeof(counts)/8;j++) { uint64_t a=values[i], expected=a<<(counts[j]&63); for(unsigned n=1;n<=20;n++) expected^=a+n; if(pressure(a,counts[j])!=expected) return 3; }\nreturn 0; }\n";
    harness
}

#[test]
fn bmi_codegen_runtime_harness_compiles() {
    let temp = std::env::temp_dir().join(format!("keel-bmi-codegen-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("test.c");
    std::fs::write(&input, runtime_harness()).unwrap();
    let output = std::process::Command::new("clang")
        .args(["-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
        .arg(input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn bmi_codegen_executes_widths_counts_zero_aliases_and_spills() {
    if !std::is_x86_feature_detected!("bmi1") || !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    let harness = runtime_harness();
    let temp =
        std::env::temp_dir().join(format!("keel-bmi-codegen-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let c = temp.join("test.c");
    std::fs::write(&c, harness).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (_, program) = compile(&fixture(), BMI, selection, allocation);
            let object = temp.join("test.o");
            std::fs::write(
                &object,
                assemble_program_object_with_features(&program, BMI).unwrap(),
            )
            .unwrap();
            let executable = temp.join("test");
            let output = std::process::Command::new("clang")
                .args(["-O2"])
                .arg(&c)
                .arg(object)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(&executable)
                    .status()
                    .unwrap()
                    .success(),
                "{selection:?}/{allocation:?}"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}
