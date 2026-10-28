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
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
    (text, object)
}

fn function(name: &str, width: u32, op: &str, divisor: Option<i64>) -> String {
    let (params, block, constant) = match divisor {
        Some(n) => (
            format!("i{width}"),
            format!("v0: i{width}"),
            format!("v1 = iconst.i{width} {n}\n"),
        ),
        None => (
            format!("i{width}, i{width}"),
            format!("v0: i{width}, v1: i{width}"),
            String::new(),
        ),
    };
    let ext = if width < 32 {
        if op.starts_with('s') {
            " sext"
        } else {
            " uext"
        }
    } else {
        ""
    };
    format!(
        "func @{name}({params}) -> i{width}{ext} cc(c) {{\nblock0({block}):\n{constant} v2 = {op} v0, v1\n return v2\n}}\n"
    )
}

#[test]
fn constant_divisor_checks_use_ir_width_and_keep_required_traps() {
    for width in [8, 16, 32, 64] {
        let mask = u64::MAX >> (64 - width);
        for op in ["sdiv", "srem", "udiv", "urem"] {
            for divisor in [None, Some(0), Some(1), Some(3), Some(-1), Some(mask as i64)] {
                let m = keel_ir::parse(&function("f", width, op, divisor)).unwrap();
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                        for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                            let (text, _) = compile(&m, selection, allocation, pools);
                            let normalized = divisor.map(|n| n as u64 & mask);
                            assert_eq!(
                                text.contains("\tcbz\t"),
                                normalized.is_none_or(|n| n == 0),
                                "{width} {op} {divisor:?}\n{text}"
                            );
                            assert_eq!(
                                text.contains("\tccmp\t"),
                                op == "sdiv" && normalized.is_none_or(|n| n == mask),
                                "{width} {op} {divisor:?}\n{text}"
                            );
                        }
                    }
                }
            }
        }
        if width < 64 {
            let m = keel_ir::parse(&function(
                "wrapped_zero",
                width,
                "udiv",
                Some(1i64 << width),
            ))
            .unwrap();
            let (text, _) = compile(&m, SelectionTier::Full, AllocationTier::Full, &SPILLED);
            assert!(text.contains("\tcbz\t"), "{text}");
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_constant_and_reused_quotients_match_oracle_and_trap_edges() {
    use std::{fs, os::unix::process::ExitStatusExt, process::Command};
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <stdlib.h>\n#include <limits.h>\n");
    let mut checks = String::new();
    let mut traps = Vec::new();
    for width in [8u32, 16, 32, 64] {
        let mask = u64::MAX >> (64 - width);
        let min = 1u64 << (width - 1);
        for op in ["sdiv", "srem", "udiv", "urem"] {
            let signed = op.starts_with('s');
            let ct = format!("{}int{width}_t", if signed { "" } else { "u" });
            let operator = if op.ends_with("div") { "/" } else { "%" };
            let mut divisors = vec![None, Some(0), Some(1), Some(3), Some(-1), Some(mask as i64)];
            if width < 64 {
                divisors.push(Some(1i64 << width));
            }
            for (index, divisor) in divisors.into_iter().enumerate() {
                let name = format!("f{width}_{op}_{index}");
                ir += &function(&name, width, op, divisor);
                c += &format!(
                    "extern {ct} {name}({ct}{});\n",
                    if divisor.is_none() {
                        format!(", {ct}")
                    } else {
                        String::new()
                    }
                );
                let args = if divisor.is_none() { "a,b" } else { "a" };
                let b = divisor.unwrap_or(3) as u64 & mask;
                let signed_b = if signed {
                    ((b << (64 - width)) as i64 >> (64 - width)).to_string()
                } else {
                    format!("{b}ULL")
                };
                if b != 0 {
                    checks += &format!(
                        "{{ {ct} b=({ct})({signed_b}); uint64_t vals[]={{0,1,2,3,{min}ULL,{}ULL,{mask}ULL}}; for(unsigned j=0;j<7;j++){{ {ct} a=({ct})vals[j];",
                        min - 1
                    );
                    if signed && op == "sdiv" && b == mask {
                        checks += &format!("if((uint64_t)a==({ct}){min}ULL)continue;");
                    }
                    let expected = if signed && op == "srem" && b == mask {
                        "0".to_string()
                    } else {
                        format!("a {operator} b")
                    };
                    checks += &format!("if({name}({args})!=({ct})({expected}))return 1; }} }}\n");
                }
                if divisor.is_none() || b == 0 {
                    traps.push(format!(
                        "(void){name}((({ct})1){});",
                        if divisor.is_none() {
                            format!(",({ct})0")
                        } else {
                            String::new()
                        }
                    ));
                }
                if signed && op == "sdiv" && (divisor.is_none() || b == mask) {
                    traps.push(format!(
                        "(void){name}(({ct}){min}ULL{});",
                        if divisor.is_none() {
                            format!(",({ct})-1")
                        } else {
                            String::new()
                        }
                    ));
                }
            }
        }
        // Same-block quotient reuse, with division results deliberately unused.
        let name = format!("reuse{width}");
        let ext = if width < 32 { " sext" } else { "" };
        ir += &format!(
            "func @{name}(i{width}, i{width}) -> i{width}{ext} cc(c) {{\nblock0(v0: i{width}, v1: i{width}):\n v2 = sdiv v0,v1\n v3 = srem v0,v1\n return v3\n}}\n"
        );
        c += &format!("extern int{width}_t {name}(int{width}_t,int{width}_t);\n");
        checks += &format!("if({name}(-101,3)!=-2)return 2;\n");
        traps.push(format!("(void){name}(1,0);"));
        traps.push(format!("(void){name}((int{width}_t){min}ULL,-1);"));
    }
    c += "int main(int argc,char **argv){ if(argc>1){switch(atoi(argv[1])){";
    for (i, call) in traps.iter().enumerate() {
        c += &format!("case {i}: {call} return 0;\n");
    }
    c += "}return 3;}\n";
    c += &checks;
    c += "return 0;}\n";
    let dir = std::env::temp_dir().join(format!("keel-arm-div-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    let oracle = dir.join("oracle.o");
    let out = Command::new("clang")
        .args(["-O1", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(&oracle)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(&ir).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                let (_, object) = compile(&m, SelectionTier::Full, allocation, pools);
                fs::write(dir.join("test.o"), object).unwrap();
                assert!(
                    Command::new("clang")
                        .arg("-O1")
                        .arg(&oracle)
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test"))
                        .output()
                        .unwrap()
                        .status
                        .success()
                );
                let out = Command::new(dir.join("test")).output().unwrap();
                assert!(out.status.success(), "{tier:?} {allocation:?}: {out:?}");
                for case in 0..traps.len() {
                    let out = Command::new(dir.join("test"))
                        .arg(case.to_string())
                        .output()
                        .unwrap();
                    assert_eq!(
                        out.status.signal(),
                        Some(5),
                        "case {case} {tier:?} {allocation:?}: {out:?}"
                    );
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
