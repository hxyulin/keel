//! Constant-divisor division on x86-64: reciprocal multiplies and shifts
//! replace `idiv`/`div`, while divisors that trap or are 1 keep the
//! hardware path and its checks.

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

const WIDTHS: [u32; 4] = [8, 16, 32, 64];
const OPS: [&str; 4] = ["sdiv", "srem", "udiv", "urem"];

fn compile(
    m: &keel_ir::Module,
    selection: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
    (text, object)
}

fn mask(width: u32) -> u64 {
    u64::MAX >> (64 - width)
}

/// The divisor as the operation sees it: wrapped to the width, then signed
/// or unsigned.
fn normalized(width: u32, op: &str, imm: i64) -> i128 {
    let bits = imm as u64 & mask(width);
    if op.starts_with('s') {
        i128::from(((bits << (64 - width)) as i64) >> (64 - width))
    } else {
        i128::from(bits)
    }
}

/// Divisor immediates: small and large, both signs, powers of two, MIN and
/// MAX, and spellings that only normalize to a divisor at the IR width.
fn divisors(width: u32) -> Vec<i64> {
    let min = i64::MIN >> (64 - width);
    let max = !min;
    let mut divisors = vec![
        0,
        1,
        -1,
        2,
        -2,
        3,
        -3,
        4,
        -4,
        5,
        -5,
        6,
        7,
        -7,
        10,
        16,
        -64,
        100,
        min,
        min + 1,
        max,
        max - 1,
        max / 3,
        min / 3,
        mask(width) as i64,
        (mask(width) - 2) as i64,
    ];
    if width >= 16 {
        divisors.extend([641, -1000, 1 << 13, 12345]);
    }
    if width >= 32 {
        divisors.extend([1_000_003, -(1 << 20), 0x7fff_0001]);
    }
    if width == 64 {
        divisors.extend([1 << 40, 0x0123_4567_89ab_cdef, -0x0123_4567_89ab_cdef]);
    }
    if width < 64 {
        // Noncanonical: 3, -3, 0 and -1 spelled above the IR width.
        divisors.extend([
            (1i64 << width) + 3,
            (1i64 << width) - 3,
            1i64 << width,
            (1i64 << width) - 1,
        ]);
    }
    divisors
}

fn ext(width: u32, op: &str) -> &'static str {
    match (width < 32, op.starts_with('s')) {
        (false, _) => "",
        (true, true) => " sext",
        (true, false) => " uext",
    }
}

fn single(name: &str, width: u32, op: &str, imm: i64) -> String {
    format!(
        "func @{name}(i{width}) -> i{width}{} cc(c) {{\nblock0(v0: i{width}):\n v1 = iconst.i{width} {imm}\n v2 = {op} v0, v1\n return v2\n}}\n",
        ext(width, op)
    )
}

/// The quotient feeds a second division while the dividend and divisor stay
/// live, so results may alias either operand.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn chained(name: &str, width: u32, op: &str, imm: i64) -> String {
    format!(
        "func @{name}(i{width}) -> i{width}{} cc(c) {{\nblock0(v0: i{width}):\n v1 = iconst.i{width} {imm}\n v2 = {op} v0, v1\n v3 = iadd v2, v0\n v4 = {op} v3, v1\n v5 = isub v4, v2\n v6 = imul v5, v1\n return v6\n}}\n",
        ext(width, op)
    )
}

/// Reductions apply for divisors other than 0 and 1 (and -1 if signed).
fn reduced(width: u32, op: &str, imm: i64) -> bool {
    let d = normalized(width, op, imm);
    if op.starts_with('s') {
        !(-1..=1).contains(&d)
    } else {
        d > 1
    }
}

#[test]
fn constant_divisors_avoid_hardware_division() {
    for width in WIDTHS {
        for op in OPS {
            for imm in divisors(width) {
                let m = keel_ir::parse(&single("f", width, op, imm)).unwrap();
                for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                            let (text, _) = compile(&m, selection, allocation, pools);
                            let hardware = text.contains("\tidiv\t") || text.contains("\tdiv\t");
                            assert_eq!(
                                hardware,
                                !reduced(width, op, imm),
                                "i{width} {op} {imm}\n{text}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_constant_division_matches_c_oracle() {
    use std::{fs, os::unix::process::ExitStatusExt, process::Command};
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <sys/resource.h>\n#include <sys/prctl.h>\nstatic uint64_t seed = 0x9e3779b97f4a7c15ULL;\nstatic uint64_t next(void) { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; return seed; }\n",
    );
    let mut checks = String::new();
    let mut traps = Vec::new();
    for width in WIDTHS {
        let min = 1u64 << (width - 1);
        let max = min - 1;
        // A `TRY(x)` per dividend; `d` is the C divisor, `ud` its bits.
        let dividends = |sample_boundaries: bool| {
            if width == 8 {
                return format!("for (uint64_t i = 0; i <= {}ULL; i++) TRY(i);", mask(width));
            }
            let mut tries = format!(
                "TRY({min}ULL); TRY({min}ULL + 1); TRY({min}ULL + 2); TRY({max}ULL); TRY({max}ULL - 1); TRY(0); TRY(1); TRY(2); TRY(-1); TRY(-2); TRY(ud); TRY(ud + 1); TRY(ud - 1); TRY(0 - ud); TRY(1 - ud); TRY(-1 - ud); for (int i = 0; i < 256; i++) {{ uint64_t r = next(); TRY(r); TRY(r >> (r & 63)); TRY((int64_t)r >> (r & 63));"
            );
            if sample_boundaries {
                // Multiples of d nearest the top and the random value.
                tries += " TRY(top / d * d); TRY((__typeof__(d))r / d * d); TRY((uint64_t)((__typeof__(d))r / d * d) - 1);";
            }
            tries + " }"
        };
        for op in OPS {
            let signed = op.starts_with('s');
            let ct = format!("{}int{width}_t", if signed { "" } else { "u" });
            let operator = if op.ends_with("div") { "/" } else { "%" };
            for (index, imm) in divisors(width).into_iter().enumerate() {
                let dn = normalized(width, op, imm);
                for chain in [false, true] {
                    let name = format!("f{width}_{op}_{index}_{}", u8::from(chain));
                    ir += &if chain {
                        chained(&name, width, op, imm)
                    } else {
                        single(&name, width, op, imm)
                    };
                    c += &format!("extern {ct} {name}({ct});\n");
                    if dn == 0 {
                        traps.push(format!("(void){name}(({ct})1);"));
                        continue;
                    }
                    if signed && dn == -1 {
                        if op == "sdiv" {
                            traps.push(format!("(void){name}(({ct}){min}ULL);"));
                        }
                        if chain {
                            // MIN can recur as an intermediate; checked singly.
                            continue;
                        }
                    }
                    // Wrapping arithmetic through the unsigned type; `/` and
                    // `%` skip only MIN / -1, which traps in the IR.
                    let divide = |x: &str| {
                        if signed && dn == -1 {
                            if op == "sdiv" {
                                format!("({ct})(0 - (uint64_t){x})")
                            } else {
                                "0".to_string()
                            }
                        } else {
                            format!("({ct})({x} {operator} d)")
                        }
                    };
                    let expected = if chain {
                        format!(
                            "{ct} q1 = {}; {ct} s1 = ({ct})((uint64_t)q1 + (uint64_t)a); {ct} r1 = {}; {ct} e = ({ct})(((uint64_t)r1 - (uint64_t)q1) * (uint64_t)d);",
                            divide("a"),
                            divide("s1")
                        )
                    } else {
                        format!("{ct} e = {};", divide("a"))
                    };
                    let skip = if signed && op == "sdiv" && dn == -1 {
                        format!("if (a == ({ct}){min}ULL) break;")
                    } else {
                        String::new()
                    };
                    let d = if signed {
                        format!("({ct})({dn}LL)")
                    } else {
                        format!("({ct})({dn}ULL)")
                    };
                    let top = if signed { max } else { mask(width) };
                    let tries = dividends(!(signed && dn == -1));
                    checks += &format!(
                        "{{ const {ct} d = {d}, top = ({ct}){top}ULL; const uint64_t ud = (uint64_t)d; (void)top; (void)ud;\n#define TRY(x) do {{ {ct} a = ({ct})(x); {skip} {expected} {ct} got = {name}(a); if (got != e) {{ printf(\"{name} a=%lld got=%lld want=%lld\\n\", (long long)a, (long long)got, (long long)e); return 1; }} }} while (0)\n {tries}\n#undef TRY\n}}\n"
                    );
                }
            }
        }
    }
    c += "int main(int argc, char **argv) { struct rlimit lim={0,0}; if(setrlimit(RLIMIT_CORE,&lim)||prctl(PR_SET_DUMPABLE,0))return 4; if (argc > 1) { switch (atoi(argv[1])) {\n";
    for (i, call) in traps.iter().enumerate() {
        c += &format!("case {i}: {call} return 0;\n");
    }
    c += "} return 3; }\n";
    c += &checks;
    c += "return 0; }\n";
    let dir = std::env::temp_dir().join(format!("keel-x86-const-div-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("oracle.c"), &c).unwrap();
    // The oracle is large and invariant: compile it once, link per config.
    let driver = Command::new("clang")
        .arg("-O0")
        .arg("-c")
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(driver.status.success(), "{driver:?}");
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(&ir).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                    let (_, object) = compile(&m, selection, allocation, pools);
                    fs::write(dir.join("test.o"), object).unwrap();
                    let link = Command::new("clang")
                        .arg(dir.join("oracle.o"))
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test"))
                        .output()
                        .unwrap();
                    assert!(link.status.success(), "{link:?}");
                    let out = Command::new(dir.join("test")).output().unwrap();
                    let context = format!("{tier:?} {selection:?} {allocation:?}");
                    assert!(out.status.success(), "{context}: {out:?}");
                    for (case, call) in traps.iter().enumerate() {
                        let out = Command::new(dir.join("test"))
                            .arg(case.to_string())
                            .output()
                            .unwrap();
                        assert_eq!(
                            out.status.signal(),
                            Some(4),
                            "case {case} ({call}) {context}: {out:?}"
                        );
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

fn remainder_pair(name: &str, ty: &str, op: &str, gap: bool) -> String {
    format!(
        "func @{name}({ty}, {ty}) -> {ty}{} cc(c) {{
block0(v0: {ty}, v1: {ty}):
 v2 = {op} v0, v1
 {}
 v3 = imul v1, v2
 v4 = isub v0, v3
 v5 = xor v2, v4
 return v5
}}",
        if matches!(ty, "i8" | "i16") {
            if op == "sdiv" { " sext" } else { " uext" }
        } else {
            ""
        },
        if gap {
            format!("v6 = iconst.{ty} 0")
        } else {
            String::new()
        }
    )
}
#[test]
fn adjacent_quotient_remainder_uses_hardware_result_without_remultiply() {
    for ty in ["i8", "i16", "i32", "i64"] {
        for op in ["sdiv", "udiv"] {
            // An empty gap is a true three-instruction sequence. A laid-out
            // constant breaks the cover even if it later becomes machine-noop.
            for gap in [false, true] {
                let m = keel_ir::parse(&remainder_pair("f", ty, op, gap)).unwrap();
                for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                    for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                        let (text, _) = compile(&m, SelectionTier::Full, alloc, pools);
                        assert_eq!(text.contains("\timul\t"), gap, "{text}");
                        let div = if op == "sdiv" { "\tidiv\t" } else { "\tdiv\t" };
                        assert_eq!(text.matches(div).count(), 1, "{text}");
                    }
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_remainder_reuse_matches_c_and_preserves_division_traps() {
    use std::{fs, os::unix::process::ExitStatusExt, process::Command};
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdlib.h>\n#include <stdio.h>\n#include <sys/resource.h>\n#include <sys/prctl.h>\n",
    );
    let mut checks = String::new();
    let mut traps = String::new();
    let mut cases = 0;
    for width in WIDTHS {
        for signed in [true, false] {
            let ct = format!("{}int{width}_t", if signed { "" } else { "u" });
            let ty = format!("i{width}");
            let name = format!("f{width}_{signed}");
            ir += &remainder_pair(&name, &ty, if signed { "sdiv" } else { "udiv" }, false);
            c += &format!("extern {ct} {name}({ct},{ct});\n");
            let min = 1u64 << (width - 1);
            let skip = if signed {
                format!("if(a==({ct}){min}ULL && b==-1) continue;")
            } else {
                String::new()
            };
            checks += &format!(
                "{{ for(unsigned i=0;i<600;i++) for(unsigned j=1;j<100;j++) {{ {ct} a=({ct})(0x9e3779b97f4a7c15ULL*i),b=({ct})(0xa0761d6478bd642fULL*j); if(!b)continue; {skip} {ct} e=({ct})((uint64_t)(a/b)^(uint64_t)(a%b)); if({name}(a,b)!=e) {{ puts(\"{name}\"); return 1; }} }} }}\n"
            );
            traps += &format!("case {cases}: (void){name}(({ct})1,({ct})0); return 0;\n");
            cases += 1;
            if signed {
                traps +=
                    &format!("case {cases}: (void){name}(({ct}){min}ULL,({ct})-1); return 0;\n");
                cases += 1;
            }
        }
    }
    c += &format!(
        "int main(int argc,char**argv){{ struct rlimit lim={{0,0}}; if(setrlimit(RLIMIT_CORE,&lim)||prctl(PR_SET_DUMPABLE,0))return 4; if(argc>1){{switch(atoi(argv[1])){{ {traps} }}return 3;}} {checks} return 0; }}"
    );
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-remainder-reuse");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    let out = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let m = keel_ir::parse(&ir).unwrap();
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                let (_, obj) = compile(&m, sel, alloc, pools);
                fs::write(dir.join("test.o"), obj).unwrap();
                let out = Command::new("clang")
                    .arg(dir.join("oracle.o"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{out:?}");
                let out = Command::new(dir.join("test")).output().unwrap();
                assert!(out.status.success(), "{sel:?} {alloc:?}: {out:?}");
                for case in 0..cases {
                    let out = Command::new(dir.join("test"))
                        .arg(case.to_string())
                        .output()
                        .unwrap();
                    assert_eq!(out.status.signal(), Some(4), "case{case}: {out:?}");
                }
            }
        }
    }
}
