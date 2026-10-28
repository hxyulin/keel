use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::VectorType;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::{fs, process::Command};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;

const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};

fn source() -> String {
    let mut source = String::new();
    for shape in VectorType::ALL_64 {
        let lane = shape.lane_type();
        let op = if lane.is_float() { "vfadd" } else { "viadd" };
        source += &format!(
            "func @add_{shape}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} v0\n v4 = load.{shape} v1\n v5 = {op} v3,v4\n store v5,v2\n return\n}}\n"
        );
        source += &format!(
            "func @identity_{shape}({shape}) -> {shape} {{\nblock0(v0: {shape}):\n return v0\n}}\n"
        );
        let binary: &[&str] = if lane.is_float() {
            &[
                "vfadd", "vfsub", "vfmul", "vfdiv", "vfmin", "vfmax", "vfpadd", "vfpmin", "vfpmax",
            ]
        } else if lane.bytes() == Some(8) {
            &["vand", "vor", "vxor"]
        } else {
            &[
                "viadd", "visub", "vimul", "vand", "vor", "vxor", "vpadd", "vsmin", "vsmax",
                "vumin", "vumax", "vsabd", "vuabd", "vshadd", "vuhadd", "vsrhadd", "vurhadd",
                "vshsub", "vuhsub",
            ]
        };
        let unary: &[&str] = if lane.is_float() {
            &[
                "vfneg",
                "vfabs",
                "vfsqrt",
                "vfceil",
                "vffloor",
                "vftrunc",
                "vfnearest",
            ]
        } else if lane.bytes() == Some(8) {
            &["vbnot", "vbswap"]
        } else if lane.bytes() == Some(1) {
            &["vineg", "vbnot", "vclz", "vpopcnt"]
        } else {
            &["vineg", "vbnot", "vclz", "vbswap"]
        };
        for op in binary.iter().copied().chain(unary.iter().copied()) {
            let args = if binary.contains(&op) { ",v4" } else { "" };
            source += &format!(
                "func @native_{shape}_{op}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} v0\n v4 = load.{shape} v1\n v5 = {op} v3{args}\n store v5,v2\n return\n}}\n"
            );
        }
    }
    for shape in VectorType::ALL_64 {
        let types = std::iter::repeat_n(shape.to_string(), 10)
            .collect::<Vec<_>>()
            .join(",");
        let params = (0..10)
            .map(|i| format!("v{i}: {shape}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = (0..10)
            .rev()
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(",");
        source += &format!(
            "decl @external_{shape}({types}) -> {shape}\nfunc @forward_{shape}({types}) -> {shape} {{\nblock0({params}):\n v10 = call @external_{shape}({args})\n return v10\n}}\n"
        );
    }
    source += "func @spill_i8x8(ptr,ptr) {\nblock0(v0: ptr,v1: ptr):\n";
    for i in 0..10 {
        let offset = 2 + i * 3;
        source += &format!(
            " v{offset} = iconst.i64 {}\n v{} = ptradd v0,v{offset}\n v{} = load.i8x8 v{}\n",
            i * 8,
            offset + 1,
            offset + 2,
            offset + 1
        );
    }
    let mut acc = 4;
    for i in 1..10 {
        let next = 2 + i * 3 + 2;
        let result = 32 + i;
        source += &format!(" v{result} = viadd v{acc},v{next}\n");
        acc = result;
    }
    source += &format!(" store v{acc},v1\n return\n}}\n");
    source += "func @hva_d(ptr sret {i8x8, i8x8}, ptr byval {i8x8, i8x8}) {\nblock0(v0: ptr,v1: ptr):\n v2 = load.i8x8 v1\n v3 = iconst.i64 8\n v4 = ptradd v1,v3\n v5 = load.i8x8 v4\n v6 = viadd v2,v2\n v7 = viadd v5,v5\n store v6,v0\n v8 = ptradd v0,v3\n store v7,v8\n return\n}\n";
    source += "func @stack_d(i8x8,i8x8,i8x8,i8x8,i8x8,i8x8,i8x8,i8x8,i8x8) -> i8x8 {\nblock0(v0: i8x8,v1: i8x8,v2: i8x8,v3: i8x8,v4: i8x8,v5: i8x8,v6: i8x8,v7: i8x8,v8: i8x8):\n return v8\n}\n";
    source
}

#[test]
fn d_values_use_exact_width_memory_and_both_object_routes() {
    let module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocator,
            };
            let text =
                keel_codegen::aarch64::compile_with_pipeline(&module, &TINY, &pipeline).unwrap();
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&module, &TINY, &pipeline)
                    .unwrap();
            assert_eq!(program.stats().text_bytes, 0);
            assert_eq!(
                assemble_program_object(&program, TargetFeatures::default()).unwrap(),
                assemble_object(&text).unwrap()
            );
            assert!(text.contains("ldr\td"), "{text}");
            assert!(text.contains("str\td"), "{text}");
            assert!(text.contains("add\tv") && text.contains(".8b"), "{text}");
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn d_values_execute_at_guard_page_and_cross_apple_abi() {
    let dir = common::temp("neon-d");
    let c = dir.join("test.c");
    fs::write(&c, r#"
#include <assert.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
typedef uint8_t U8 __attribute__((vector_size(8)));
typedef uint16_t U16 __attribute__((vector_size(8)));
typedef uint32_t U32 __attribute__((vector_size(8)));
typedef uint64_t U64 __attribute__((vector_size(8)));
typedef float F32 __attribute__((vector_size(8)));
typedef double F64 __attribute__((vector_size(8)));
extern void add_i8x8(const void*,const void*,void*);
extern void add_i16x4(const void*,const void*,void*);
extern void add_i32x2(const void*,const void*,void*);
extern void add_i64x1(const void*,const void*,void*);
extern void add_f32x2(const void*,const void*,void*);
extern void add_f64x1(const void*,const void*,void*);
#define FORWARD(T,S) \
T external_##S(T a,T b,T c,T d,T e,T f,T g,T h,T i,T j) { return a+b+c+d+e+f+g+h+i+j; } \
extern T forward_##S(T,T,T,T,T,T,T,T,T,T);
FORWARD(U8,i8x8) FORWARD(U16,i16x4) FORWARD(U32,i32x2) FORWARD(U64,i64x1)
FORWARD(F32,f32x2) FORWARD(F64,f64x1)
extern U8 identity_i8x8(U8);
extern U16 identity_i16x4(U16);
extern U32 identity_i32x2(U32);
extern U64 identity_i64x1(U64);
extern F32 identity_f32x2(F32);
extern F64 identity_f64x1(F64);
extern void spill_i8x8(const void*,void*);
typedef struct { U8 a,b; } Pair;
extern Pair hva_d(Pair);
extern U8 stack_d(U8,U8,U8,U8,U8,U8,U8,U8,U8);
static void *edge(void) {
  long page = sysconf(_SC_PAGESIZE);
  char *p = mmap(0, (size_t)page*2, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0);
  assert(p != MAP_FAILED && mprotect(p+page, (size_t)page, PROT_NONE) == 0);
  return p+page-8;
}


int main(void) {
  uint8_t *a=edge(), *b=edge(), out[8];
  void (*integer[])(const void*,const void*,void*) = {
    add_i8x8,add_i16x4,add_i32x2,add_i64x1
  };
  memset(a,0x11,8); memset(b,0x22,8);
  for (int i=0;i<4;i++) {
    memset(out,0xA5,8); integer[i](a,b,out);
    for (int j=0;j<8;j++) assert(out[j]==0x33);
  }
  float af[2]={1.25f,-3.5f}, bf[2]={2.0f,0.5f}, rf[2];
  memcpy(a,af,8); memcpy(b,bf,8); add_f32x2(a,b,out);
  memcpy(rf,out,8); assert(rf[0]==3.25f && rf[1]==-3.0f);
  double ad=1.25,bd=2.0,rd;
  memcpy(a,&ad,8); memcpy(b,&bd,8); add_f64x1(a,b,out);
  memcpy(&rd,out,8); assert(rd==3.25);
  U8 u8={1,2,3,4,5,6,7,8}; U8 r8=identity_i8x8(u8);
  assert(memcmp(&u8,&r8,8)==0);
  U16 u16={1,2,3,4}; U16 r16=identity_i16x4(u16);
  assert(memcmp(&u16,&r16,8)==0);
  U32 u32={1,2}; U32 r32=identity_i32x2(u32);
  assert(memcmp(&u32,&r32,8)==0);
  U64 u64={0x0123456789abcdefULL}; U64 r64=identity_i64x1(u64);
  assert(memcmp(&u64,&r64,8)==0);
  F32 f32={1.25f,-3.5f}; F32 q32=identity_f32x2(f32);
  assert(memcmp(&f32,&q32,8)==0);
  F64 f64={1.25}; F64 q64=identity_f64x1(f64);
  assert(memcmp(&f64,&q64,8)==0);
#define CHECK(T,S,V) { T r=forward_##S(V,V,V,V,V,V,V,V,V,V); T e=V+V+V+V+V+V+V+V+V+V; assert(memcmp(&r,&e,8)==0); }
  CHECK(U8,i8x8,u8) CHECK(U16,i16x4,u16) CHECK(U32,i32x2,u32) CHECK(U64,i64x1,u64)
  CHECK(F32,f32x2,f32) CHECK(F64,f64x1,f64)
  // Exact output widths and in-place aliases at protected boundaries.
  memset(a,0x11,8);memset(b,0x22,8);add_i8x8(a,b,a);
  for(int i=0;i<8;i++)assert(a[i]==0x33);
  add_i8x8(a,b,b);for(int i=0;i<8;i++)assert(b[i]==0x55);
  uint8_t values[80], spill_out[8];
  for (int i=0;i<10;i++) memset(values+8*i,i+1,8);
  spill_i8x8(values,spill_out);
  for (int i=0;i<8;i++) assert(spill_out[i]==55);
  Pair pair={u8,u8}, doubled=hva_d(pair);
  for (int i=0;i<8;i++) assert(doubled.a[i]==2*u8[i] && doubled.b[i]==2*u8[i]);
  U8 ninth={9,8,7,6,5,4,3,2};
  U8 stack=stack_d(u8,u8,u8,u8,u8,u8,u8,u8,ninth);
  assert(memcmp(&stack,&ninth,8)==0);
  return 0;
}
"#).unwrap();
    let module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&module, &TINY, &pipeline)
                    .unwrap();
            let object = dir.join("test.o");
            fs::write(
                &object,
                assemble_program_object(&program, TargetFeatures::default()).unwrap(),
            )
            .unwrap();
            let exe = dir.join("test");
            common::command(
                Command::new("clang")
                    .args(["-O2", "-ffp-contract=off"])
                    .arg(&c)
                    .arg(&object)
                    .arg("-o")
                    .arg(&exe),
            );
            common::command(&mut Command::new(&exe));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn d_text_and_typed_instruction_bytes_match_clang() {
    use object::{Object, ObjectSection};
    let mut ir = source();
    for &op in keel_ir::NeonIntrinsic::ALL {
        if op.memory().is_some() {
            continue;
        }
        for shape in VectorType::ALL_64 {
            let ty = keel_ir::Type::Vector(shape);
            let Some(inputs) = op.operand_types(ty) else {
                continue;
            };
            let mut args = Vec::new();
            ir += &format!("func @{op}_{shape}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n");
            for (i, input) in inputs.iter().enumerate() {
                let n = 2 + i;
                args.push(format!("v{n}"));
                if *input == keel_ir::Type::I32 {
                    ir += &format!(
                        " v{n} = iconst.i32 {}\n",
                        op.immediate_range(ty).unwrap().start()
                    );
                } else {
                    ir += &format!(" v{n} = load.{input} v0\n");
                }
            }
            ir += &format!(
                " v20 = {op}.{shape}({})\n store v20,v1\n return\n}}\n",
                args.join(",")
            );
        }
    }
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Baseline,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&module, &TINY, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, &TINY, &pipeline).unwrap();
    let dir = common::temp("neon-d-clang");
    let input = dir.join("test.s");
    let output = dir.join("clang.o");
    fs::write(&input, text).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&output),
    );
    let bytes = fs::read(output).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let clang = file.section_by_name("__text").unwrap().data().unwrap();
    assert_eq!(
        keel_asm::assemble_program(&program, TargetFeatures::default())
            .unwrap()
            .text,
        clang
    );
    fs::remove_dir_all(dir).unwrap();
}
