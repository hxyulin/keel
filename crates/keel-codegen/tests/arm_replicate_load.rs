//! Exact-width NEON load/splat covers, including native guards and all spills.
#[path = "arm_replicate_load/arm_packed_low_load.rs"]
mod arm_packed_low_load;
#[path = "arm_replicate_load/arm_vector_fusion_boundaries.rs"]
mod arm_vector_fusion_boundaries;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "common/native_oracle.rs"]
mod native_oracle;

use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
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

fn source() -> String {
    let mut src = String::new();
    for (bits, shape) in [(8, "i8x16"), (16, "i16x8"), (32, "i32x4"), (64, "i64x2")] {
        src += &format!(
            "func @rep{bits}(ptr,ptr) {{
block0(v0: ptr,v1: ptr):
 v2 = load.i{bits} v0
 v3 = vsplat.{shape} v2
 store v3,v1
 return
}}\n"
        );
    }
    for (from, to) in [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)] {
        let packed = (128 / to) * from;
        let packed_shape = match packed {
            16 => "i16x8",
            32 => "i32x4",
            64 => "i64x2",
            _ => unreachable!(),
        };
        for op in ["swiden_low", "uwiden_low"] {
            src += &format!(
                "func @{op}_{from}_{to}(ptr,ptr,ptr) {{
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i{packed} v0
 v4 = vsplat.{packed_shape} v3
 v5 = bitcast.i{from}x{source_lanes} v4
 v6 = {op}.i{to}x{dest_lanes} v5
 store v6,v1
 store v4,v2
 return
}}\n",
                source_lanes = 128 / from,
                dest_lanes = 128 / to
            );
        }
    }
    for (from, to) in [(8, 16), (16, 32), (32, 64)] {
        let packed = (128 / to) * from;
        for op in ["swiden_low", "uwiden_low"] {
            for commute in [false, true] {
                let operands = if commute { "v7,v6" } else { "v6,v7" };
                src += &format!(
                    "func @{op}_add_{from}_{to}_{commute}(ptr,ptr,ptr) {{
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i{packed} v0
 v4 = vsplat.i{packed}x{packed_lanes} v3
 v5 = bitcast.i{from}x{source_lanes} v4
 v7 = load.i{to}x{dest_lanes} v1
 v6 = {op}.i{to}x{dest_lanes} v5
 v8 = viadd {operands}
 store v8,v1
 store v4,v2
 return
}}\n",
                    packed_lanes = 128 / packed,
                    source_lanes = 128 / from,
                    dest_lanes = 128 / to
                );
            }
        }
    }
    src
}

fn parse(src: &str) -> keel_ir::Module {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

fn compile_module(
    module: &keel_ir::Module,
    selection: SelectionTier,
    allocation: AllocationTier,
    spilled: bool,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let pools = if spilled {
        &EMPTY
    } else {
        &keel_codegen::aarch64::REGISTERS
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
    let object = assemble_program_object(&program, TargetFeatures::default()).unwrap();
    assert_eq!(assemble_object(&text).unwrap(), object);
    (text, object)
}

#[test]
fn exact_replicates_and_conservative_rejections() {
    let source = source();
    let module = parse(&source);
    let shared_widen = parse(&source.replace(" store v8,v1", " store v6,v2\n store v8,v1"));
    let simple = source.split("func @swiden_low").next().unwrap();
    let volatile = parse(
        &simple
            .replace("load.i8 ", "load.i8 volatile ")
            .replace("load.i16 ", "load.i16 volatile ")
            .replace("load.i32 ", "load.i32 volatile ")
            .replace("load.i64 ", "load.i64 volatile "),
    );
    let shared = parse(&simple.replace(" store v3,v1", " store v2,v1\n store v3,v1"));
    let barrier = parse(&simple.replace(" v3 = vsplat", " store v1,v1\n v3 = vsplat"));
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (text, _) = compile_module(&module, selection, allocation, spilled);
                assert_eq!(text.matches("ld1r\t").count(), 28);
                assert_eq!(text.matches("saddw\t").count(), 6);
                assert_eq!(text.matches("uaddw\t").count(), 6);
                let shared_text = compile_module(&shared_widen, selection, allocation, spilled).0;
                assert!(!shared_text.contains("saddw\t") && !shared_text.contains("uaddw\t"));
                assert!(
                    !compile_module(&volatile, selection, allocation, spilled)
                        .0
                        .contains("ld1r\t")
                );
                assert!(
                    !compile_module(&shared, selection, allocation, spilled)
                        .0
                        .contains("ld1r\t")
                );
                // A shared source and intervening memory operation must not move.
                assert!(
                    !compile_module(&barrier, selection, allocation, spilled)
                        .0
                        .contains("ld1r\t")
                );
            }
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_exact_guarded_reads_aliases_and_spilled_values() {
    use native_oracle::PreparedOracle;
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n#include <stdio.h>\n",
    );
    let mut checks = String::new();
    for bits in [8, 16, 32, 64] {
        let bytes = bits / 8;
        harness += &format!("extern void rep{bits}(const void*,void*);\n");
        checks += &format!("for(unsigned pattern=0;pattern<257;pattern++) for(unsigned offset=0;offset<3;offset++) {{
 unsigned char *src=end-{bytes}-offset, out[18]; uint{bits}_t v=(uint{bits}_t)(0x9e3779b97f4a7c15ULL*pattern); memcpy(src,&v,{bytes}); memset(out,0xa5,sizeof out);
 rep{bits}(src,out+1); if(out[0]!=0xa5||out[17]!=0xa5) return 1;
 for(unsigned lane=0;lane<16/{bytes};lane++) if(memcmp(out+1+lane*{bytes},&v,{bytes})) return 2;
 unsigned char alias[18]; memset(alias,0xa5,sizeof alias); memcpy(alias+1,&v,{bytes}); rep{bits}(alias+1,alias+1);
 if(alias[0]!=0xa5||alias[17]!=0xa5) return 3;
 for(unsigned lane=0;lane<16/{bytes};lane++) if(memcmp(alias+1+lane*{bytes},&v,{bytes})) return 4;
 }}\n");
    }
    for (from, to) in [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)] {
        let packed_bytes = (128 / to) * from / 8;
        for (op, scalar) in [
            ("swiden_low", format!("int{from}_t")),
            ("uwiden_low", format!("uint{from}_t")),
        ] {
            harness += &format!("extern void {op}_{from}_{to}(const void*,void*,void*);\n");
            checks += &format!("for(unsigned pattern=0;pattern<257;pattern++) for(unsigned offset=0;offset<3;offset++) {{
 unsigned char *src=end-{packed_bytes}-offset, out[18], live[18], original[8];
 for(unsigned i=0;i<{packed_bytes};i++) src[i]=(unsigned char)(pattern+i*31); memcpy(original,src,{packed_bytes});
 memset(out,0xa5,sizeof out); memset(live,0xa5,sizeof live);
 {op}_{from}_{to}(src,out+1,live+1); if(out[0]!=0xa5||out[17]!=0xa5||live[0]!=0xa5||live[17]!=0xa5) return 6;
 for(unsigned i=0;i<16;i++) if(live[i+1]!=original[i%{packed_bytes}]) return 7;
 for(unsigned i=0;i<{lanes};i++) {{ {scalar} v; uint{to}_t actual; memcpy(&v,original+i*sizeof v,sizeof v); memcpy(&actual,out+1+i*sizeof actual,sizeof actual); if(actual!=(uint{to}_t)v)return 8; }}
 unsigned char alias[18]; memcpy(alias+1,original,{packed_bytes});
 {op}_{from}_{to}(alias+1,alias+1,live+1); if(memcmp(alias+1,out+1,16))return 9;
 }}\n",lanes=128/to);
        }
    }
    for (from, to) in [(8, 16), (16, 32), (32, 64)] {
        let packed_bytes = (128 / to) * from / 8;
        for (op, scalar) in [
            ("swiden_low", format!("int{from}_t")),
            ("uwiden_low", format!("uint{from}_t")),
        ] {
            for commute in [false, true] {
                let name = format!("{op}_add_{from}_{to}_{commute}");
                harness += &format!("extern void {name}(const void*,void*,void*);\n");
                checks += &format!("for(unsigned pattern=0;pattern<257;pattern++) for(unsigned offset=0;offset<3;offset++) {{
 unsigned char *src=end-{packed_bytes}-offset, out[18], live[18], original[8], expected[16];
 for(unsigned i=0;i<{packed_bytes};i++) src[i]=(unsigned char)(pattern+i*31); memcpy(original,src,{packed_bytes});
 memset(out,0xa5,sizeof out); memset(live,0xa5,sizeof live);
 for(unsigned i=0;i<{lanes};i++) {{ uint{to}_t acc=(uint{to}_t)(UINT64_MAX-(pattern*31+i)); {scalar} v; memcpy(&v,original+i*sizeof v,sizeof v); uint{to}_t sum=(uint{to}_t)(acc+(uint{to}_t)v); memcpy(out+1+i*sizeof acc,&acc,sizeof acc); memcpy(expected+i*sizeof sum,&sum,sizeof sum); }}
 {name}(src,out+1,live+1); if(out[0]!=0xa5||out[17]!=0xa5||live[0]!=0xa5||live[17]!=0xa5)return 10;
 if(memcmp(out+1,expected,16))return 11;
 for(unsigned i=0;i<16;i++)if(live[i+1]!=original[i%{packed_bytes}])return 12;
 }}\n",lanes=128/to);
            }
        }
    }
    harness += &format!(
        "int main(void){{ size_t n=sysconf(_SC_PAGESIZE); unsigned char *p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0); if(p==MAP_FAILED||mprotect(p+n,n,PROT_NONE))return 5; unsigned char *end=p+n; {checks} puts(\"exact replicate guard/alias/spill oracle passed\"); return 0; }}"
    );
    let oracle = PreparedOracle::new("replicate", &harness, &["-O2"]);
    let module = parse(&source());
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (_, object) = compile_module(&module, selection, allocation, spilled);
                oracle.run(
                    &object,
                    &format!("{selection:?}/{allocation:?}/spilled={spilled}"),
                );
            }
        }
    }
}
