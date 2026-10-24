//! The same C kernels compiled by Keel and Clang, linked to a Clang ABI oracle.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::Target;
use std::{fs, path::Path, process::Command};

fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(
        output.status.success(),
        "{c:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn sources() -> (String, String) {
    let shapes = [
        ("i8", "signed char", "signed char", 16),
        ("u8", "unsigned char", "signed char", 16),
        ("i16", "short", "short", 8),
        ("u16", "unsigned short", "short", 8),
        ("i32", "int", "int", 4),
        ("u32", "unsigned", "int", 4),
        ("i64", "long", "long", 2),
        ("u64", "unsigned long", "long", 2),
        ("f32", "float", "int", 4),
        ("f64", "double", "long", 2),
    ];
    let mut source = String::new();
    let mut harness = String::from(
        "extern int memcmp(const void*,const void*,unsigned long);\n#define INFINITY __builtin_inff()\n#define NAN __builtin_nanf(\"\")\n",
    );
    let mut main = String::from("int main(void) {\n");
    for (id, base, mask, lanes) in shapes {
        let typedef = format!(
            "typedef {base} V_{id} __attribute__((vector_size(16)));\ntypedef {mask} M_{id} __attribute__((vector_size(16)));\n"
        );
        source.push_str(&typedef);
        harness.push_str(&typedef);
        let mut ops = vec![
            ("add", "a+b", false),
            ("sub", "a-b", false),
            ("mul", "a*b", false),
            ("neg", "-a", false),
            ("eq", "a==b", true),
            ("ne", "a!=b", true),
            ("lt", "a<b", true),
            ("ge", "a>=b", true),
            ("broadcast_add", "a+b[0]", false),
            ("broadcast_sub", "b[0]-a", false),
            ("broadcast_mul", "a*b[0]", false),
            ("broadcast_eq", "a==b[0]", true),
            ("broadcast_lt", "b[0]<a", true),
            ("broadcast_ne", "a!=b[0]", true),
            ("broadcast_le", "a<=b[0]", true),
            ("broadcast_gt", "b[0]>a", true),
            ("broadcast_ge", "a>=b[0]", true),
        ];
        if id.starts_with('f') {
            ops.push(("div", "a/b", false));
            ops.push(("broadcast_div", "a/b[0]", false));
        } else {
            ops.extend([
                ("and", "a&b", false),
                ("broadcast_and", "a&b[0]", false),
                ("broadcast_or", "b[0]|a", false),
                ("broadcast_xor", "a^b[0]", false),
                ("or", "a|b", false),
                ("xor", "a^b", false),
                ("not", "~a", false),
                ("shl", "a<<b", false),
                ("shr", "a>>b", false),
            ]);
        }
        for (op, expr, comparison) in ops {
            let ret = if comparison { "M" } else { "V" };
            source.push_str(&format!(
                "{ret}_{id} k_{id}_{op}(V_{id} a,V_{id} b) {{ return {expr}; }}\n"
            ));
            harness.push_str(&format!("extern {ret}_{id} k_{id}_{op}(V_{id},V_{id});\nextern {ret}_{id} c_{id}_{op}(V_{id},V_{id});\n"));
            main.push_str(&format!(r#"{{ V_{id} a={{0}},b={{0}}; for(int trial=0;trial<19;trial++) {{
                for(int i=0;i<{lanes};i++) {{ a[i]=({base})(trial*7919+i*13-70000); b[i]=({base})(i%3+1); }}
                {ret}_{id} k=k_{id}_{op}(a,b), c=c_{id}_{op}(a,b); if(memcmp(&k,&c,16)) return __LINE__;
            }} }}
"#));
        }
        if id.starts_with('f') {
            for op in [
                "broadcast_add",
                "broadcast_sub",
                "broadcast_mul",
                "broadcast_div",
                "broadcast_eq",
                "broadcast_ne",
                "broadcast_lt",
                "broadcast_le",
                "broadcast_gt",
                "broadcast_ge",
            ] {
                let ret = if matches!(
                    op,
                    "broadcast_add" | "broadcast_sub" | "broadcast_mul" | "broadcast_div"
                ) {
                    "V"
                } else {
                    "M"
                };
                main.push_str(&format!(r#"{{ {base} values[]={{0.0,-0.0,INFINITY,-INFINITY,NAN,1.5,-2.5}};
                    for(int x=0;x<7;x++) for(int y=0;y<7;y++) {{
                        V_{id} a={{0}},b={{0}}; for(int i=0;i<{lanes};i++)a[i]=values[(x+i)%7]; b[0]=values[y];
                        {ret}_{id} k=k_{id}_{op}(a,b),c=c_{id}_{op}(a,b); if(memcmp(&k,&c,16))return __LINE__;
                    }} }}
"#));
            }
        }
        source.push_str(&format!(
            "V_{id} k_{id}_compound(V_{id} a,{base} s) {{ a+=s; a-=s; a*=s; return a; }}\n"
        ));
        harness.push_str(&format!(
            "extern V_{id} k_{id}_compound(V_{id},{base}), c_{id}_compound(V_{id},{base});\n"
        ));
        main.push_str(&format!("{{ V_{id} a={{1,2}}; V_{id} k=k_{id}_compound(a,3), c=c_{id}_compound(a,3); if(memcmp(&k,&c,16))return __LINE__; }}\n"));
        let constant = match id {
            "i8" => "127",
            "u8" => "255",
            "i16" => "32767",
            "u16" => "65535",
            "i32" => "2147483647L",
            "u32" => "4294967295UL",
            "i64" => "9223372036854775807L",
            "u64" => "18446744073709551615UL",
            _ => "1.5",
        };
        source.push_str(&format!(
            "V_{id} k_{id}_constant(V_{id} a) {{ return a+{constant}; }}\n"
        ));
        harness.push_str(&format!(
            "extern V_{id} k_{id}_constant(V_{id}),c_{id}_constant(V_{id});\n"
        ));
        main.push_str(&format!("{{ V_{id} a={{1,2}}; V_{id} k=k_{id}_constant(a),c=c_{id}_constant(a); if(memcmp(&k,&c,16))return __LINE__; }}\n"));
        // Keel -> Clang calls and Clang -> Keel indirect calls, with overflowing FP argument banks.
        let params = (0..10)
            .map(|i| format!("V_{id} a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let values = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sum = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("+");
        source.push_str(&format!("extern V_{id} callback_{id}({params});\nV_{id} k_stack_{id}({params}) {{ return {sum}; }}\nV_{id} k_call_{id}({params}) {{ return callback_{id}({values}); }}\n{base} k_lane_{id}(V_{id} a, int i) {{ V_{id} b={{1}}; b[i]=a[i]; b[(i+1)%{lanes}]+=2; return b[i]; }}\n"));
        harness.push_str(&format!("V_{id} callback_{id}({params}) {{ return {sum}; }}\nextern V_{id} k_stack_{id}({params}),k_call_{id}({params});\nextern {base} k_lane_{id}(V_{id},int);\n"));
        main.push_str(&format!(r#"{{ V_{id} a={{0}}; for(int i=0;i<{lanes};i++)a[i]=i+1;
            V_{id} (*volatile indirect)({params})=k_stack_{id};
            V_{id} expected=callback_{id}(a,a,a,a,a,a,a,a,a,a), actual=indirect(a,a,a,a,a,a,a,a,a,a);
            if(memcmp(&expected,&actual,16)) return __LINE__;
            actual=k_call_{id}(a,a,a,a,a,a,a,a,a,a); if(memcmp(&expected,&actual,16))return __LINE__;
            for(int i=0;i<{lanes};i++)if(k_lane_{id}(a,i)!=a[i])return __LINE__;
        }}
"#));
    }
    source.push_str(
        r#"
typedef int DI __attribute__((mode(DI)));
typedef unsigned int UDI __attribute__((mode(DI)));
DI k_mode(DI x) { return x/3; } UDI k_umode(UDI x) { return x/3; }
typedef int V __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
V k_global={2,4};
V k_init(void) { V v={1,2}; v[3]=9; return v; }
V k_cast(F v) { return (V)v; }
V k_temporary(V a,V b,int i) { V v=a+b; return (V){(a+b)[i],v[1],v[2],v[3]}; }
V k_choose(int x,V a,V b) { return x?a:b; }
V k_broadcast_operands(V *p,int *i,int *s) { return p[(*i)++] + (*s)++; }
int k_broadcast_once(V *p,int *i,int *s) { p[(*i)++] += (*s)++; return p[0][3]; }
int k_broadcast_proofs(int *side) {
    typedef signed char B __attribute__((vector_size(16)));
    typedef unsigned char U __attribute__((vector_size(16)));
    B b={0}; U u={0}; F f={0}; V v={0};
    b+=((1<<6)+63); u=((~0UL)>>56)+u; f+=(double)0.1f;
    v+=sizeof((*side)++);
    for(int i=0;i<16;i++)if(b[i]!=127||u[i]!=255)return 1;
    b=(B){0}; b+=(signed char)257;
    for(int i=0;i<16;i++)if(b[i]!=1)return 2;
    for(int i=0;i<4;i++)if(v[i]!=sizeof(int)||f[i]!=0.1f)return 3;
    return 0;
}
int k_once(V *p,int *i) { (*p)[(*i)++] += 3; return (*p)[0]; }
"#,
    );
    harness.push_str(
        r#"
extern long k_mode(long); extern unsigned long k_umode(unsigned long);
typedef int V __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
extern V k_global;
extern V k_init(void), k_cast(F), k_temporary(V,V,int), k_choose(int,V,V);
extern V k_broadcast_operands(V*,int*,int*);
extern int k_once(V*,int*), k_broadcast_once(V*,int*,int*);
extern int k_broadcast_proofs(int*),c_broadcast_proofs(int*);
"#,
    );
    main.push_str(r#"
{ int side=17; if(k_broadcast_proofs(&side)||side!=17)return __LINE__;
if(c_broadcast_proofs(&side)||side!=17)return __LINE__; }
if(k_mode(-0x123456789L)!=-0x123456789L/3 || k_umode(~0UL)!=~0UL/3)return __LINE__;
{ if(k_global[0]!=2||k_global[1]!=4||k_global[2]!=0||k_global[3]!=0)return __LINE__; V v=k_init(); if(v[0]!=1||v[1]!=2||v[2]!=0||v[3]!=9)return __LINE__; }
{ F f={0.0f,-0.0f,INFINITY,NAN}; V v=k_cast(f); if(memcmp(&v,&f,16))return __LINE__; }
{ V a={1,2,3,4},b={5,6,7,8}; V v=k_temporary(a,b,2); if(v[0]!=10||v[1]!=8||v[2]!=10||v[3]!=12)return __LINE__;
v=k_choose(0,a,b);if(memcmp(&v,&b,16))return __LINE__;v=k_choose(1,a,b);if(memcmp(&v,&a,16))return __LINE__;
int i=0;if(k_once(&a,&i)!=4||i!=1||a[0]!=4)return __LINE__;
struct { unsigned long before; V a; unsigned long after; } box={0x12345678,{1,2,3,4},0x87654321};
i=0; int scalar=7; V broadcast=k_broadcast_operands(&box.a,&i,&scalar);
if(i!=1||scalar!=8||broadcast[0]!=8||broadcast[3]!=11||box.a[0]!=1)return __LINE__;
i=0; scalar=7; if(k_broadcast_once(&box.a,&i,&scalar)!=11||i!=1||scalar!=8)return __LINE__;
if(box.before!=0x12345678||box.after!=0x87654321||box.a[0]!=8||box.a[1]!=9||box.a[2]!=10)return __LINE__; }
{ F a={0.0f,-0.0f,INFINITY,NAN}, b={-0.0f,0.0f,INFINITY,1.0f};
V k=k_f32_eq(a,b),c=c_f32_eq(a,b);if(memcmp(&k,&c,16))return __LINE__;
k=k_f32_ne(a,b);c=c_f32_ne(a,b);if(memcmp(&k,&c,16))return __LINE__;
k=k_f32_lt(a,b);c=c_f32_lt(a,b);if(memcmp(&k,&c,16))return __LINE__; }
{ V_f64 a={NAN,-0.0},b={1.0,0.0};
M_f64 k=k_f64_ge(a,b),c=c_f64_ge(a,b);if(memcmp(&k,&c,16))return __LINE__;
k=k_f64_ne(a,b);c=c_f64_ne(a,b);if(memcmp(&k,&c,16))return __LINE__; }
return 0; }
"#);
    harness.push_str(&main);
    (source, harness)
}

fn native(target: Target) -> bool {
    match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin => cfg!(all(target_arch = "aarch64", target_os = "macos")),
        Target::Aarch64LinuxGnu => cfg!(all(target_arch = "aarch64", target_os = "linux")),
        Target::X86_64LinuxGnu => cfg!(all(target_arch = "x86_64", target_os = "linux")),
    }
}
fn clang_object(dir: &Path, source: &str, object: &str, triple: &str) {
    command(Command::new("clang").current_dir(dir).args([
        "-target",
        triple,
        "-std=gnu11",
        "-O1",
        "-c",
        source,
        "-o",
        object,
    ]));
}
#[test]
fn clang_behavior_and_bidirectional_vector_abi() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gnu-vector-abi");
    fs::create_dir_all(&dir).unwrap();
    let (source, harness) = sources();
    fs::write(dir.join("oracle.c"), source.replace(" k_", " c_")).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for target in Target::NATIVE {
        let triple = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin => "arm64-apple-macos11",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
        };
        clang_object(&dir, "oracle.c", "oracle.o", triple);
        clang_object(&dir, "harness.c", "harness.o", triple);
        for (selector, allocator, optimized) in [
            (SelectionTier::Baseline, AllocationTier::Full, false),
            (SelectionTier::Trees, AllocationTier::Fast, false),
            (SelectionTier::Full, AllocationTier::Full, true),
        ] {
            let mut module = keel_c::compile(
                &source,
                "kernels.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let program =
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
            assert_eq!(program.stats().text_bytes, 0);
            let object = match target {
                keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                Target::Aarch64AppleDarwin => {
                    keel_asm::assemble_program_object(&program, Default::default())
                }
                Target::Aarch64LinuxGnu => {
                    keel_asm::assemble_elf_program_object(&program, Default::default())
                }
                Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object_with_features(
                    &program,
                    Default::default(),
                ),
            }
            .unwrap();
            fs::write(dir.join("keel.o"), object).unwrap();
            if native(target) {
                command(Command::new("clang").current_dir(&dir).args([
                    "keel.o",
                    "oracle.o",
                    "harness.o",
                    "-o",
                    "run",
                ]));
                command(&mut Command::new(dir.join("run")));
            } else {
                // Cross-objects are real ELF/Mach-O objects; all three units must agree on the format.
                let keel = fs::read(dir.join("keel.o")).unwrap();
                for name in ["oracle.o", "harness.o"] {
                    assert_eq!(keel[..4], fs::read(dir.join(name)).unwrap()[..4]);
                }
            }
        }
    }
}

#[test]
fn darwin_homogeneous_vector_aggregates_match_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gnu-vector-hva");
    fs::create_dir_all(&dir).unwrap();
    let types = r#"
        typedef int V __attribute__((vector_size(16)));
        typedef float F __attribute__((vector_size(16)));
        struct One { V a; };
        struct Two { V a; F b; };
        struct Four { V a[4]; };
        struct Five { V a[5]; };
    "#;
    let mut source = types.to_owned();
    let mut harness = types.to_owned();
    harness.push_str("extern int memcmp(const void*,const void*,unsigned long);\n");
    let mut main = String::from("int main(void) {\n");
    for name in ["One", "Two", "Four", "Five"] {
        source.push_str(&format!("extern struct {name} cb_{name}(struct {name});\nstruct {name} k_{name}(struct {name} a){{return cb_{name}(a);}}\nstruct {name} k_overflow_{name}(V a,V b,V c,V d,V e,V f,V g,struct {name} s){{return cb_{name}(s);}}\n"));
        harness.push_str(&format!("struct {name} cb_{name}(struct {name} s){{return s;}}\nextern struct {name} k_{name}(struct {name});\nextern struct {name} k_overflow_{name}(V,V,V,V,V,V,V,struct {name});\n"));
        main.push_str(&format!("{{struct {name} s={{0}}; unsigned char *p=(unsigned char*)&s; for(unsigned i=0;i<sizeof s;i++)p[i]=(unsigned char)(i*13+7); struct {name} r=k_{name}(s);if(memcmp(&r,&s,sizeof s))return __LINE__;V v={{1,2,3,4}};r=k_overflow_{name}(v,v,v,v,v,v,v,s);if(memcmp(&r,&s,sizeof s))return __LINE__;}}\n"));
    }
    main.push_str("return 0;}\n");
    harness.push_str(&main);
    fs::write(dir.join("harness.c"), harness).unwrap();
    clang_object(&dir, "harness.c", "harness.o", "arm64-apple-macos11");
    for optimized in [false, true] {
        let mut module = keel_c::compile(
            &source,
            "hva.c",
            keel_c::Options {
                target: Target::Aarch64AppleDarwin,
                debug_info: true,
            },
        )
        .unwrap();
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let program = keel_codegen::compile_program(&module, Target::Aarch64AppleDarwin).unwrap();
        fs::write(
            dir.join("keel.o"),
            keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
        )
        .unwrap();
        if native(Target::Aarch64AppleDarwin) {
            command(Command::new("clang").current_dir(&dir).args([
                "keel.o",
                "harness.o",
                "-o",
                "run",
            ]));
            command(&mut Command::new(dir.join("run")));
        }
    }
}
