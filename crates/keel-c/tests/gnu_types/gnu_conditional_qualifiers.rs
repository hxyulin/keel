//! GNU omitted-middle conditionals capture values without qualifying the temporary.
use keel_ir::{InstData, Target};
use std::{fs, process::Command};

const SOURCE: &str = r#"
typedef const char *Text;
typedef Text const FixedText;
struct Handler { Text prefix, name; };
const char *prefix(const struct Handler *handler) {
    return handler->prefix ?: handler->name;
}
const char *alias(FixedText text, Text fallback) { return text ?: fallback; }
long number(const long *value, long fallback) { return *value ?: fallback; }
double real(const double *value, double fallback) { return *value ?: fallback; }
long observed(const volatile long *value, long fallback) { return *value ?: fallback; }
int *writable(int *const *value, int *fallback) { return *value ?: fallback; }
volatile int *volatile_pointee(volatile int *const *value, volatile int *fallback) {
    return *value ?: fallback;
}
static unsigned source_calls, fallback_calls;
static Text source_value;
static Text source(void) { ++source_calls; return source_value; }
static Text fallback(void) { ++fallback_calls; return "fallback"; }
Text once(Text value) { source_value=value; return source() ?: fallback(); }
int test(void) {
    struct Handler primary={"prefix","name"}, secondary={0,"name"};
    if(prefix(&primary)!=primary.prefix || prefix(&secondary)!=secondary.name) return 1;
    if(alias(primary.prefix,primary.name)!=primary.prefix || alias(0,primary.name)!=primary.name) return 2;
    const long a=17,b=0;
    if(number(&a,23)!=17 || number(&b,23)!=23) return 3;
    const double x=1.25,y=0.0;
    if(real(&x,2.5)!=1.25 || real(&y,2.5)!=2.5) return 4;
    const volatile long v=37,z=0;
    if(observed(&v,41)!=37 || observed(&z,41)!=41) return 5;
    int n=0, *const p=&n, *const q=0;
    *writable(&p,&n)=43;
    if(n!=43 || writable(&q,&n)!=&n) return 6;
    volatile int m=0, *const r=&m;
    *volatile_pointee(&r,&m)=47;
    if(m!=47) return 7;
    source_calls=fallback_calls=0;
    if(once(primary.prefix)!=primary.prefix || source_calls!=1 || fallback_calls!=0) return 8;
    if(once(0)[0]!='f' || source_calls!=2 || fallback_calls!=1) return 9;
    return 0;
}
"#;

fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "gnu-conditional.c",
        keel_c::Options {
            target,
            debug_info: debug,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn qualified_values_compile_and_execute_without_repeating_effects() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gnu-conditional-qualifiers");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void); int main(void){return test();}",
    )
    .unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    for target in Target::NATIVE {
        for debug in [false, true] {
            let mut module = compile(SOURCE, target, debug);
            for optimize in [false, true] {
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let assembly = keel_codegen::compile(&module, target).unwrap();
                if Some(target) == Target::host() {
                    fs::write(dir.join("generated.s"), assembly).unwrap();
                    let exe = dir.join("run");
                    let result = Command::new("clang")
                        .arg(dir.join("generated.s"))
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    assert!(
                        Command::new(&exe).status().unwrap().success(),
                        "debug={debug}, optimize={optimize}"
                    );
                }
            }
        }
    }
    if let Some(export) = std::env::var_os("KEEL_GNU_CONDITIONAL_EXPORT") {
        let export = std::path::PathBuf::from(export);
        fs::create_dir_all(&export).unwrap();
        fs::write(export.join("source.c"), SOURCE).unwrap();
        fs::copy(dir.join("main.c"), export.join("main.c")).unwrap();
        for debug in [false, true] {
            let mut module = compile(SOURCE, Target::X86_64LinuxGnu, debug);
            for optimize in [false, true] {
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let assembly = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
                fs::write(
                    export.join(format!("conditional-{debug}-{optimize}.o")),
                    keel_asm::x86::assemble_object(&assembly).unwrap(),
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn the_original_volatile_read_is_the_only_volatile_access() {
    for target in Target::NATIVE {
        let module = compile(
            "long f(const volatile long *p,long fallback){return *p ?: fallback;}",
            target,
            false,
        );
        let f = &module.functions[0];
        let loads = f
            .layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i),InstData::Load{flags,..} if flags.volatile))
            .count();
        let stores = f
            .layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i),InstData::Store{flags,..} if flags.volatile))
            .count();
        assert_eq!((loads, stores), (1, 0), "{target:?}");
    }
}

#[test]
fn real_const_writes_and_const_pointees_still_reject() {
    for target in Target::NATIVE {
        for source in [
            "int f(void){const int x=1;x=2;return x;}",
            "int f(const int *p){return ++*p;}",
            "struct S{int x;};void f(const struct S *p){p->x=1;}",
            "typedef const int C;void f(C *p){*p=1;}",
            "int f(const int *const *p,const int *q){*(*p ?: q)=1;return 0;}",
            "int f(const int *p,int *q){*(q ?: p)=1;return 0;}",
        ] {
            let error = keel_c::compile(
                source,
                "const-write.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("const-qualified"),
                "{target:?}: {source}: {error}"
            );
        }
    }
}
