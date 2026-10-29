//! Controlled allocation proves the operand overlap; native C checks the values
//! independently, including dirty upper bits, wrapped boundaries and spills.
use keel_codegen::{
    CodegenPipeline, Pools, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, RegisterAllocator},
};

const OPS: [&str; 8] = ["iadd", "and", "or", "xor", "imul", "isub", "shl", "ushr"];
const LAYOUTS: [&str; 8] = [
    "rhs",
    "lhs",
    "distinct",
    "same",
    "stacka",
    "stackb",
    "stackall",
    "stackresult",
];
const WIDTHS: [u32; 4] = [8, 16, 32, 64];

struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-two-address"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        let layout = input.func.name.rsplit('_').next().unwrap();
        let locations = match layout {
            "rhs" => [Loc::Reg(6), Loc::Reg(7), Loc::Reg(7)],
            "lhs" | "same" => [Loc::Reg(6), Loc::Reg(7), Loc::Reg(6)],
            "distinct" => [Loc::Reg(6), Loc::Reg(7), Loc::Reg(8)],
            "stacka" => [Loc::Stack(0), Loc::Reg(7), Loc::Reg(7)],
            "stackb" => [Loc::Reg(6), Loc::Stack(0), Loc::Reg(6)],
            "stackall" => [Loc::Stack(0), Loc::Stack(1), Loc::Stack(2)],
            "stackresult" => [Loc::Reg(6), Loc::Reg(7), Loc::Stack(0)],
            _ => unreachable!("{layout}"),
        };
        alloc.spill_slots = 3;
        for (i, loc) in locations.into_iter().enumerate() {
            alloc.locs[0][i] = Some(loc);
        }
        if alloc.locs[0].len() > 3 {
            alloc.locs[0][3] = Some(Loc::Reg(9));
        }
        alloc
    }
}

fn source(op: &str, bits: u32, layout: &str) -> String {
    let rhs = if layout == "same" { "v0" } else { "v1" };
    let ret = if bits == 64 {
        "return v2".to_string()
    } else {
        "v3 = uext.i64 v2\nreturn v3".to_string()
    };
    format!(
        "func @f_{op}_{bits}_{layout}(i{bits}, i{bits}) -> i64 {{\nblock0(v0: i{bits}, v1: i{bits}):\nv2 = {op} v0, {rhs}\n{ret}\n}}\n"
    )
}

fn assembly(
    source: &str,
    selector: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools<'_>,
) -> String {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_codegen::x86_64::compile_with_pipeline(
        &module,
        pools,
        &CodegenPipeline {
            selector: &selector,
            allocator,
        },
    )
    .unwrap()
}

#[test]
fn controlled_overlap_instructions_and_materialization() {
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for bits in WIDTHS {
            for op in &OPS[..6] {
                let mnemonic = match *op {
                    "iadd" => "add",
                    "isub" => "sub",
                    other => other,
                };
                let (a, b, d) = if bits == 64 {
                    ("rsi", "rdi", "r8")
                } else {
                    ("esi", "edi", "r8d")
                };
                for layout in LAYOUTS {
                    let asm = assembly(
                        &source(op, bits, layout),
                        selector,
                        &Controlled,
                        &keel_codegen::x86_64::REGISTERS,
                    );
                    let expected = match layout {
                        "rhs" if *op == "isub" => format!(
                            "mov\trax, rsi\n\tsub\t{}, {b}\n\tmov\trdi, rax",
                            if bits == 64 { "rax" } else { "eax" }
                        ),
                        "rhs" => format!("{mnemonic}\t{b}, {a}"),
                        "lhs" | "same" => {
                            format!("{mnemonic}\t{a}, {}", if layout == "same" { a } else { b })
                        }
                        // A sum into a third register is one LEA.
                        "distinct" if *op == "iadd" => format!("lea\t{d}, [rsi + rdi * 1]"),
                        "distinct" => format!("mov\tr8, rsi\n\t{mnemonic}\t{d}, {b}"),
                        "stacka" if *op != "isub" => format!(
                            "{mnemonic}\t{b}, {}",
                            if bits == 64 { "rax" } else { "eax" }
                        ),
                        "stacka" => format!("sub\t{}, {b}", if bits == 64 { "rax" } else { "eax" }),
                        "stackb" => format!(
                            "{mnemonic}\t{a}, {}",
                            if bits == 64 { "rcx" } else { "ecx" }
                        ),
                        "stackresult" if *op == "iadd" => format!(
                            "lea\t{}, [rsi + rdi * 1]",
                            if bits == 64 { "rax" } else { "eax" }
                        ),
                        "stackall" | "stackresult" => format!(
                            "{mnemonic}\t{}, {}",
                            if bits == 64 { "rax" } else { "eax" },
                            if layout == "stackall" {
                                if bits == 64 { "rcx" } else { "ecx" }
                            } else {
                                b
                            }
                        ),
                        _ => unreachable!(),
                    };
                    assert!(
                        asm.contains(&expected),
                        "{op}/{bits}/{layout}/{selector:?}: expected {expected}\n{asm}"
                    );
                    if layout == "rhs" && *op != "isub" {
                        assert!(
                            !asm.contains("mov\trax, rsi"),
                            "redundant scratch copy: {asm}"
                        );
                        assert!(!asm.contains("mov\trdi, rax"), "redundant move back: {asm}");
                    }
                    if layout.starts_with("stack") {
                        assert!(
                            asm.contains("qword ptr [rbp + -"),
                            "missing spill materialization: {asm}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn resolved_alias_uses_the_same_destination_once() {
    use keel_ir::{EntityRef, Value};
    for bits in WIDTHS {
        for op in &OPS[..6] {
            let mut module = keel_ir::parse(
                &source(op, bits, "same").replace(&format!("{op} v0, v0"), &format!("{op} v0, v1")),
            )
            .unwrap();
            module.functions[0].replace_all_uses(Value::new(1), Value::new(0));
            keel_ir::verify_module(&module).unwrap();
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let asm = keel_codegen::x86_64::compile_with_pipeline(
                    &module,
                    &keel_codegen::x86_64::REGISTERS,
                    &CodegenPipeline {
                        selector: &selector,
                        allocator: &Controlled,
                    },
                )
                .unwrap();
                let mnemonic = match *op {
                    "iadd" => "add",
                    "isub" => "sub",
                    other => other,
                };
                let reg = if bits == 64 { "rsi" } else { "esi" };
                assert!(asm.contains(&format!("{mnemonic}\t{reg}, {reg}")), "{asm}");
            }
        }
    }
}

#[test]
fn immediate_multiply_and_shifts_keep_their_operand_order() {
    for bits in WIDTHS {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for op in ["shl", "ushr"] {
                let asm = assembly(
                    &source(op, bits, "rhs"),
                    selector,
                    &Controlled,
                    &keel_codegen::x86_64::REGISTERS,
                );
                assert!(
                    asm.contains(if op == "shl" { "shl\t" } else { "shr\t" }),
                    "{asm}"
                );
            }
            let src = format!(
                "func @imm(i{bits}) -> i64 {{\nblock0(v0: i{bits}):\nv1 = iconst.i{bits} 7\nv2 = imul v0, v1\n{}\n}}",
                if bits == 64 {
                    "return v2"
                } else {
                    "v3 = uext.i64 v2\nreturn v3"
                }
            );
            let asm = assembly(
                &src,
                selector,
                &AllocationTier::Full,
                &keel_codegen::x86_64::REGISTERS,
            );
            assert!(asm.contains("imul\t"), "{asm}");
            if selector == SelectionTier::Full {
                assert!(
                    asm.lines()
                        .any(|l| l.contains("imul\t") && l.ends_with(", 7")),
                    "three operand immediate multiply: {asm}"
                );
            }
        }
    }
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn native_wrapped_values_with_controlled_and_real_allocators() {
    let mut src = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    let mut checks = String::new();
    for bits in WIDTHS {
        for op in OPS {
            for layout in LAYOUTS {
                src.push_str(&source(op, bits, layout));
                let name = format!("f_{op}_{bits}_{layout}");
                c.push_str(&format!("extern uint64_t {name}(uint64_t,uint64_t);\n"));
                let b = if layout == "same" { "a" } else { "b" };
                let expr = match op {
                    "iadd" => format!("a+{b}"),
                    "isub" => format!("a-{b}"),
                    "imul" => format!("a*{b}"),
                    "and" => format!("a&{b}"),
                    "or" => format!("a|{b}"),
                    "xor" => format!("a^{b}"),
                    "shl" => format!("(a&mask)<<({b}&{})", bits - 1),
                    "ushr" => format!("(a&mask)>>({b}&{})", bits - 1),
                    _ => unreachable!(),
                };
                let mask = u64::MAX >> (64 - bits);
                checks.push_str(&format!("{{ uint64_t mask=UINT64_C({mask}), want=({expr})&mask, got={name}(a,b); if(got!=want) {{ fprintf(stderr,\"{name}: %llx %llx got %llx want %llx\\n\",(unsigned long long)a,(unsigned long long)b,(unsigned long long)got,(unsigned long long)want); return 1; }} }}\n"));
            }
        }
    }
    c.push_str("int main(void) { const uint64_t v[]={0,1,7,127,128,255,256,32767,32768,65535,65536,UINT64_C(0x7fffffff),UINT64_C(0x80000000),UINT64_C(0xffffffff),UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_C(0xffffffffffffffff),UINT64_C(0xfedcba98765432ab)}; for(unsigned i=0;i<sizeof(v)/sizeof(*v);i++) for(unsigned j=0;j<sizeof(v)/sizeof(*v);j++) { uint64_t a=v[i], b=v[j];\n");
    c.push_str(&checks);
    c.push_str("} return 0; }\n");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("commutative-two-address");
    std::fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    std::fs::write(&driver, c).unwrap();
    let tiny = Pools {
        caller_saved: &[6],
        callee_saved: &[3],
        float_caller_saved: &[8],
        float_callee_saved: &[],
    };
    #[allow(unused_mut)]
    let mut tiers = vec![AllocationTier::Fast, AllocationTier::Full];
    #[cfg(feature = "experimental-regalloc")]
    tiers.extend([AllocationTier::Local, AllocationTier::Split]);
    let mut allocators: Vec<&dyn RegisterAllocator> = vec![&Controlled];
    allocators.extend(tiers.iter().map(|a| a as &dyn RegisterAllocator));
    let mut runs = 0;
    for allocator in allocators {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &tiny] {
                if allocator.name() == Controlled.name() && pools.caller_saved.len() == 1 {
                    continue;
                }
                let asm = dir.join("test.s");
                let exe = dir.join("run");
                std::fs::write(&asm, assembly(&src, selector, allocator, pools)).unwrap();
                let result = std::process::Command::new("clang")
                    .args([&asm, &driver])
                    .arg("-O2")
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(
                    std::process::Command::new(&exe).status().unwrap().success(),
                    "{} {selector:?} tiny={}",
                    allocator.name(),
                    pools.caller_saved.len() == 1
                );
                runs += 1;
            }
        }
    }
    eprintln!(
        "native configurations={runs}, functions=256, boundary pairs/function=324, comparisons={}",
        runs * 256 * 324
    );
}
