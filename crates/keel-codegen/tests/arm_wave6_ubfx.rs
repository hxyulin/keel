//! UBFX covers must publish their only leaf before allocation.
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

fn source(bits: u32, shift: i64, mask: i64, left: bool, shared: bool) -> String {
    let args = if left { "v3, v2" } else { "v2, v3" };
    let tail = if shared {
        "v5 = xor v4, v2\nreturn v5"
    } else {
        "return v4"
    };
    format!(
        "func @f(i{bits}) -> i{bits} {{\nblock0(v0: i{bits}):\n v1 = iconst.i{bits} {shift}\n v2 = ushr v0, v1\n v3 = iconst.i{bits} {mask}\n v4 = and {args}\n {tail}\n}}"
    )
}

fn compile(
    src: &str,
    tier: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &tier,
        allocator: &allocation,
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
fn modulo_shift_fields_and_spills_have_native_text_parity() {
    for bits in [32u32, 64] {
        for shift in [0i64, 7, -1, i64::from(bits), i64::from(bits) + 7] {
            let lsb = shift as u64 & u64::from(bits - 1);
            let width = 8u64.min(u64::from(bits) - lsb);
            for left in [false, true] {
                let src = source(bits, shift, ((1u64 << width) - 1) as i64, left, false);
                for tier in [SelectionTier::Baseline, SelectionTier::Full] {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                            let (text, _) = compile(&src, tier, allocation, pools);
                            assert_eq!(
                                text.contains("\tubfx\t"),
                                matches!(tier, SelectionTier::Full),
                                "{text}"
                            );
                            if matches!(tier, SelectionTier::Full) {
                                assert!(text.contains(&format!("#{lsb}, #{width}")), "{text}");
                                assert!(!text.contains("\tlsr\t"), "{text}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_producers_nonruns_zero_allones_and_overlong_fields_fall_back() {
    for bits in [32u32, 64] {
        for (shift, mask, shared) in [
            (7, 255, true),
            (7, 0, false),
            (7, -1, false),
            (7, 0x55, false),
            (i64::from(bits) - 4, 255, false),
        ] {
            let (text, _) = compile(
                &source(bits, shift, mask, false, shared),
                SelectionTier::Full,
                AllocationTier::Full,
                &EMPTY,
            );
            assert!(!text.contains("\tubfx\t"), "{text}");
            assert!(text.contains("\tlsr\t"), "{text}");
        }
    }
}

#[test]
fn typed_mask_normalization_and_shared_mask_preserve_their_uses() {
    let src =
        source(32, 7, 0x1000000ff, false, false).replace("return v4", "v5 = xor v4, v3\nreturn v5");
    let (text, _) = compile(&src, SelectionTier::Full, AllocationTier::Full, &EMPTY);
    assert!(
        text.contains("\tubfx\t") && text.contains("#7, #8"),
        "{text}"
    );
    // The mask's XOR use remains independently encoded or materialized.
    assert!(text.contains("\teor\t"), "{text}");
    for bits in [8, 16] {
        let (text, _) = compile(
            &source(bits, 2, 7, false, false),
            SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY,
        );
        assert!(!text.contains("\tubfx\t"), "{text}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_extraction_matches_independent_bits_with_spills() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../suite/reports/optimization-wave6-20261016/native-tests/ubfx-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    for bits in [32u32, 64] {
        for shift in [0i64, 7, -1, i64::from(bits) + 7] {
            let lsb = shift as u32 & (bits - 1);
            let width = 8.min(bits - lsb);
            let mask = (1u64 << width) - 1;
            let mut src = source(bits, shift, mask as i64, false, false);
            if bits == 32 {
                // Truncation can leave dirty high bits in a 64-bit stack home.
                // The selected extraction must read the W execution width.
                src = src
                    .replace("func @f(i32)", "func @f(i64)")
                    .replace("block0(v0: i32):", "block0(v10: i64):\n v0 = trunc.i32 v10");
            }
            let c = format!(
                "#include <stdint.h>\nextern uint{bits}_t f(uint64_t);\nint main(void) {{\n uint64_t x=0x123456789abcdef0ULL;\n for(int i=0;i<1000;i++) {{\n x^=x<<13;x^=x>>7;x^=x<<17;\n uint{bits}_t input=(uint{bits}_t)x;\n if(f(x)!=((input >> {lsb}) & UINT64_C({mask}))) return 1;\n }}\nreturn 0;\n}}"
            );
            fs::write(dir.join("test.c"), c).unwrap();
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                    let (_, object) = compile(&src, SelectionTier::Full, allocation, pools);
                    fs::write(dir.join("test.o"), object).unwrap();
                    let status = Command::new("clang")
                        .arg("-O2")
                        .arg(dir.join("test.c"))
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test"))
                        .status()
                        .unwrap();
                    assert!(status.success());
                    assert!(Command::new(dir.join("test")).status().unwrap().success());
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
