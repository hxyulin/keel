//! SIMD asm operands retain their full width through staging and spills.
#[path = "common/native_oracle.rs"]
mod native_oracle;
use native_oracle::PreparedOracle;

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
    let mut source = String::new();
    for (name, ty, template, constraints) in [
        ("barrier", "i32x4", "/*%q0*/", "=w,0,~{memory},~{cc}"),
        (
            "fixed_barrier",
            "i32x4",
            "/*%v0*/",
            "={v0},0,~{memory},~{cc}",
        ),
        (
            "invert",
            "i32x4",
            "not %v0.16b, %v1.16b",
            "=w,w,~{memory},~{cc}",
        ),
        (
            "fixed_invert",
            "i32x4",
            "not %v0.16b, %v1.16b",
            "={v0},{v1},~{memory},~{cc}",
        ),
        (
            "tied_invert",
            "i32x4",
            "not %v0.16b, %v1.16b",
            "=w,0,~{memory},~{cc}",
        ),
        ("narrow_invert", "i8x8", "not %v0.8b, %v1.8b", "=w,0"),
        ("float_barrier", "f64x2", "/*%q0*/", "=w,0"),
    ] {
        source += &format!(
            "func @{name}(ptr,ptr,ptr) {{
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = load.{ty} v0
 v4 = asm \"{template}\", \"{constraints}\"(v3) -> {ty}
 store v4,v1
 store v3,v2
 return
}}\n"
        );
    }
    source += "func @barrier_value(i32x4) -> i32x4 {
block0(v0:i32x4):
 v1 = asm \"/*%q0*/\", \"=w,0,~{memory},~{cc}\"(v0) -> i32x4
 return v1
}
func @unused_barrier(i32x4) {
block0(v0:i32x4):
 v1 = asm \"/*unused %v0*/\", \"=w,0,~{memory},~{cc}\"(v0) -> i32x4
 return
}\n";
    source
}

fn compile(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    spilled: bool,
) -> (String, Vec<u8>) {
    compile_module(&parse(source), selection, allocation, spilled)
}

fn parse(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
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
fn vector_operands_assemble_and_execute_without_losing_high_lanes() {
    let source = source();
    // Exercise the barrier through optimization too: its opaque result and
    // original input remain distinct, and the asm must not disappear.
    let mut module = keel_ir::parse(&source).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let optimized = module.to_string();
    assert_eq!(optimized.matches("asm ").count(), 9);
    let module = parse(&(optimized + &large_spills()));
    let oracle = PreparedOracle::new("vector-asm", HARNESS, &["-O2"]);
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for spilled in [false, true] {
            let (text, object) = compile_module(&module, selection, AllocationTier::Full, spilled);
            assert!(text.contains("/*q0*/"), "{text}");
            assert!(text.contains("/*v0*/"), "{text}");
            assert!(text.contains("/*unused v0*/"), "{text}");
            oracle.run(&object, &format!("{selection:?}/spilled={spilled}"));
        }
    }
}

const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#define DECL(n) extern void n(const void*,void*,void*);
DECL(barrier) DECL(fixed_barrier) DECL(invert) DECL(fixed_invert)
DECL(tied_invert) DECL(narrow_invert) DECL(float_barrier)
typedef int32_t vec4 __attribute__((vector_size(16)));
extern vec4 barrier_value(vec4);
extern uint64_t spill_input(const void*,uint64_t);
extern uint64_t spill_output(void*,uint64_t);
int main(void) {
 uint64_t words[2]={11,0xfedcba9876543210ULL};
 if(spill_input(words,5)!=11+5+31*11)return 4;
 if(spill_output(words,5)!=7+5+30*11)return 5;
 if(words[0]!=11||words[1]!=0xfedcba9876543210ULL)return 6;
 vec4 value={1,-2,0x12345678,-1234567};
 vec4 result=barrier_value(value);
 if(memcmp(&result,&value,16))return 3;
 void (*run[])(const void*,void*,void*)={barrier,fixed_barrier,invert,fixed_invert,tied_invert,narrow_invert,float_barrier};
 for(unsigned f=0;f<7;f++) for(unsigned p=0;p<17;p++) {
  unsigned char input[16],out[16],live[16];
  for(unsigned i=0;i<16;i++)input[i]=(unsigned char)(p*71+i*19);
  memset(out,0xa5,16); memset(live,0xa5,16);
  run[f](input,out,live);
  unsigned size=f==5?8:16;
  for(unsigned i=0;i<size;i++) {
   unsigned char expected=f>=2&&f<=5?(unsigned char)~input[i]:input[i];
   if(out[i]!=expected||live[i]!=input[i])return 1;
  }
  for(unsigned i=size;i<16;i++)if(out[i]!=0xa5||live[i]!=0xa5)return 2;
 }
 return 0;
}
"#;

#[test]
fn simd_modifier_rejects_general_registers_and_keeps_scalar_support() {
    let source = "func @f(i64) -> i64 {
block0(v0:i64):
 v1 = asm \"/*%v0*/\", \"=r,0\"(v0) -> i64
 return v1
}";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let error = keel_codegen::aarch64::compile(&module).unwrap_err();
    assert!(
        error.to_string().contains("does not fit operand 0"),
        "{error}"
    );
    for ty in ["f32", "f64"] {
        let scalar = source.replace("i64", ty).replace("=r,0", "=w,0");
        compile(&scalar, SelectionTier::Full, AllocationTier::Full, false);
    }
}

fn large_spills() -> String {
    let mut source = String::new();
    for (name, count, output) in [("spill_input", 31, false), ("spill_output", 30, true)] {
        source += &format!("func @{name}(ptr,i64) -> i64 {{\nblock0(v0:ptr,v1:i64):\n");
        for v in 2..count + 2 {
            source += &format!(" v{v} = load.i64 v0\n");
        }
        let vector = count + 2;
        source += &format!(" v{vector} = load.i32x4 v0\n");
        let mut result = vector + 1;
        if output {
            source += &format!(
                " v{result},v{} = asm \"mov %1,#7\", \"=w,={{x17}},0\"(v{vector}) -> (i32x4,i64)\n store v{result},v0\n",
                result + 1
            );
            result += 1;
        } else {
            source += &format!(
                " v{result} = asm \"umov %0, %v2.d[0]\\n add %0, %0, %1\", \"=r,{{x17}},w\"(v1,v{vector}) -> i64\n"
            );
        }
        for v in (2..count + 2).chain(output.then_some(1)) {
            source += &format!(" v{} = iadd v{result},v{v}\n", result + 1);
            result += 1;
        }
        source += &format!(" return v{result}\n}}\n");
    }
    source
}

#[test]
fn matching_initializers_and_early_clobbers_obey_constraints() {
    for ty in ["i64", "i32x4"] {
        let reg = if ty == "i64" { "x0" } else { "v0" };
        let class = if ty == "i64" { "r" } else { "w" };
        for constraints in [
            format!("={class},0,0"),
            format!("={{{reg}}},{{{reg}}},{{{reg}}}"),
        ] {
            let source = format!(
                "func @f({ty}) -> {ty} {{\nblock0(v0:{ty}):\n v1 = asm \"\", \"{constraints}\"(v0,v0) -> {ty}\n return v1\n}}"
            );
            compile(&source, SelectionTier::Full, AllocationTier::Full, true);
        }
        for input in [format!("{{{reg}}}"), "0".into()] {
            let source = format!(
                "func @f({ty}) -> {ty} {{\nblock0(v0:{ty}):\n v1 = asm \"\", \"=&{{{reg}}},{input}\"(v0) -> {ty}\n return v1\n}}"
            );
            let module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let result = keel_codegen::aarch64::compile(&module);
            if input == "0" {
                result.unwrap();
            } else {
                assert!(result.unwrap_err().to_string().contains("early-clobber"));
            }
        }
    }
}
