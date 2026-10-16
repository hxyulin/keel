//! Definition-sensitive upper-bit checks, plus native spill/call correctness.
use keel_codegen::{CodegenPipeline, Pools, SelectionTier, Target, regalloc::AllocationTier};

fn allocators() -> Vec<AllocationTier> {
    #[allow(unused_mut)]
    let mut tiers = vec![AllocationTier::Fast, AllocationTier::Full];
    #[cfg(feature = "experimental-regalloc")]
    tiers.extend([
        AllocationTier::Local,
        AllocationTier::Split,
        AllocationTier::Weighted,
    ]);
    tiers
}

fn assembly(
    source: &str,
    target: Target,
    selector: SelectionTier,
    allocator: AllocationTier,
    tiny: bool,
) -> String {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocator,
    };
    match target {
        Target::X86_64LinuxGnu => {
            let pools = if tiny {
                Pools {
                    caller_saved: &[6],
                    callee_saved: &[3],
                    float_caller_saved: &[8],
                    float_callee_saved: &[],
                }
            } else {
                keel_codegen::x86_64::REGISTERS
            };
            keel_codegen::x86_64::compile_with_pipeline(&module, &pools, &pipeline).unwrap()
        }
        Target::Aarch64AppleDarwin => {
            let pools = if tiny {
                Pools {
                    caller_saved: &[9],
                    callee_saved: &[19],
                    float_caller_saved: &[19],
                    float_callee_saved: &[8],
                }
            } else {
                keel_codegen::aarch64::REGISTERS
            };
            keel_codegen::aarch64::compile_with_pipeline(&module, &pools, &pipeline).unwrap()
        }
    }
}

fn extensions(output: &str, target: Target) -> usize {
    output
        .lines()
        .filter(|line| match target {
            Target::X86_64LinuxGnu => line.trim_start().starts_with("movzx\t"),
            Target::Aarch64AppleDarwin => ["uxtb\t", "uxth\t"]
                .iter()
                .any(|op| line.trim_start().starts_with(op)),
        })
        .count()
}

#[test]
fn materialized_loads_need_no_second_extension() {
    for target in Target::ALL {
        for allocator in allocators() {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for ty in ["i8", "i16"] {
                    // The second use keeps the narrow source alive, exercising
                    // a distinct destination as well as coalesced conversions.
                    let source = format!(
                        "func @f(ptr) -> i64 {{
block0(v0: ptr):
\n v1 = load.{ty} v0
\n v2 = uext.i64 v1
\n v3 = uext.i64 v1
\n v4 = iadd v2, v3
\n return v4
}}"
                    );
                    for tiny in [false, true] {
                        let output = assembly(&source, target, selector, allocator, tiny);
                        let expected = usize::from(target == Target::X86_64LinuxGnu);
                        assert_eq!(
                            extensions(&output, target),
                            expected,
                            "{target:?} {allocator:?} {selector:?}: {output}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn unknown_upper_bits_and_truncations_still_extend() {
    let sources = [
        "func @f(i8) -> i64 {\nblock0(v0: i8):\n v1 = uext.i64 v0\n return v1\n}",
        "func @f(i8, i8) -> i64 {\nblock0(v0: i8, v1: i8):\n v2 = iadd v0, v1\n v3 = uext.i64 v2\n return v3\n}",
        "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i8 v0\n jump block1(v1)\n block1(v2: i8):\n v3 = uext.i64 v2\n return v3\n}",
        "decl @external() -> i8\nfunc @f() -> i64 {\nblock0:\n v0 = call @external()\n v1 = uext.i64 v0\n return v1\n}",
        "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i16 v0\n v2 = trunc.i8 v1\n v3 = uext.i64 v2\n return v3\n}",
        "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = atomic_load.i8 seq_cst v0\n v2 = uext.i64 v1\n return v2\n}",
    ];
    for target in Target::ALL {
        for allocator in allocators() {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for (i, source) in sources.iter().enumerate() {
                    let output = assembly(source, target, selector, allocator, false);
                    let expected = if target == Target::X86_64LinuxGnu && [2, 4, 5].contains(&i) {
                        3
                    } else {
                        1
                    };
                    assert_eq!(extensions(&output, target), expected, "{i}: {output}");
                }
            }
        }
    }
}

#[test]
fn cset_is_clean_but_setcc_is_not() {
    let source = "func @f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = icmp.ult v0, v1\n v3 = uext.i64 v2\n return v3\n}";
    for target in Target::ALL {
        for allocator in allocators() {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let output = assembly(source, target, selector, allocator, false);
                assert_eq!(
                    extensions(&output, target),
                    usize::from(target == Target::X86_64LinuxGnu),
                    "{output}"
                );
            }
        }
    }
}

#[test]
fn unsigned_narrow_compare_reuses_clean_loads_on_arm() {
    for ty in ["i8", "i16"] {
        let source = format!(
            "func @f(ptr, ptr) -> i64 {{\nblock0(v0: ptr, v1: ptr):\n v2 = load.{ty} v0\n v3 = load.{ty} v1\n v4 = icmp.ult v2, v3\n v5 = uext.i64 v4\n return v5\n}}"
        );
        for allocator in allocators() {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let output = assembly(
                    &source,
                    Target::Aarch64AppleDarwin,
                    selector,
                    allocator,
                    false,
                );
                assert_eq!(
                    extensions(&output, Target::Aarch64AppleDarwin),
                    0,
                    "{output}"
                );
                let signed = source.replace("icmp.ult", "icmp.slt");
                let output = assembly(
                    &signed,
                    Target::Aarch64AppleDarwin,
                    selector,
                    allocator,
                    false,
                );
                assert!(
                    output.contains(if ty == "i8" { "sxtb\t" } else { "sxth\t" }),
                    "{output}"
                );
            }
        }
    }
}

#[test]
fn aliases_follow_the_resolved_definition_including_block_arguments() {
    use keel_ir::{EntityRef, Value};
    let source = "func @f(ptr, i8) -> i64 {\nblock0(v0: ptr, v1: i8):\n v2 = load.i8 v0\n v3 = uext.i64 v2\n return v3\n}";
    for clean in [false, true] {
        let source = if clean {
            source.replace("uext.i64 v2", "uext.i64 v1")
        } else {
            source.to_string()
        };
        let mut module = keel_ir::parse(&source).unwrap();
        let func = &mut module.functions[0];
        if clean {
            // Chain an otherwise unused parameter through a load definition.
            func.replace_all_uses(Value::new(1), Value::new(2));
        } else {
            // A load result aliased to an argument must lose its proof.
            func.replace_all_uses(Value::new(2), Value::new(1));
        }
        keel_ir::verify_module(&module).unwrap();
        for target in Target::ALL {
            for allocator in allocators() {
                let output = keel_codegen::compile_with_pipeline(
                    &module,
                    target,
                    &CodegenPipeline {
                        selector: &SelectionTier::Full,
                        allocator: &allocator,
                    },
                )
                .unwrap();
                let expected = usize::from(target == Target::X86_64LinuxGnu) + usize::from(!clean);
                assert_eq!(
                    extensions(&output, target),
                    expected,
                    "clean={clean}: {output}"
                );
            }
        }
    }
}

#[test]
fn distinct_conversion_destination_still_copies_the_value() {
    use keel_codegen::regalloc::{Allocation, AllocationInput, Loc, RegisterAllocator};
    struct Distinct;
    impl RegisterAllocator for Distinct {
        fn name(&self) -> &'static str {
            "distinct-test-registers"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let mut alloc = AllocationTier::Full.allocate(input);
            for (i, loc) in alloc.locs[0].iter_mut().enumerate() {
                *loc = Some(Loc::Reg(input.pools.caller_saved[i]));
            }
            alloc
        }
    }
    let module = keel_ir::parse("func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i8 v0\n v2 = uext.i64 v1\n return v2\n}").unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in Target::ALL {
        let output = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &SelectionTier::Baseline,
                allocator: &Distinct,
            },
        )
        .unwrap();
        // The forced source/destination are rdi/r8 on x86 and w10/w11 on ARM.
        let copy = match target {
            Target::X86_64LinuxGnu => "mov\tr8d, edi",
            Target::Aarch64AppleDarwin => "mov\tw11, w10",
        };
        assert!(output.contains(copy), "{output}");
        assert_eq!(
            extensions(&output, target),
            usize::from(target == Target::X86_64LinuxGnu),
            "{output}"
        );
    }
}

fn pressure_source() -> String {
    let mut source =
        String::from("decl @clobber()\nfunc @pressure(ptr) -> i64 {\nblock0(v0: ptr):\n");
    for i in 0..12 {
        let n = 1 + i * 3;
        source.push_str(&format!(
            "v{n} = iconst.i64 {}\nv{} = ptradd v0, v{n}\nv{} = load.i16 v{}\n",
            i * 2,
            n + 1,
            n + 2,
            n + 1
        ));
    }
    source.push_str("call @clobber()\njump block1\nblock1:\n");
    for i in 0..12 {
        source.push_str(&format!("v{} = uext.i64 v{}\n", 37 + i, 3 + i * 3));
    }
    source.push_str("v49 = iadd v37, v38\n");
    for i in 0..10 {
        source.push_str(&format!("v{} = iadd v{}, v{}\n", 50 + i, 49 + i, 39 + i));
    }
    source.push_str("return v59\n}\n");
    // Wrapped arithmetic and truncation must discard deliberately dirty bits.
    source.push_str("decl @dirty() -> i8\nfunc @call_narrow() -> i64 {\nblock0:\n v0 = call @dirty()\n v1 = uext.i64 v0\n return v1\n}\n");
    source.push_str("func @wrapped(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = trunc.i8 v0\n v3 = trunc.i8 v1\n v4 = iadd v2, v3\n v5 = uext.i64 v4\n return v5\n}\n");
    source.push_str(
        "func @incoming(i8) -> i64 {\nblock0(v0: i8):\n v1 = uext.i64 v0\n return v1\n}\n",
    );
    source.push_str("func @boolean(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = icmp.ult v0, v1\n v3 = uext.i64 v2\n return v3\n}\n");
    source
}

#[test]
fn native_loads_survive_call_spills_and_cross_block_liveness() {
    let Some(target) = Target::host() else { return };
    let source = pressure_source();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("narrow-extensions");
    std::fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    std::fs::write(&driver, r#"
#include <stdint.h>
extern uint64_t pressure(uint16_t *), wrapped(uint64_t,uint64_t), incoming(uint64_t), boolean(uint64_t,uint64_t), call_narrow(void);
uint64_t dirty(void) { return UINT64_C(0xfedcba98765432ab); }
__attribute__((noinline)) void clobber(void) {
#if defined(__x86_64__)
 __asm__ volatile("mov $-1, %%rax\nmov $-1, %%rcx\nmov $-1, %%rdx\nmov $-1, %%rsi\nmov $-1, %%rdi\nmov $-1, %%r8\nmov $-1, %%r9\nmov $-1, %%r10\nmov $-1, %%r11" ::: "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "memory");
#elif defined(__aarch64__)
 __asm__ volatile("mov x0, #-1\nmov x1, #-1\nmov x2, #-1\nmov x3, #-1\nmov x4, #-1\nmov x5, #-1\nmov x6, #-1\nmov x7, #-1\nmov x8, #-1\nmov x9, #-1\nmov x10, #-1\nmov x11, #-1\nmov x12, #-1\nmov x13, #-1\nmov x14, #-1\nmov x15, #-1\nmov x16, #-1\nmov x17, #-1" ::: "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "memory");
#endif
}
int main(void) {
 uint16_t p[12];
 for (unsigned k=0;k<256;k++) {
  uint64_t expected=0;
  for (unsigned i=0;i<12;i++) { p[i]=(uint16_t)(k*313+i*997); expected+=p[i]; }
  if (pressure(p)!=expected) return 1;
  uint64_t a=UINT64_C(0xfedcba9876543200)|k, b=UINT64_C(0x87654321000000ff);
  if (wrapped(a,b)!=(uint8_t)(a+b)) return 2;
  if (incoming(a)!=(uint8_t)a) return 3;
  if (call_narrow()!=171) return 5;
  if (boolean(a,b)!=(a<b) || boolean(b,a)!=(b<a)) return 4;
 }
 return 0;
}
"#).unwrap();
    for allocator in allocators() {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for tiny in [false, true] {
                let output = assembly(&source, target, selector, allocator, tiny);
                // Every extension in pressure must disappear; x86 retains the
                // twelve memory loads. Its other functions have four movzx.
                let expected = if target == Target::X86_64LinuxGnu {
                    16
                } else {
                    3
                };
                assert_eq!(extensions(&output, target), expected, "{output}");
                let asm = dir.join("test.s");
                let exe = dir.join("run");
                std::fs::write(&asm, output).unwrap();
                let result = std::process::Command::new("clang")
                    .args([&asm, &driver])
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = std::process::Command::new(&exe).status().unwrap();
                assert!(
                    result.success(),
                    "{allocator:?} {selector:?} tiny={tiny}: {result}"
                );
            }
        }
    }
}
