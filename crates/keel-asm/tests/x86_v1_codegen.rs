//! Strict-v1 lowering: tier matrix, independent assembler and runtime oracle.
use keel_asm::x86::{assemble_object_with_features, assemble_program_object_with_features};
use keel_asm_ir::{
    Program,
    x86::{CpuLevel, Features},
};
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier, x86_features::FeatureSelector,
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

fn fixture() -> String {
    let mut s = String::new();
    for bits in [8, 16, 32, 64] {
        s += &format!("func @pc{bits}(i64) -> i64 {{\nblock0(v0: i64):\n");
        if bits == 64 {
            s += " v1 = popcnt v0\n return v1\n}\n";
        } else {
            s += &format!(
                " v1 = trunc.i{bits} v0\n v2 = popcnt v1\n v3 = uext.i64 v2\n return v3\n}}\n"
            );
        }
    }
    s += "func @pc128(i128) -> i128 {\nblock0(v0: i128):\n v1 = popcnt v0\n return v1\n}\n";
    s += "func @pc_pressure(i64) -> i64 {\nblock0(v0: i64):\n";
    for n in 1..21 {
        s += &format!(
            " v{} = iconst.i64 {n}\n v{} = iadd v0, v{}\n",
            2 * n - 1,
            2 * n,
            2 * n - 1
        );
    }
    s += " v41 = popcnt v0\n";
    for n in 1..21 {
        s += &format!(" v{} = xor v{}, v{}\n", 41 + n, 40 + n, 2 * n);
    }
    s += " return v61\n}\n";
    for bits in [32, 64] {
        for op in ["fnearest", "ffloor", "fceil", "ftrunc"] {
            s += &format!(
                "func @{op}{bits}(f{bits}) -> f{bits} {{\nblock0(v0: f{bits}):\n v1 = {op} v0\n return v1\n}}\n"
            );
            // Inputs stay live and enough intermediates remain live to require spills.
            s += &format!(
                "func @{op}{bits}_pressure(f{bits}) -> f{bits} {{\nblock0(v0: f{bits}):\n"
            );
            for n in 1..21 {
                s += &format!(" v{n} = fadd v0, v0\n");
            }
            s += &format!(" v21 = {op} v0\n");
            for n in 1..21 {
                s += &format!(" v{} = fadd v{}, v{n}\n", 21 + n, 20 + n);
            }
            s += " return v41\n}\n";
        }
    }
    s
}
fn compile(selection: SelectionTier, allocation: AllocationTier) -> (String, Program) {
    compile_source(&fixture(), selection, allocation)
}
fn compile_source(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
) -> (String, Program) {
    compile_features(
        source,
        selection,
        allocation,
        Features::for_level(CpuLevel::V1),
    )
}
fn compile_features(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    features: Features,
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
fn run(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{c:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
type RelocationRecord = (u64, String, i64, String);
fn section(bytes: &[u8]) -> (Vec<u8>, Vec<RelocationRecord>) {
    let obj = object::File::parse(bytes).unwrap();
    let text = obj.section_by_name(".text").unwrap();
    (
        text.data().unwrap().to_vec(),
        text.relocations()
            .map(|(at, r)| {
                let object::RelocationTarget::Symbol(i) = r.target() else {
                    panic!("symbol required")
                };
                (
                    at,
                    format!("{:?}", r.flags()),
                    r.addend(),
                    obj.symbol_by_index(i).unwrap().name().unwrap().to_owned(),
                )
            })
            .collect(),
    )
}
#[test]
fn strict_v1_tier_matrix_matches_llvm_and_gnu() {
    let dir = std::env::temp_dir().join(format!("keel-v1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (text, p) = compile(selection, allocation);
            for forbidden in [
                "popcnt", "roundss", "roundsd", "tzcnt", "shlx", "vaddss", "vaddsd",
            ] {
                assert!(
                    !text
                        .lines()
                        .any(|line| line.split_whitespace().next() == Some(forbidden)),
                    "{forbidden}"
                );
            }
            let source = dir.join("input.s");
            std::fs::write(&source, &text).unwrap();
            let ours = assemble_program_object_with_features(&p, Features::for_level(CpuLevel::V1))
                .unwrap();
            let reference = dir.join("llvm.o");
            run(Command::new("clang")
                .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                .arg(&source)
                .arg("-o")
                .arg(&reference));
            assert_eq!(section(&ours), section(&std::fs::read(&reference).unwrap()));
            let gnu = std::env::var("KEEL_TEST_GNU_AS")
                .or_else(|_| std::env::var("KEEL_GNU_AS"))
                .unwrap_or_else(|_| "as".into());
            let reference = dir.join("gnu.o");
            run(Command::new(gnu)
                .arg("--64")
                .arg(&source)
                .arg("-o")
                .arg(&reference));
            let reference = std::fs::read(&reference).unwrap();
            // GNU and LLVM choose different multi-byte NOP padding at p2align.
            // Compare every function's exact bytes, excluding inter-function padding.
            let a = object::File::parse(ours.as_slice()).unwrap();
            let b = object::File::parse(reference.as_slice()).unwrap();
            let at = a.section_by_name(".text").unwrap().data().unwrap();
            let bt = b.section_by_name(".text").unwrap().data().unwrap();
            for sym in a
                .symbols()
                .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
            {
                let other = b
                    .symbols()
                    .find(|s| s.name().ok() == sym.name().ok())
                    .unwrap();
                let aa = &at[sym.address() as usize..(sym.address() + sym.size()) as usize];
                let bb = &bt[other.address() as usize..(other.address() + other.size()) as usize];
                assert!(
                    aa == bb,
                    "GNU instruction mismatch {}: lengths {} {} first {:?}",
                    sym.name().unwrap(),
                    aa.len(),
                    bb.len(),
                    aa.iter().zip(bb).position(|(a, b)| a != b)
                );
            }
            assert_eq!(section(&ours).1, section(&reference).1);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <math.h>\n#include <fenv.h>\n#include <stdio.h>\n#include <xmmintrin.h>\n",
    );
    for bits in [8, 16, 32, 64] {
        c += &format!("extern uint64_t pc{bits}(uint64_t);\n");
    }
    c += "extern __uint128_t pc128(__uint128_t);\nextern uint64_t pc_pressure(uint64_t);\n";
    for bits in [32, 64] {
        let ty = if bits == 32 { "float" } else { "double" };
        for op in ["fnearest", "ffloor", "fceil", "ftrunc"] {
            c +=
                &format!("extern {ty} {op}{bits}({ty});\nextern {ty} {op}{bits}_pressure({ty});\n");
        }
    }
    c += "int main(void) { uint64_t r=0x123456789abcdefULL;\nfor(unsigned n=0;n<10000;n++) {r^=r<<13;r^=r>>7;r^=r<<17;\n";
    for bits in [8, 16, 32, 64] {
        c += &format!(
            "{{uint{bits}_t x=(uint{bits}_t)r; unsigned want=0; for(unsigned b=0;b<{bits};b++) want+=(x>>b)&1; if(pc{bits}(x)!=want) return 1;}}\n"
        );
    }
    c += "{uint64_t hi=(r<<19)|(r>>45); __uint128_t x=((__uint128_t)hi<<64)|r; unsigned want=0; for(unsigned b=0;b<128;b++) want+=(x>>b)&1; if(pc128(x)!=want) return 2;}\n{uint64_t want=0; for(unsigned b=0;b<64;b++) want+=(r>>b)&1; for(unsigned j=1;j<=20;j++) want^=r+j; if(pc_pressure(r)!=want) return 5;}\n}\n";
    // Independent libm oracle, with bit-exact zero/NaN checks and varied ambient rounding.
    for bits in [32, 64] {
        let (ty, suffix, one, sign, quiet, exp) = if bits == 32 {
            (
                "float",
                "f",
                0x3f800000u64,
                0x80000000u64,
                0x00400000u64,
                0x7f800000u64,
            )
        } else {
            (
                "double",
                "",
                0x3ff0000000000000,
                0x8000000000000000,
                0x0008000000000000,
                0x7ff0000000000000,
            )
        };
        let mut extra = Vec::new();
        for value in [
            0.5f64,
            1.5,
            2.5,
            3.5,
            0.49999999999999994,
            0.5000000000000001,
            8388607.5,
            8388608.0,
            4503599627370495.5,
            4503599627370496.0,
        ] {
            let raw = if bits == 32 {
                (value as f32).to_bits() as u64
            } else {
                value.to_bits()
            };
            for magnitude in [raw - 1, raw, raw + 1] {
                extra.push(format!("{}ULL", magnitude));
                extra.push(format!("{}ULL", magnitude | sign));
            }
        }
        let extra = extra.join(",");
        c += &format!(
            "{{ uint{bits}_t special[]={{0,{sign}ULL,1,{sign}ULL|1,{one}ULL,{one}ULL|{sign}ULL,{exp}ULL,{exp}ULL|{sign}ULL,{exp}ULL|1,{exp}ULL|{quiet}ULL|123,{extra}}};\nfor(unsigned n=0;n<20000;n++) {{r^=r<<13;r^=r>>7;r^=r<<17; uint{bits}_t raw=n<sizeof(special)/sizeof(special[0])?special[n]:(uint{bits}_t)r; {ty} x; memcpy(&x,&raw,sizeof(x));\n"
        );
        for (op, oracle) in [
            ("fnearest", "nearbyint"),
            ("ffloor", "floor"),
            ("fceil", "ceil"),
            ("ftrunc", "trunc"),
        ] {
            c += &format!(
                "{{_mm_setcsr(_mm_getcsr()&~64u); fesetround(FE_TONEAREST); volatile {ty} want={oracle}{suffix}(x); uint{bits}_t expected; memcpy(&expected,(const void*)&want,sizeof(want)); if((raw&{exp}ULL)=={exp}ULL && (raw&({quiet}ULL*2-1))) expected=raw|{quiet}ULL; int modes[]={{FE_TONEAREST,FE_DOWNWARD,FE_UPWARD,FE_TOWARDZERO}}; for(unsigned m=0;m<4;m++) for(unsigned daz=0;daz<=64;daz+=64) {{fesetround(modes[m]); _mm_setcsr((_mm_getcsr()&~64u)|daz); uint{bits}_t check_expected=(daz && !(raw&{exp}ULL))?(raw&{sign}ULL):expected; feclearexcept(FE_ALL_EXCEPT); {ty} got={op}{bits}(x); uint{bits}_t actual; memcpy(&actual,&got,sizeof(got)); if(actual!=check_expected || fetestexcept(FE_INEXACT) || (!!fetestexcept(FE_INVALID) != (((raw&{exp}ULL)=={exp}ULL) && (raw&({quiet}ULL*2-1)) && !(raw&{quiet}ULL)))) {{fprintf(stderr,\"{op}{bits} n=%u mode=%u raw=%llx got=%llx want=%llx\\n\",n,m,(unsigned long long)raw,(unsigned long long)actual,(unsigned long long)check_expected); return 3;}} }} }}\n"
            );
        }
        c += "}}\n";
    }
    c += "_mm_setcsr(_mm_getcsr()&~64u); fesetround(FE_TONEAREST);\n";
    for bits in [32, 64] {
        for op in ["fnearest", "ffloor", "fceil", "ftrunc"] {
            c += &format!("if({op}{bits}_pressure(1.25)!= {op}{bits}(1.25)+50.0) return 4;\n");
        }
    }
    c += "return 0; }\n";
    c
}
#[test]
fn strict_v1_executes_against_independent_oracle() {
    let darwin = cfg!(target_os = "macos");
    if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        && !(darwin
            && Command::new("arch")
                .args(["-x86_64", "/usr/bin/true"])
                .status()
                .is_ok_and(|s| s.success()))
    {
        eprintln!("SKIP: no local x86-64 runner; Linux execution remains a parent gate");
        return;
    }
    let dir = std::env::temp_dir().join(format!("keel-v1-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let c = dir.join("test.c");
    std::fs::write(&c, harness()).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (_, p) = compile(selection, allocation);
            let obj = dir.join("test.o");
            std::fs::write(
                &obj,
                assemble_program_object_with_features(&p, Features::for_level(CpuLevel::V1))
                    .unwrap(),
            )
            .unwrap();
            let exe = dir.join("test");
            let mut clang = Command::new("clang");
            let input = if darwin {
                // Same SysV register ABI. Rewrap each relocation-free function's
                // exact ELF bytes, preserving internal relative branches.
                let bytes = std::fs::read(&obj).unwrap();
                let file = object::File::parse(bytes.as_slice()).unwrap();
                let text = file.section_by_name(".text").unwrap();
                assert_eq!(text.relocations().count(), 0);
                let bytes = text.data().unwrap();
                let mut source = String::from(".text\n");
                for sym in file
                    .symbols()
                    .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
                {
                    let name = sym.name().unwrap();
                    source += &format!(
                        ".globl _{name}\n_{name}:\n.byte {}\n",
                        bytes[sym.address() as usize..(sym.address() + sym.size()) as usize]
                            .iter()
                            .map(u8::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
                let path = dir.join("wrapped.s");
                std::fs::write(&path, source).unwrap();
                clang.args(["-target", "x86_64-apple-darwin"]);
                path
            } else {
                obj
            };
            run(clang
                .args(["-O2", "-fno-builtin", "-Wall", "-Wextra", "-Werror"])
                .arg(&c)
                .arg(&input)
                .arg("-lm")
                .arg("-o")
                .arg(&exe));
            run(&mut Command::new(&exe));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn strict_v1_existing_scalar_abi_and_fault_fixtures_remain_structured() {
    for name in [
        "unary_ops",
        "floats",
        "i128",
        "regalloc_pressure",
        "call_args",
        "abi_ext",
        "memory_flags",
        "div_by_zero",
        "sdiv_overflow",
        "compact_frame",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../keel-codegen/tests/programs/{name}.keel"));
        let source = std::fs::read_to_string(path).unwrap();
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                compile_source(&source, selection, allocation);
            }
        }
    }
}

#[test]
fn explicit_levels_and_independent_popcount_rounding_gates() {
    let v1 = Features::for_level(CpuLevel::V1);
    for features in [
        v1,
        Features::for_level(CpuLevel::V2),
        Features::for_level(CpuLevel::V3),
        Features {
            popcnt: true,
            sse41: true,
            ..Features::NONE
        },
        v1.with_features("popcnt").unwrap(),
        v1.with_features("sse41").unwrap(),
    ] {
        let (text, _) = compile_features(
            &fixture(),
            SelectionTier::Full,
            AllocationTier::Full,
            features,
        );
        let has = |name: &str| {
            text.lines()
                .any(|line| line.split_whitespace().next() == Some(name))
        };
        assert_eq!(has("popcnt"), features.popcnt);
        assert_eq!(has("roundss"), features.sse41);
        assert_eq!(has("roundsd"), features.sse41);
        if features.is_empty() {
            let module = keel_ir::parse(&fixture()).unwrap();
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            };
            assert_eq!(
                text,
                keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
                    .unwrap()
            );
        }
    }
    for instruction in [
        "popcnt rax,rdi",
        "roundss xmm0,xmm1,8",
        "roundsd xmm0,xmm1,8",
        "vaddss xmm0,xmm1,xmm2",
        "tzcnt rax,rdi",
        "shlx rax,rdi,rsi",
    ] {
        assert!(
            assemble_object_with_features(instruction, v1).is_err(),
            "{instruction}"
        );
        let mut p = Program::new();
        p.x86_instruction(keel_asm_ir::x86::Instruction::parse_intel(instruction).unwrap());
        assert!(
            assemble_program_object_with_features(&p, v1).is_err(),
            "{instruction}"
        );
        assert!(
            assemble_object_with_features(instruction, Features::for_level(CpuLevel::V3)).is_ok(),
            "{instruction}"
        );
    }
}
