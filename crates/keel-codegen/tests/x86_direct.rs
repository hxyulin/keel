//! Direct x86 leaves preserve scalar semantics and all output transports.
#[path = "x86_direct/integer.rs"]
mod integer;
#[path = "x86_direct/memory.rs"]
mod memory;
#[path = "x86_direct/stack.rs"]
mod stack;
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, UnwindTables,
    backend_profile, regalloc::AllocationTier,
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::fmt::Write;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::{fs, process::Command};

fn request() -> Request<'static> {
    let mut options = EmissionOptions::default();
    options.backend_mode = BackendMode::CompileSpeed;
    Request::new(
        Target::X86_64LinuxGnu,
        CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .emission(options)
}
fn parse(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn corpus() -> (String, String) {
    let mut source = include_str!("x86_direct/leaves.keel").to_owned();
    let mut oracle = String::from("#include <stdint.h>\n");
    let mut widths = String::from(
        "int check_widths(unsigned char *bytes,unsigned char *out){\nconst uint64_t values[]={0,1,127,128,255,256,32767,32768,65535,UINT64_C(0x80000000),UINT64_MAX};\nconst uint64_t counts[]={0,1,7,8,15,16,31,32,63,64,127,255,UINT64_MAX};\n",
    );
    let mut comparisons = String::from(
        "int check_comparisons(void){\nconst uint64_t values[]={0,1,127,128,255,32767,32768,65535,UINT64_C(0x80000000),UINT64_C(0x8000000000000000),UINT64_MAX};\n",
    );
    for bits in [8, 16, 32, 64] {
        for op in ["shl", "ushr", "sshr"] {
            writeln!(source,"func @{op}{bits}(i{bits},i64)->i64 {{\nblock0(v0:i{bits},v1:i64):\n v2={op} v0,v1\n {extend} return {result}\n}}",extend=if bits<64 {"v3=uext.i64 v2\n".to_owned()}else{String::new()},result=if bits<64 {"v3"}else{"v2"}).unwrap();
            writeln!(oracle, "extern uint64_t {op}{bits}(uint{bits}_t,uint64_t);").unwrap();
            let mask = if bits == 64 {
                "UINT64_MAX".to_owned()
            } else {
                format!("((UINT64_C(1)<<{bits})-1)")
            };
            let expression = match op {
                "shl" => format!("(x<<shift)&{mask}"),
                "ushr" => "x>>shift".to_owned(),
                _ => format!("((uint64_t)((int64_t)(int{bits}_t)x>>shift))&{mask}"),
            };
            writeln!(widths,"for(unsigned i=0;i<11;i++)for(unsigned j=0;j<13;j++){{uint64_t x=(uint{bits}_t)values[i],shift=counts[j]&{};if({op}{bits}((uint{bits}_t)x,counts[j])!=({expression}))return 1;}}",bits-1).unwrap();
        }
        // Arithmetic/bitwise normalization and sign extension of a negative result.
        writeln!(source,"func @math{bits}(i{bits},i{bits})->i64 {{\nblock0(v0:i{bits},v1:i{bits}):\n v2=iadd v0,v1\n v3=imul v2,v0\n v4=isub v3,v1\n v5=bnot v4\n v6=ineg v5\n v7=and v6,v1\n v8=or v7,v0\n v9=xor v8,v1\n {extend} return {result}\n}}",extend=if bits<64 {"v10=sext.i64 v9\n".to_owned()}else{String::new()},result=if bits<64 {"v10"}else{"v9"}).unwrap();
        writeln!(
            oracle,
            "extern int64_t math{bits}(uint{bits}_t,uint{bits}_t);"
        )
        .unwrap();
        writeln!(widths,"for(unsigned i=0;i<11;i++){{uint64_t a=(uint{bits}_t)values[i],b=(uint{bits}_t)~values[i];uint64_t x=-~((a+b)*a-b);x=((x&b)|a)^b;if(math{bits}(a,b)!=(int64_t)(int{bits}_t)x)return 2;}}").unwrap();
        writeln!(source,"func @copy{bits}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2=load.i{bits} v0\n store v2,v1\n return\n}}").unwrap();
        writeln!(oracle, "extern void copy{bits}(void *,void *);").unwrap();
        writeln!(widths,"for(unsigned i=0;i<24;i++)out[i]=77;copy{bits}(bytes+1,out+1);for(unsigned i=0;i<24;i++)if(out[i]!=(i>=1&&i<{}?bytes[i]:77))return 3;",bits/8+1).unwrap();
        for (condition, operator, signed) in [
            ("eq", "==", false),
            ("ne", "!=", false),
            ("ult", "<", false),
            ("ule", "<=", false),
            ("ugt", ">", false),
            ("uge", ">=", false),
            ("slt", "<", true),
            ("sle", "<=", true),
            ("sgt", ">", true),
            ("sge", ">=", true),
        ] {
            writeln!(source,"func @cmp_{condition}{bits}(i{bits},i{bits})->i64 {{\nblock0(v0:i{bits},v1:i{bits}):\n v2=icmp.{condition} v0,v1\n v3=uext.i64 v2\n return v3\n}}").unwrap();
            writeln!(
                oracle,
                "extern uint64_t cmp_{condition}{bits}(uint{bits}_t,uint{bits}_t);"
            )
            .unwrap();
            let kind = if signed { "int" } else { "uint" };
            writeln!(comparisons,"for(unsigned i=0;i<11;i++)for(unsigned j=0;j<11;j++)if(cmp_{condition}{bits}(values[i],values[j])!=(({kind}{bits}_t)values[i]{operator}({kind}{bits}_t)values[j]))return 1;").unwrap();
        }
    }
    // Pointer arithmetic and truncation are checked independently of native pointer ABI.
    source.push_str("func @truncate(i64)->i64 {\nblock0(v0:i64):\n v1=trunc.i8 v0\n v2=uext.i64 v1\n return v2\n}\n");
    oracle.push_str("extern uint64_t truncate(uint64_t);\n");
    widths.push_str("for(unsigned i=0;i<11;i++)if(truncate(values[i])!=(uint8_t)values[i])return 4;return 0;}\n");
    comparisons.push_str("return 0;}\n");
    oracle.push_str(&widths);
    oracle.push_str(&comparisons);
    oracle.push_str(include_str!("x86_direct/oracle.c"));
    (source, oracle)
}
#[test]
fn scalar_leaves_share_stream_text_chunks_and_unwind_bytes() {
    let (source, _) = corpus();
    let module = parse(&source);
    for unwind in [UnwindTables::All, UnwindTables::RequiredOnly] {
        let mut req = request();
        req.emission.unwind_tables = unwind;
        let (program, report) = backend_profile::capture(|| req.program(&module));
        let program = program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            module.functions.len() as u64
        );
        let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_source_for(req.target, &req.text(&module).unwrap()).unwrap()
        );
        let mut chunks = keel_asm_ir::Program::new();
        req.program_chunks(&module, &mut |p| chunks.append(p))
            .unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_program_for(req.target, &chunks).unwrap()
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
        assert_eq!(
            file.section_by_name(".eh_frame").is_some(),
            unwind == UnwindTables::All
        );
    }
}
#[test]
fn unsupported_debug_and_abi_shapes_fall_back_transactionally() {
    let cases = [
        "func @f(f64)->f64 {\nblock0(v0:f64):\n return v0\n}\n",
        "func @f(i64,i64)->i64 {\nblock0(v0:i64,v1:i64):\n v2=rotl v0,v1\n return v2\n}\n",
        "func @f(ptr byval {i64})->i64 {\nblock0(v0:ptr):\n v1=iconst.i64 1\n return v1\n}\n",
        "func @f()->i64 {\n ss0=stack_slot 4096,align 32\nblock0:\n v0=iconst.i64 1\n return v0\n}\n",
        "file f0 = \"source.c\"\nfunc @f(i32)->i32 {\nblock0(v0:i32):\n return v0\n}\n",
    ];
    for source in cases {
        let module = parse(source);
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
}
#[test]
fn native_or_exported_linux_oracle_and_rosetta_byte_oracle() {
    let (source, oracle) = corpus();
    let module = parse(&source);
    let req = request();
    let program = req.program(&module).unwrap();
    let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
    // Keep the actual Linux ABI image and oracle available to the export runner.
    keel_test_support::native_program!("direct-scalars")
        .file("leaves.keel", source)
        .file("leaves.o", &bytes)
        .file("oracle.c", &oracle)
        .link(["-O2", "oracle.c", "leaves.o"])
        .check();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        // SysV register arguments match Darwin x86-64 for this scalar leaf subset.
        // Relocation-free symbol ranges preserve every emitted instruction byte.
        let rosetta = Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .unwrap();
        if !rosetta.success() {
            keel_test_support::skip!("Rosetta unavailable for x86 direct byte oracle");
        }
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let mut assembly = String::from(".text\n");
        for symbol in file
            .symbols()
            .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
        {
            let section = file
                .section_by_index(symbol.section_index().unwrap())
                .unwrap();
            assert!(section.relocations().next().is_none());
            let data = section.data().unwrap();
            let start = symbol.address() as usize;
            let body = &data[start..start + symbol.size() as usize];
            writeln!(
                assembly,
                ".p2align 4\n.globl _{}\n_{}:",
                symbol.name().unwrap(),
                symbol.name().unwrap()
            )
            .unwrap();
            for chunk in body.chunks(32) {
                assembly.push_str(".byte ");
                for (i, b) in chunk.iter().enumerate() {
                    if i > 0 {
                        assembly.push(',');
                    }
                    write!(assembly, "{b}").unwrap();
                }
                assembly.push('\n');
            }
        }
        let dir = keel_test_support::TempDir::new("x86-direct-rosetta");
        fs::write(dir.join("leaves.s"), assembly).unwrap();
        fs::write(dir.join("oracle.c"), oracle).unwrap();
        let Some(clang) = keel_test_support::clang() else {
            keel_test_support::skip!(keel_test_support::NO_CLANG);
        };
        let result = clang
            .command()
            .current_dir(dir.path())
            .args([
                "-arch", "x86_64", "-O2", "oracle.c", "leaves.s", "-o", "oracle",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            Command::new("arch")
                .arg("-x86_64")
                .arg(dir.join("oracle"))
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn direct_symbols_keep_visibility_comdat_sections_and_alignment() {
    let source = "func @visible()->i64 section \".direct\" align 64 weak hidden {\nblock0:\n v0=iconst.i64 7\n return v0\n}\nfunc @coalesced()->i64 weak_odr {\nblock0:\n v0=iconst.i64 9\n return v0\n}\nfunc @private()->i64 local {\nblock0:\n v0=iconst.i64 11\n return v0\n}\n";
    let mut module = parse(source);
    module.functions[0].attrs.retain = true;
    let mut req = request();
    req.emission.function_sections = true;
    let (program, report) = backend_profile::capture(|| req.program(&module));
    let program = program.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        3
    );
    let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let section = file.section_by_name(".direct").unwrap();
    assert_eq!(section.align(), 64);
    assert!(file.section_by_name(".text.coalesced").is_some());
    assert!(file.section_by_name(".text.private").is_some());
    assert!(
        file.symbols()
            .find(|s| s.name() == Ok("visible"))
            .unwrap()
            .is_weak()
    );
    assert!(
        file.symbols()
            .find(|s| s.name() == Ok("coalesced"))
            .unwrap()
            .is_weak()
    );
    assert!(
        file.symbols()
            .find(|s| s.name() == Ok("private"))
            .unwrap()
            .is_local()
    );
    assert!(program.items.iter().any(|item|matches!(item,keel_asm_ir::Item::Directive(keel_asm_ir::Directive::ElfHidden(name)) if name=="visible")));
    assert!(file.comdats().count() > 0);
    let stream = req
        .assemble_x86(
            &module,
            keel_asm::x86::ProgramAssembler::new(Default::default()),
            Box::new(|_| Err("no binary inputs".into())),
        )
        .unwrap();
    assert_eq!(bytes, stream.finish().unwrap());
}

#[path = "x86_direct/multi.rs"]
mod multi;
