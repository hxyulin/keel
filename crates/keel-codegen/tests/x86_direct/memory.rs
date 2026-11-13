//! Runtime memory helpers retain values and the ordinary PLT policy.
use super::{parse, request};
use keel_codegen::{Request, backend_profile};
use object::{Object, ObjectSection, ObjectSymbol};
const SOURCE: &str = include_str!("memory.keel");

#[test]
fn memory_helpers_match_libc_and_preserve_all_homes_when_registers_clobber() {
    super::integer::execute("memory-libc", SOURCE, include_str!("memory_oracle.c"));
    let oracle = include_str!("memory_clobber.c").to_owned() + include_str!("memory_oracle.c");
    super::integer::execute("memory-clobber", SOURCE, &oracle);
}
#[test]
fn memory_helper_sites_keep_stream_parity_and_always_use_plt() {
    use keel_codegen::x86_64::CodeModel;
    let source = "func @memcpy(ptr,ptr,i64)->ptr {\nblock0(v0:ptr,v1:ptr,v2:i64):\n return v0\n}\nfunc @f(ptr,ptr,i64,i8) {\nblock0(v0:ptr,v1:ptr,v2:i64,v3:i8):\n intrinsic memcpy(v0,v1,v2)\n intrinsic memmove(v0,v1,v2)\n intrinsic memset(v0,v3,v2)\n return\n}\n";
    let module = parse(source);
    for model in [
        CodeModel::PositionIndependent,
        CodeModel::Small,
        CodeModel::Kernel,
    ] {
        let mut req = request();
        req.emission.x86_code_model = Some(model);
        let (result, report) = backend_profile::capture(|| req.program(&module));
        let program = result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            2
        );
        let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_source_for(req.target, &req.text(&module).unwrap()).unwrap()
        );
        let stream = req
            .assemble_x86(
                &module,
                keel_asm::x86::ProgramAssembler::new(Default::default()),
                Box::new(|_| Err("no binary inputs".into())),
            )
            .unwrap();
        assert_eq!(bytes, stream.finish().unwrap());
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let text = file.section_by_name(".text").unwrap();
        let mut names = Vec::new();
        for (_, relocation) in text.relocations() {
            assert_eq!(
                relocation.flags(),
                object::RelocationFlags::Elf {
                    r_type: object::elf::R_X86_64_PLT32
                }
            );
            let object::RelocationTarget::Symbol(index) = relocation.target() else {
                panic!("non-symbol runtime call");
            };
            names.push(file.symbol_by_index(index).unwrap().name().unwrap());
        }
        names.sort();
        assert_eq!(names, ["memcpy", "memmove", "memset"]);
    }
    // memcmp has a different result contract and remains outside this batch.
    let module = parse(
        "func @f(ptr,ptr,i64)->i32 {\nblock0(v0:ptr,v1:ptr,v2:i64):\n v3=intrinsic memcmp(v0,v1,v2)\n return v3\n}\n",
    );
    let req = request();
    let (program, report) = backend_profile::capture(|| req.program(&module));
    let program = program.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
    assert_eq!(
        program,
        Request::new(req.target, req.pipeline)
            .program(&module)
            .unwrap()
    );
}
