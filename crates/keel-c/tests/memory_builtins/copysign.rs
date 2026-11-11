use super::*;

#[test]
fn copysign_recognition_respects_types_bindings_and_policy() {
    for target in Target::NATIVE {
        for (name, ty) in [("copysign", "double"), ("copysignf", "float")] {
            let source =
                format!("{ty} {name}({ty},{ty}); {ty} f({ty} x,{ty} y) {{return {name}(x,y);}}");
            let operation = keel_c::LibraryBuiltin::ALL
                .into_iter()
                .find(|op| op.name() == name)
                .unwrap();
            let m = compile(&source, target);
            assert!(!calls(&m, name), "{m}");
            assert!(m.to_string().contains("fcopysign"), "{m}");
            keel_codegen::compile(&m, target).unwrap();
            for policy in [
                keel_c::LibraryBuiltins::disabled(),
                keel_c::LibraryBuiltins::hosted().disable(operation),
            ] {
                for explicit in [false, true] {
                    let source = if explicit {
                        source.replace(
                            &format!("return {name}("),
                            &format!("return __builtin_{name}("),
                        )
                    } else {
                        source.clone()
                    };
                    let m = keel_c::compile_with_policies(
                        &source,
                        "copysign.c",
                        keel_c::Options {
                            target,
                            ..Default::default()
                        },
                        keel_c::FunctionBinding::default(),
                        policy,
                    )
                    .unwrap();
                    assert_eq!(calls(&m, name), !explicit, "{m}");
                }
            }
            let weak = format!(
                "__attribute__((weak)) {ty} {name}({ty},{ty}); {ty} f({ty} x,{ty} y) {{return {name}(x,y);}}"
            );
            assert!(calls(&compile(&weak, target), name));
            let incompatible =
                format!("{ty} {name}({ty},int); {ty} f({ty} x,int y) {{return {name}(x,y);}}");
            assert!(calls(&compile(&incompatible, target), name));
            for source in [
                format!("{source} {ty} {name}({ty} x,{ty} y) {{return x+y;}}"),
                format!(
                    "static {ty} {name}({ty} x,{ty} y) {{return x+y;}} {ty} f({ty} x,{ty} y) {{return {name}(x,y);}}"
                ),
                format!(
                    "{ty} {name}({ty},{ty},...); {ty} f({ty} x,{ty} y) {{return {name}(x,y);}}"
                ),
            ] {
                assert!(calls(&compile(&source, target), name));
            }
        }
    }
}

#[test]
fn copysign_native_preserves_bits_and_evaluates_each_argument_once() {
    let source = r#"
    double copysign(double,double); float copysignf(float,float);
    int nx,ny;
    double x(void) { ++nx; return -0.0; }
    double y(void) { ++ny; return -1.0; }
    int main(void) {
      if (!__builtin_signbit(copysign(x(), y())) || nx!=1 || ny!=1) return 1;
      unsigned long bits[] = {0,0x8000000000000000UL,1,0x7ff0000000000000UL,0x7ff8000000000042UL,0x7ff0000000000042UL};
      for (int i=0;i<6;++i) for (int j=0;j<6;++j) {
        union {unsigned long u; double f;} a,b,r;
        a.u=bits[i]; b.u=bits[j]; r.f=copysign(a.f,b.f);
        if (r.u != ((a.u&0x7fffffffffffffffUL)|(b.u&0x8000000000000000UL))) return 2;
        r.f=__builtin_copysign(a.f,b.f);
        if (r.u != ((a.u&0x7fffffffffffffffUL)|(b.u&0x8000000000000000UL))) return 3;
      }
      unsigned values[] = {0,0x80000000U,1,0x7f800000U,0x7fc00042U,0x7f800042U};
      for (int i=0;i<6;++i) for (int j=0;j<6;++j) {
        union {unsigned u; float f;} a,b,r;
        a.u=values[i]; b.u=values[j]; r.f=copysignf(a.f,b.f);
        if (r.u != ((a.u&0x7fffffffU)|(b.u&0x80000000U))) return 4;
      }
      return 0;
    }"#;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("copysign-boundaries");
    std::fs::create_dir_all(&dir).unwrap();
    for optimized in [false, true] {
        let target = keel_c::Options::default().target;
        let mut m = compile(source, target);
        if optimized {
            keel_opt::optimize(&mut m).unwrap();
        }
        let asm = dir.join(format!("copy-{optimized}.s"));
        let exe = dir.join(format!("copy-{optimized}"));
        std::fs::write(&asm, keel_codegen::compile(&m, target).unwrap()).unwrap();
        checked(Command::new("clang").arg(asm).arg("-o").arg(&exe));
        checked(&mut Command::new(exe));
    }
}

#[test]
fn copysign_capabilities_and_darwin_long_double_recognition() {
    for target in Target::NATIVE {
        let source = "#if !__has_builtin(__builtin_copysign) || !__has_builtin(__builtin_copysignf)\n#error missing copysign\n#endif\ndouble f(double x,double y){return __builtin_copysign(x,y);}";
        let text = keel_c::preprocess(
            source,
            "copysign.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!calls(&compile(&text, target), "copysign"));
    }
    let m = compile(
        "long double copysignl(long double,long double);long double f(long double x,long double y){return copysignl(x,y);}",
        Target::Aarch64AppleDarwin,
    );
    assert!(!calls(&m, "copysignl"));
}
