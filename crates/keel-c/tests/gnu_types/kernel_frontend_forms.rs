//! Frontend forms used by the pinned Linux headers.
use keel_ir::Target;
fn compile(source: &str) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "kernel-forms.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
}
#[test]
fn fallthrough_is_independent_of_lexer_chunk_position() {
    for padding in 0..512 {
        let source = format!(
            "int f(int x){{switch(x){{case 0:x+=1;{} __attribute__((__fallthrough__));case 1:return x;default:return 0;}}}}",
            ";".repeat(padding)
        );
        compile(&source).unwrap_or_else(|e| panic!("padding {padding}: {e}"));
    }
}
#[test]
fn static_assert_messages_accept_adjacent_strings() {
    compile("_Static_assert(1,\"first\" \"second\" \"third\"); int f(void){_Static_assert(1,\"a\" \"b\");return 0;}").unwrap();
    assert!(
        compile("_Static_assert(0,\"first\" \"second\");")
            .unwrap_err()
            .message
            .contains("static assertion failed")
    );
    assert!(
        compile("_Static_assert(1,123);")
            .unwrap_err()
            .message
            .contains("string literal")
    );
}
#[test]
fn static_compound_copies_preserve_offsets_bits_strings_and_relocations() {
    let source = "struct Inner {unsigned a:5,b:3; int *p;}; struct Outer {int before; struct Inner in; char name[4]; int after;}; int target=37; struct Outer value=(struct Outer){.before=11,.in=(struct Inner){.a=17,.b=5,.p=&target},.name=\"abc\",.after=43}; int check(void){return value.before==11&&value.in.a==17&&value.in.b==5&&value.in.p==&target&&value.name[2]=='c'&&value.after==43;}";
    let module = compile(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.to_string();
    let roundtrip = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&roundtrip).unwrap();
    keel_codegen::compile(&roundtrip, Target::X86_64LinuxGnu).unwrap();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("static-compound-copy");
    std::fs::create_dir_all(&dir).unwrap();
    let body = dir.join("source.c");
    let runner = dir.join("runner.c");
    std::fs::write(&body, source).unwrap();
    std::fs::write(
        &runner,
        "extern int check(void);int main(void){return !check();}",
    )
    .unwrap();
    for generated in [false, true] {
        let input = if generated {
            let target = Target::host().unwrap();
            let module = keel_c::compile(
                source,
                "compound-copy.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            let path = dir.join("generated.s");
            std::fs::write(&path, keel_codegen::compile(&module, target).unwrap()).unwrap();
            path
        } else {
            body.clone()
        };
        let exe = dir.join(format!("run-{generated}"));
        let output = std::process::Command::new("clang")
            .args(["-w", "-O2"])
            .arg(&input)
            .arg(&runner)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(std::process::Command::new(&exe).status().unwrap().success());
    }
    for invalid in [
        "int f(void); struct S{int x;}; struct S s=(struct S){f()};",
        "int x; struct S{int x;}; struct S s=(struct S){++x};",
    ] {
        assert!(
            compile(invalid)
                .unwrap_err()
                .message
                .contains("not a constant")
        );
    }
}

fn native_check(source: &str, name: &str) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let body = dir.join("source.c");
    let runner = dir.join("runner.c");
    std::fs::write(&body, source).unwrap();
    std::fs::write(
        &runner,
        "extern int check(void);int main(void){return !check();}",
    )
    .unwrap();
    let target = Target::host().unwrap();
    let module = keel_c::compile(
        source,
        "native-forms.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    let generated = dir.join("generated.s");
    std::fs::write(&generated, keel_codegen::compile(&module, target).unwrap()).unwrap();
    for input in [&body, &generated] {
        let exe = input.with_extension("exe");
        let output = std::process::Command::new("clang")
            .args(["-O2", "-w"])
            .arg(input)
            .arg(&runner)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(std::process::Command::new(exe).status().unwrap().success());
    }
}
#[test]
fn local_labels_shadow_in_a_separate_namespace() {
    let source = r#"
 int labels(int x) {int out=0;
 {__label__ done;if(x)goto done;out+=1;done:out+=2;}
 {__label__ done;void *p=&&done;if(x>0)goto *p;out+=4;done:out+=8;}
 return out;}
 int check(void){return labels(0)==15&&labels(1)==10&&labels(-1)==14;}
 "#;
    compile(source).unwrap();
    native_check(source, "local-labels");
    compile(
        "int f(void){__label__ done;asm goto(\"jmp %l[done]\"::::done);return 0;done:return 1;}",
    )
    .unwrap();
    assert!(compile("int f(void){__label__ done,done;done:return 0;}").is_err());
}
#[test]
fn static_flexible_arrays_extend_storage_without_changing_sizeof() {
    let source = r#"
 struct Tail {unsigned long count;unsigned long values[];};
 struct Tail values={3,{11,22,33}};
 int target=77;struct Pointers {unsigned long count;int *values[];};
 struct Pointers pointers={1,{&target}};
 _Static_assert(sizeof(values)==8,"nominal size");
 int check(void){return values.count==3&&values.values[0]==11&&values.values[2]==33&&pointers.count==1&&*pointers.values[0]==77;}
 "#;
    compile(source).unwrap();
    native_check(source, "static-flexible-arrays");
}

#[test]
fn local_static_compound_copies_remain_constant_initialization() {
    let source = "struct S{int x;};int f(void){static struct S value=(struct S){37};return value.x;}int check(void){return f()==37;}";
    compile(source).unwrap();
    native_check(source, "local-static-compound");
    assert!(compile("struct S{int x;};int f(void);int g(void){static struct S s=(struct S){f()};return s.x;}").is_err());
    assert!(
        compile("struct S{int x;};int *f(void){static int *p=&((struct S){37}).x;return p;}")
            .is_err()
    );
}

#[test]
fn non_fieldless_zero_size_aggregate_abi_stays_an_explicit_boundary() {
    for source in [
        "struct Empty{int a[0];}; int f(struct Empty value,int x){return x;}",
        "struct Empty{}; struct Nested{struct Empty value;}; struct Nested f(void){return (struct Nested){};}",
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.message.contains("zero-size aggregate ABI"),
            "{source}: {error}"
        );
    }
    compile("struct Empty{}; int external(struct Empty); int f(void){struct Empty local;return sizeof(local)==0;}").unwrap();
}
