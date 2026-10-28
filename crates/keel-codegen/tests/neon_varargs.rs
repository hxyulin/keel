use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
#[test]
fn apple_mixed_d_q_vector_varargs() {
    let source = "decl @external(i64, ...) -> i8x16\nfunc @read(ptr, ...) {\n ss0 = stack_slot 8, align 8\nblock0(v0: ptr):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.i64 v1\n v3 = va_arg.i8x8 v1\n v4 = va_arg.i8x16 v1\n v5 = va_arg.i8x8 v1\n v6 = va_arg.i8x16 v1\n store v2,v0\n v7 = iconst.i64 8\n v8 = ptradd v0,v7\n store v3,v8\n v9 = iconst.i64 16\n v10 = ptradd v0,v9\n store v4,v10\n v11 = iconst.i64 32\n v12 = ptradd v0,v11\n store v5,v12\n v13 = iconst.i64 40\n v14 = ptradd v0,v13\n store v6,v14\n return\n}\nfunc @send(i8x8,i8x16) -> i8x16 {\nblock0(v0: i8x8,v1: i8x16):\n v2 = iconst.i64 7\n v3 = call @external(v2,v2,v0,v1,v0,v1)\n return v3\n}\n";
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-varargs");
    fs::write(dir.join("test.c"),r#"
#include <stdint.h>
#include <stdarg.h>
#include <string.h>
#include <assert.h>
typedef uint8_t D __attribute__((vector_size(8)));
typedef uint8_t Q __attribute__((vector_size(16)));
extern void read(void*,...);extern Q send(D,Q);
Q external(uint64_t tag,...){va_list a;va_start(a,tag);assert(va_arg(a,uint64_t)==7);D d=va_arg(a,D);Q q=va_arg(a,Q);D e=va_arg(a,D);Q r=va_arg(a,Q);va_end(a);assert(!memcmp(&d,&e,8));assert(!memcmp(&q,&r,16));return r;}
int main(void){D d={1,2,3,4,5,6,7,8};Q q={1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16};unsigned char out[56];read(out,(uint64_t)7,d,q,d,q);uint64_t x;memcpy(&x,out,8);assert(x==7);assert(!memcmp(out+8,&d,8));assert(!memcmp(out+16,&q,16));assert(!memcmp(out+32,&d,8));assert(!memcmp(out+40,&q,16));Q r=send(d,q);assert(!memcmp(&q,&r,16));}
"#).unwrap();
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let program = keel_codegen::aarch64::compile_program_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let text = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let bytes = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(bytes, keel_asm::assemble_object(&text).unwrap());
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                fs::write(dir.join("test.o"), bytes).unwrap();
                common::command(
                    Command::new("clang")
                        .arg("-O2")
                        .arg(dir.join("test.c"))
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test")),
                );
                common::command(&mut Command::new(dir.join("test")));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
