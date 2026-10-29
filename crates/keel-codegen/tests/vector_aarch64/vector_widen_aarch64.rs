//! Packed low-lane widening and exact-footprint mixed-width reductions.
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use crate::native_oracle;
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{Type, VectorType};
use keel_opt::VectorTarget;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use native_oracle::PreparedOracle;
use std::fs;

const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};

fn fixture() -> String {
    let mut text = String::new();
    for (from, to) in [
        (Type::I8, Type::I16),
        (Type::I8, Type::I32),
        (Type::I8, Type::I64),
        (Type::I16, Type::I32),
        (Type::I16, Type::I64),
        (Type::I32, Type::I64),
    ] {
        let a = VectorType::ALL_128
            .into_iter()
            .find(|s| s.lane_type() == from)
            .unwrap();
        let b = VectorType::ALL_128
            .into_iter()
            .find(|s| s.lane_type() == to)
            .unwrap();
        for op in ["swiden_low", "uwiden_low"] {
            text += &format!(
                "func @{op}_{from}_{to}(ptr,ptr) {{
block0(v0: ptr,v1: ptr):
 v2 = load.{a} v0
 v3 = {op}.{b} v2
 store v3,v1
 return
}}\n"
            );
        }
    }
    text
}

fn arm(module: &keel_ir::Module, allocation: AllocationTier, tiny: bool) -> keel_asm_ir::Program {
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &allocation,
    };
    let pools = if tiny {
        &TINY
    } else {
        &keel_codegen::aarch64::REGISTERS
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    assert_eq!(
        assemble_object(&text).unwrap(),
        assemble_program_object(&program, TargetFeatures::default()).unwrap()
    );
    assert!(text.contains("sshll\tv") && text.contains("ushll\tv"));
    if tiny && module.functions.iter().any(|f| f.name.starts_with("sum_")) {
        assert!(
            text.lines()
                .any(|line| line.contains("str\tq")
                    && (line.contains("[sp") || line.contains("[x29"))),
            "expected forced vector spills"
        );
    }
    if let Some(dir) = std::env::var_os("KEEL_WIDEN_EVIDENCE") {
        let dir = std::path::PathBuf::from(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{:?}-tiny{tiny}.s", allocation)), &text).unwrap();
    }
    program
}

#[test]
fn low_lane_widening_validates_roundtrips_and_emits_on_both_targets() {
    let source = fixture();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&module.to_string()).unwrap()).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            arm(&module, allocation, tiny);
        }
        // The shared legalizer supplies an exact scalar fallback on SSE2.
        // This emission gate deliberately needs no x86 selector/API changes.
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &allocation,
        };
        let program = keel_codegen::x86_64::compile_program_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &pipeline,
        )
        .unwrap();
        assert_eq!(program.stats().text_bytes, 0);
        keel_asm::x86::assemble_program_object(&program).unwrap();
    }
    for (from, to) in [
        ("i32x4", "i16x8"),
        ("i32x4", "i32x4"),
        ("f32x4", "i64x2"),
        ("i16", "i32x4"),
        ("i8x16", "i64"),
    ] {
        let bad = format!(
            "func @bad({from}) -> {to} {{\nblock0(v0: {from}):\n v1 = swiden_low.{to} v0\n return v1\n}}"
        );
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
}

fn reduction(from: Type, to: Type, signed: bool) -> String {
    let op = if signed { "sext" } else { "uext" };
    let bytes = from.bytes().unwrap();
    let source = format!(
        "func @sum_{op}_{from}_{to}(ptr,i64,{to}) -> {to} {{
block0(v0: ptr,v1: i64,v2: {to}):
 v3 = iconst.i64 0
 jump block1(v3,v2)
block1(v4: i64,v5: {to}):
 v6 = icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7 = iconst.i64 {bytes}
 v8 = imul v4,v7
 v9 = ptradd v0,v8
 v10 = load.{from} v9
 v11 = {op}.{to} v10
 v12 = iadd v5,v11
 v13 = iconst.i64 1
 v14 = iadd v4,v13
 jump block1(v14,v12)
block3:
 return v5
}}\n"
    );
    if bytes == 1 {
        source
            .replace(" v8 = imul v4,v7\n", "")
            .replace("ptradd v0,v8", "ptradd v0,v4")
    } else {
        source
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn widening_and_mixed_width_reductions_execute_edges_guard_pages_and_spills() {
    let mut source = fixture();
    let pairs = [
        (Type::I8, Type::I32),
        (Type::I8, Type::I64),
        (Type::I16, Type::I32),
        (Type::I16, Type::I64),
        (Type::I32, Type::I64),
    ];
    let mut harness = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut checks = String::new();
    for (from, to) in pairs {
        for signed in [false, true] {
            source += &reduction(from, to, signed);
            let op = if signed { "sext" } else { "uext" };
            let f = format!("sum_{op}_{from}_{to}");
            let src = format!(
                "{}int{}_t",
                if signed { "" } else { "u" },
                from.bytes().unwrap() * 8
            );
            let dst = format!("uint{}_t", to.bytes().unwrap() * 8);
            let sign_bit = from.bytes().unwrap() * 8 - 1;
            harness += &format!("extern {dst} {f}(const void*,uint64_t,{dst});\n");
            checks += &format!("{{ for (unsigned n=0;n<97;n++) for(unsigned offset=0;offset<3;offset++) for(unsigned pattern=0;pattern<5;pattern++) {{
 unsigned char *p=end-n*sizeof({src})-offset;
 for(unsigned i=0;i<n;i++) {{ uint64_t bits=pattern==0?0:pattern==1?UINT64_MAX:pattern==2?(1ULL<<{sign_bit}):pattern==3?((1ULL<<{sign_bit})-1):(uint64_t)(i*0x9e3779b9U+17); {src} v=({src})bits; memcpy(p+i*sizeof(v),&v,sizeof(v)); }}
 {dst} expected=({dst})-19;
 for(unsigned i=0;i<n;i++) {{ {src} v; memcpy(&v,p+i*sizeof(v),sizeof(v)); expected+=({dst})v; }}
 {dst} actual={f}(p,n,({dst})-19);
 if(actual!=expected) {{ fprintf(stderr,\"{f} n=%u offset=%u pattern=%u got=%llu expected=%llu\\n\",n,offset,pattern,(unsigned long long)actual,(unsigned long long)expected); return 1; }}
 }} if({f}(end,0,({dst})-19)!=({dst})-19) return 2; }}\n");
        }
    }
    for (from, to) in [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)] {
        for signed in [false, true] {
            let op = if signed { "swiden_low" } else { "uwiden_low" };
            let name = format!("{op}_i{from}_i{to}");
            let scalar = format!("{}int{from}_t", if signed { "" } else { "u" });
            harness += &format!("extern void {name}(const void*,void*);\n");
            checks += &format!("{{ unsigned char input[17],output[17]; for(unsigned pattern=0;pattern<257;pattern++) {{
             for(unsigned i=0;i<16;i++) input[i+1]=(unsigned char)(pattern+i*31);
             memset(output,0x5a,sizeof(output)); {name}(input+1,output+1);
             if(output[0]!=0x5a) return 4;
             for(unsigned i=0;i<{lanes};i++) {{ {scalar} v; uint{to}_t actual; memcpy(&v,input+1+i*sizeof(v),sizeof(v)); memcpy(&actual,output+1+i*sizeof(actual),sizeof(actual)); if(actual!=(uint{to}_t)v) return 5; }}
             }} }}\n", lanes=128/to);
        }
    }
    checks +=
        "puts(\"mixed-width: guard/tail/extreme/wrapping and low-lane oracle passed\"); return 0;";
    harness += &format!(
        "int main(void) {{ size_t size=(size_t)sysconf(_SC_PAGESIZE); unsigned char *p=mmap(0,2*size,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0); if(p==MAP_FAILED||mprotect(p+size,size,PROT_NONE)) return 3; unsigned char *end=p+size; {checks} }}"
    );
    let mut module = keel_ir::parse(&source).unwrap();
    keel_opt::run_passes_for_target(&mut module, &["loop-reduce"], VectorTarget::Aarch64Neon)
        .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(module.to_string().contains("swiden_low.i64x2"));
    for f in &module.functions {
        if f.name.starts_with("sum_") {
            assert!(
                f.layout_insts().any(|(_, i)| matches!(
                    f.inst_data(i),
                    keel_ir::InstData::Convert {
                        op: keel_ir::ConvertOp::SwidenLow | keel_ir::ConvertOp::UwidenLow,
                        ..
                    }
                )),
                "reduction did not vectorize: {}",
                f.name
            );
        }
    }
    let oracle = PreparedOracle::new("widen-arm", &harness, &["-O2"]);
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let program = arm(&module, allocation, tiny);
            oracle.run(
                &assemble_program_object(&program, TargetFeatures::default()).unwrap(),
                &format!("{allocation:?} tiny={tiny}"),
            );
        }
    }
}
