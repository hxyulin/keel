//! Exhaustive lane indices and source choices, with independent byte-copy oracle.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::VectorType;
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut main = String::from(
        "int main(void){unsigned char a[16],b[16],r[48],e[16]; for(unsigned k=0;k<256;k++){for(unsigned i=0;i<16;i++){a[i]=(unsigned char)(k+17*i);b[i]=(unsigned char)(255-k-13*i);}\n",
    );
    for shape in VectorType::ALL_128 {
        let n = shape.lanes();
        let bytes = shape.lane_type().bytes().unwrap();
        for lane in 0..n {
            // Every source index is observed at every output lane over this matrix.
            for offset in 0..2 * n {
                let name = format!("shuffle_{shape}_{lane}_{offset}");
                let indices: Vec<_> = (0..n)
                    .map(|i| {
                        ((u16::from(i) * u16::from(lane * 2 + 1) + u16::from(offset))
                            % u16::from(2 * n)) as u8
                    })
                    .collect();
                let list = indices
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vshuffle v3,v4,[{list}]\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                for (i, j) in indices.iter().enumerate() {
                    main += &format!(
                        "memcpy(e+{},{}+{}, {bytes});",
                        i as u32 * bytes,
                        if *j < n { "a" } else { "b" },
                        u32::from(*j % n) * bytes
                    );
                }
                main += &format!(
                    "{name}(a,b,r);if(memcmp(e,r,16)||memcmp(a,r+16,16)||memcmp(b,r+32,16))return 1;\n"
                );
            }
            let name = format!("lane_{shape}_{lane}");
            ir += &format!(
                "func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{} align 1 v1\n v5 = vinsert v3,v4,{lane}\n v6 = vextract v5,{lane}\n store align 1 v6,v2\n return\n}}\n",
                shape.lane_type()
            );
            c += &format!("extern void {name}(const void*,const void*,void*);\n");
            main += &format!(
                "memset(r,0xcd,48);{name}(a,b,r+1);if(memcmp(r+1,b,{bytes})||r[0]!=0xcd||r[{}]!=0xcd)return 2;\n",
                bytes + 1
            );
        }
    }
    main += "}return 0;}\n";
    c += &main;
    (ir, c)
}
#[test]
fn packed_lane_matrix_oracle_and_encoding_parity() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = common::temp("neon-lane-matrix");
    fs::write(dir.join("test.c"), c).unwrap();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    common::command(
        Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("test.c"))
            .arg("-o")
            .arg(dir.join("oracle.o")),
    );
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                assert_eq!(program.stats().text_bytes, 0);
                assert!(
                    !text.contains(" = stack_addr"),
                    "lane legalization allocated buffers"
                );
                let object =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
                fs::write(dir.join("test.s"), &text).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-target", "arm64-apple-macos11", "-c"])
                        .arg(dir.join("test.s"))
                        .arg("-o")
                        .arg(dir.join("clang.o")),
                );
                let reference = fs::read(dir.join("clang.o")).unwrap();
                let file = object::File::parse(&*reference).unwrap();
                assert_eq!(
                    keel_asm::assemble_program(&program, Default::default())
                        .unwrap()
                        .text,
                    file.section_by_name("__text").unwrap().data().unwrap()
                );
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    for (route, bytes) in [
                        ("direct", object),
                        ("text", keel_asm::assemble_object(&text).unwrap()),
                    ] {
                        fs::write(dir.join("test.o"), bytes).unwrap();
                        common::command(
                            Command::new("clang")
                                .arg("-O2")
                                .arg(dir.join("oracle.o"))
                                .arg(dir.join("test.o"))
                                .arg("-o")
                                .arg(dir.join(route)),
                        );
                        common::command(&mut Command::new(dir.join(route)));
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
