//! Boundary oracles for AArch64 compile-speed scalar and stack lowering.
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, backend_profile,
    regalloc::AllocationTier,
};
use std::{fmt::Write, fs, process::Command};

fn request(target: Target) -> Request<'static> {
    let mut emission = EmissionOptions::default();
    emission.backend_mode = BackendMode::CompileSpeed;
    Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .emission(emission)
}
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut header = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut checks = String::from("int main(void){\n");
    for width in [8, 16, 32, 64] {
        let ext = if width < 32 { " uext" } else { "" };
        writeln!(ir,"func @calc{width}(i{width}{ext},i{width}{ext},i64)->i64 {{\nblock0(v0:i{width},v1:i{width},v2:i64):\n v3=iadd v0,v1\n v4=isub v3,v0\n v5=imul v4,v1\n v6=bnot v5\n v7=ineg v6\n v8=and v7,v0\n v9=or v8,v1\n v10=xor v9,v0\n v11=shl v10,v2\n v12=ushr v11,v2\n v13=sshr v10,v2\n v14=icmp.slt v10,v1\n v15=select v14,v12,v13").unwrap();
        if width < 64 {
            ir.push_str(" v16=uext.i64 v15\n return v16\n}\n");
        } else {
            ir.push_str(" return v15\n}\n");
        }
        writeln!(
            header,
            "extern uint64_t calc{width}(uint{width}_t,uint{width}_t,uint64_t);"
        )
        .unwrap();
        // Values above the narrow width and count-width boundaries exercise modulo
        // shifts and negative signed operands without undefined signed arithmetic.
        writeln!(checks,"{{const uint64_t values[]={{0,1,127,128,255,256,32767,32768,65535,UINT64_C(0x80000000),UINT64_C(0x8000000000000000),UINT64_MAX}};const uint64_t counts[]={{0,1,7,8,15,16,31,32,63,64,65,255,UINT64_MAX}};for(unsigned i=0;i<12;i++)for(unsigned j=0;j<12;j++)for(unsigned k=0;k<13;k++){{uint64_t mask=UINT{width}_MAX,a=values[i]&mask,b=values[j]&mask,c=counts[k]&{minus};uint64_t x=((((b*b)&mask)+1)&mask);x=((x&a)|b)^a;uint64_t left=((x<<c)&mask)>>c;uint64_t right=(uint64_t)((int{width}_t)x>>c)&mask;uint64_t expected=(int{width}_t)x<(int{width}_t)b?left:right;if(calc{width}(a,b,counts[k])!=expected)return {width};}}}}",minus=width-1).unwrap();
        writeln!(ir,"func @less{width}(i{width}{ext},i{width}{ext})->i64 {{\nblock0(v0:i{width},v1:i{width}):\n v2=icmp.slt v0,v1\n brif v2,block1,block2\nblock1:\n v3=iconst.i64 1\n return v3\nblock2:\n v4=iconst.i64 0\n return v4\n}}").unwrap();
        writeln!(
            header,
            "extern uint64_t less{width}(uint{width}_t,uint{width}_t);"
        )
        .unwrap();
        writeln!(checks,"{{const uint64_t values[]={{0,1,127,128,255,256,32767,32768,65535,UINT64_C(0x80000000),UINT64_C(0x8000000000000000),UINT64_MAX}};for(unsigned i=0;i<12;i++)for(unsigned j=0;j<12;j++)if(less{width}(values[i],values[j])!=((int{width}_t)values[i]<(int{width}_t)values[j]))return 69;}}").unwrap();
        writeln!(ir,"func @touch{width}(ptr,i{width}{ext})->i{width}{ext} {{\nblock0(v0:ptr,v1:i{width}):\n v2=load.i{width} v0\n v3=iadd v2,v1\n store v3,v0\n return v3\n}}").unwrap();
        writeln!(
            header,
            "extern uint{width}_t touch{width}(void*,uint{width}_t);"
        )
        .unwrap();
        writeln!(checks,"{{unsigned char bytes[24];memset(bytes,0xa5,sizeof bytes);uint{width}_t before;memcpy(&before,bytes+1,sizeof before);uint{width}_t expected=before+19;if(touch{width}(bytes+1,19)!=expected)return 71;uint{width}_t after;memcpy(&after,bytes+1,sizeof after);if(after!=expected)return 72;for(unsigned i=0;i<sizeof bytes;i++)if((i<1||i>=1+sizeof after)&&bytes[i]!=0xa5)return 73;}}").unwrap();
    }
    ir.push_str("func @stack(i64)->i64 {\n ss0=stack_slot 16, align 16\n ss1=stack_slot 8, align 8\nblock0(v0:i64):\n v1=stack_addr ss0\n v2=stack_addr ss1\n v3=iconst.i64 17\n store v0,v1\n store v3,v2\n v4=load.i64 v1\n v5=load.i64 v2\n v6=iadd v4,v5\n v7=ptrtoint.i64 v1\n v8=iconst.i64 15\n v9=and v7,v8\n v10=iadd v6,v9\n return v10\n}\nfunc @put(ptr,i32) {\nblock0(v0:ptr,v1:i32):\n store v1,v0\n return\n}\nfunc @offset(ptr,i64)->ptr {\nblock0(v0:ptr,v1:i64):\n v2=ptradd v0,v1\n v3=ptrtoint.i64 v2\n v4=inttoptr.ptr v3\n return v4\n}\nfunc @signed8(i8 sext)->i8 sext {\nblock0(v0:i8):\n v1=ineg v0\n return v1\n}\nfunc @signed16(i16 sext)->i16 sext {\nblock0(v0:i16):\n v1=ineg v0\n return v1\n}\nfunc @narrow(i64)->i64 {\nblock0(v0:i64):\n v1=trunc.i8 v0\n v2=sext.i32 v1\n v3=trunc.i16 v2\n v4=sext.i64 v3\n return v4\n}\nfunc @pointer_slot(ptr)->ptr {\n ss0=stack_slot 8, align 8\nblock0(v0:ptr):\n v1=stack_addr ss0\n store v0,v1\n v2=load.ptr v1\n return v2\n}\n");
    ir.push_str("func @private8(i8)->i8 cc(fast) local {\nblock0(v0:i8):\n v1=ineg v0\n return v1\n}\nfunc @call_private8(i8 uext)->i64 {\nblock0(v0:i8):\n v1=call @private8(v0)\n v2=sext.i64 v1\n return v2\n}\nfunc @truth8(i8 uext)->i64 {\nblock0(v0:i8):\n brif v0,block1,block2\nblock1:\n v1=iconst.i64 1\n return v1\nblock2:\n v2=iconst.i64 0\n return v2\n}\n");
    header.push_str("extern int64_t call_private8(uint8_t);extern uint64_t truth8(uint8_t);\n");
    checks.push_str("for(unsigned i=0;i<256;i++){if(call_private8(i)!=(int8_t)(-(int8_t)i))return 87;if(truth8(i)!=(i!=0))return 88;}\n");
    ir.push_str("func @swap8(i8 uext,i8 uext,i8 uext)->i8 uext {\nblock0(v0:i8,v1:i8,v2:i8):\n v3=iconst.i8 0\n v4=iconst.i8 1\n jump block1(v0,v1,v2)\nblock1(v5:i8,v6:i8,v7:i8):\n v8=icmp.ugt v5,v3\n brif v8,block2,block3(v6)\nblock2:\n v9=isub v5,v4\n jump block1(v9,v7,v6)\nblock3(v10:i8):\n return v10\n}\nfunc @signed32(i32)->i32 {\nblock0(v0:i32):\n v1=ineg v0\n return v1\n}\n");
    header.push_str(
        "extern uint8_t swap8(uint8_t,uint8_t,uint8_t);extern int32_t signed32(int32_t);\n",
    );
    checks.push_str("for(unsigned i=0;i<256;i++){for(unsigned n=0;n<16;n++)if(swap8(n,i,255-i)!=(n&1?255-i:i))return 89;uint32_t input=i*UINT32_C(0x9e3779b9);if(signed32(input)!=(int32_t)(0u-input))return 90;}\n");
    header.push_str("extern uint64_t stack(uint64_t);extern void put(void*,uint32_t);extern void *offset(void*,uint64_t);extern int8_t signed8(int8_t);extern int16_t signed16(int16_t);extern int64_t narrow(uint64_t);extern void *pointer_slot(void*);\n");
    checks.push_str("for(uint64_t i=0;i<256;i++){if(stack(i)!=i+17)return 80;if(signed8(i)!=(int8_t)(-(int8_t)i))return 81;if(signed16(i*257)!=(int16_t)(-(int16_t)(i*257)))return 82;if(narrow(i)!=(int8_t)i)return 83;}uint32_t value=0;put(&value,0xdeadbeef);if(value!=0xdeadbeef)return 84;unsigned char data[16];if(offset(data,7)!=data+7)return 85;if(pointer_slot(data)!=data)return 86;return 0;}\n");
    header.push_str(&checks);
    (ir, header)
}
#[test]
fn narrow_memory_conversions_stacks_and_unwind_keep_text_object_parity() {
    let (source, _) = fixtures();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let req = request(target);
        let (program, report) = backend_profile::capture(|| req.program(&module));
        let program = program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            module.functions.len() as u64
        );
        assert_eq!(
            keel_asm::assemble_program_for(target, &program).unwrap(),
            keel_asm::assemble_source_for(target, &req.text(&module).unwrap()).unwrap()
        );
    }
}
#[test]
fn native_narrow_scalar_and_memory_results_match_independent_c() {
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Target::Aarch64AppleDarwin
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Target::Aarch64LinuxGnu
    } else {
        keel_test_support::skip!("requires native AArch64 host");
    };
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    };
    let (source, oracle) = fixtures();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let req = request(target);
    let program = req.program(&module).unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct");
    fs::write(
        dir.join("program.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    let out = clang
        .command()
        .current_dir(dir.path())
        .args(["-O2", "oracle.c", "program.o", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(dir.join("oracle")).output().unwrap();
    assert!(out.status.success(), "exit {:?}", out.status.code());
}
#[test]
fn unsupported_abis_and_frames_still_fall_back() {
    for source in [
        "func @f(f64)->f64 {\nblock0(v0:f64):\n return v0\n}\n",
        "func @f(ptr)->ptr {\n ss0=stack_slot 16, align 32\nblock0(v0:ptr):\n return v0\n}\n",
        "func @f(i128)->i128 {\nblock0(v0:i128):\n return v0\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let (out, report) =
            backend_profile::capture(|| request(Target::Aarch64AppleDarwin).program(&module));
        out.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            0
        );
    }
}

fn control_fixtures() -> (keel_ir::Module, String) {
    let mut source = include_str!("arm_direct/control.keel").to_owned();
    let mut header = include_str!("arm_direct/control.c").to_owned();
    let mut checks = String::from(
        "int main(int argc,char**argv){if(argc>1){volatile uint64_t sink=0;switch(argv[1][0]){case 'z':sink=udiv64(17,0);break;case 'a':sink=sdiv8(128,255);break;case 'b':sink=sdiv16(32768,65535);break;case 'c':sink=sdiv32(UINT64_C(0x80000000),UINT32_MAX);break;case 'd':sink=sdiv64(UINT64_C(0x8000000000000000),UINT64_MAX);break;}return sink==17;}if(check_symbols_switches())return 9;\n",
    );
    for width in [8, 16, 32, 64] {
        let ext = if width < 32 { " uext" } else { "" };
        for op in ["udiv", "urem", "sdiv", "srem"] {
            writeln!(source,"func @{op}{width}(i{width}{ext},i{width}{ext})->i64 {{\nblock0(v0:i{width},v1:i{width}):\n v2={op} v0,v1").unwrap();
            if width < 64 {
                source.push_str(" v3=uext.i64 v2\n return v3\n}\n");
            } else {
                source.push_str(" return v2\n}\n");
            }
            writeln!(
                header,
                "extern uint64_t {op}{width}(uint{width}_t,uint{width}_t);"
            )
            .unwrap();
        }
        writeln!(checks,"{{const uint64_t values[]={{0,1,2,7,127,128,255,32767,32768,65535,UINT64_C(0x80000000),UINT64_C(0x8000000000000000),UINT64_MAX}};for(unsigned i=0;i<13;i++)for(unsigned j=0;j<13;j++){{uint{width}_t a=values[i],b=values[j];if(!b)continue;if(udiv{width}(a,b)!=(uint{width}_t)(a/b)||urem{width}(a,b)!=(uint{width}_t)(a%b))return 10;int{width}_t sa=a,sb=b;int overflow=sa==INT{width}_MIN&&sb==-1;if(!overflow&&sdiv{width}(a,b)!=(uint{width}_t)(sa/sb))return 11;if(srem{width}(a,b)!=(overflow?0:(uint{width}_t)(sa%sb)))return 12;}}}}").unwrap();
    }
    checks.push_str("return 0;}\n");
    header.push_str(&checks);
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    (module, header)
}
fn control_target(module: &mut keel_ir::Module, target: Target) {
    if target == Target::Aarch64LinuxGnu {
        // AArch64 ELF deliberately does not support weak_odr. The Mach-O arm
        // retains the ODR definitions and checks their GOT/coalescing policy.
        for data in &mut module.data {
            data.weak_odr = false;
        }
    }
}
#[test]
fn symbol_address_binding_matches_ordinary_policy_and_tls_falls_back() {
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let (mut module, _) = control_fixtures();
        control_target(&mut module, target);
        let req = request(target);
        let (result, report) = backend_profile::capture(|| req.program(&module));
        let program = result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            module.functions.len() as u64
        );
        let ordinary = Request::new(target, req.pipeline).program(&module).unwrap();
        let bindings = |program: &keel_asm_ir::Program| {
            program
                .items
                .iter()
                .filter_map(|item| match item {
                    keel_asm_ir::Item::Symbolic(
                        keel_asm_ir::SymbolicInstruction::PageAddress { kind, target, .. },
                    ) => Some((target.clone(), *kind)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(bindings(&program), bindings(&ordinary));
        assert_eq!(
            keel_asm::assemble_program_for(target, &program).unwrap(),
            keel_asm::assemble_source_for(target, &req.text(&module).unwrap()).unwrap()
        );
    }
    let module=keel_ir::parse("data @tls=thread_local align 8 {i64 1}\nfunc @f()->ptr {\nblock0:\n v0=symbol_addr @tls\n return v0\n}\n").unwrap();
    let (program, report) =
        backend_profile::capture(|| request(Target::Aarch64AppleDarwin).program(&module));
    program.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
}
#[test]
fn native_symbol_switch_and_division_boundaries_match_c_and_trap() {
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Target::Aarch64AppleDarwin
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Target::Aarch64LinuxGnu
    } else {
        keel_test_support::skip!("requires native AArch64 host");
    };
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    };
    let (mut module, oracle) = control_fixtures();
    control_target(&mut module, target);
    let program = request(target).program(&module).unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct-control");
    fs::write(
        dir.join("program.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    let out = clang
        .command()
        .current_dir(dir.path())
        .args(["-O2", "oracle.c", "program.o", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(dir.join("oracle")).output().unwrap();
    assert!(out.status.success(), "exit {:?}", out.status.code());
    use std::os::unix::process::ExitStatusExt;
    for trap in ["z", "a", "b", "c", "d"] {
        let out = Command::new(dir.join("oracle")).arg(trap).output().unwrap();
        assert_eq!(out.status.signal(), Some(5), "{trap}: {:?}", out.status);
    }
}
