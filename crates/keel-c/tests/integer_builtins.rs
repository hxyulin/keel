//! GNU bit-search and digit builtins keep their widths and evaluate once.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
_Static_assert(__builtin_ffs(0)==0,"zero");
_Static_assert(__builtin_ffs(0x80000000U)==32,"sign bit");
_Static_assert(__builtin_ffsl(1UL<<40)==41,"long");
_Static_assert(__builtin_ffsll(1ULL<<63)==64,"long long");
_Static_assert(__builtin_ffs(1ULL<<32)==0,"convert to int first");
_Static_assert(__builtin_ffsll(-8)==4,"signed conversion");
_Static_assert(__builtin_isdigit('0') && __builtin_isdigit('9'),"digits");
_Static_assert(!__builtin_isdigit('/') && !__builtin_isdigit(':') && !__builtin_isdigit(-1),"non digits");
static int calls;
static long long next(void){++calls;return 1LL<<40;}
int find32(int n){return __builtin_ffs(n);}
int findlong(long n){return __builtin_ffsl(n);}
int find64(long long n){return __builtin_ffsll(n);}
int digit(int n){return !!__builtin_isdigit(n);}
struct S{int n;};
typedef const struct S *ConstPointer;
_Static_assert(_Generic((ConstPointer)0,struct S *:0,ConstPointer:1),"typedef qualifiers");
_Static_assert(_Generic((struct S *const)0,struct S *:1,default:0),"outer qualifiers dropped");
_Static_assert(_Generic((volatile struct S *)0,struct S *:0,volatile struct S *:1),"volatile differs");
_Static_assert(_Generic((const struct S **)0,struct S **:0,const struct S **:1),"nested qualifier");
int generic_write(struct S *p){_Generic((p), const struct S *: (const struct S *)(p), default: (struct S *)(p))->n++;return p->n;}
_Static_assert(_Generic((struct S *)0,const struct S *:0,struct S *:1),"pointee const differs");
_Static_assert(_Generic((const struct S *)0,struct S *:0,const struct S *:1),"const pointee");
int test(void){
    unsigned long long samples[]={0,1,2,3,8,0x80000000ULL,0x100000000ULL,1ULL<<63,~0ULL};
    for(unsigned j=0;j<9;++j){
        unsigned long long n=samples[j]; int want=0;
        if(n){want=1;while(!(n&1)){++want;n>>=1;}}
        if(find64(samples[j])!=want || findlong(samples[j])!=want)return 1;
        unsigned m=samples[j];want=0;
        if(m){want=1;while(!(m&1)){++want;m>>=1;}}
        if(find32(samples[j])!=want)return 2;
    }
    for(int n=-1;n<256;++n)if(digit(n)!=(n>=48&&n<=57))return 3;
    calls=0;if(__builtin_ffsll(next())!=41||calls!=1)return 4;
    calls=0;if(__builtin_isdigit((calls++,'5'))==0||calls!=1)return 5;
    struct S s={12};if(generic_write(&s)!=13)return 7;
    calls=0;if(__builtin_constant_p(__builtin_ffsll((calls++,8)))||calls)return 6;
    return 0;
}
"#;
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let m = keel_c::compile(
        source,
        "integer-builtins.c",
        keel_c::Options {
            target,
            debug_info: debug,
        },
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
#[test]
fn builtin_capabilities_and_invalid_calls_match_support() {
    let queries = "__has_builtin(__builtin_ffs) __has_builtin(__builtin_ffsl) __has_builtin(__builtin_ffsll) __has_builtin(__builtin_isdigit)";
    for target in Target::ALL {
        let text = keel_c::preprocess(
            queries,
            "capabilities.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let values: Vec<_> = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .flat_map(str::split_whitespace)
            .filter(|token| matches!(*token, "0" | "1"))
            .collect();
        assert_eq!(
            values,
            if target == Target::Wasm32UnknownUnknown {
                vec!["0"; 4]
            } else {
                vec!["1"; 4]
            },
            "{target:?}: {text}"
        );
    }
    for source in [
        "int f(void){return __builtin_ffs();}",
        "int f(void){return __builtin_isdigit(1,2);}",
        "int x;int value=__builtin_ffs(x);",
        "struct S{int n;};int f(const struct S *p){_Generic(p,struct S *:(struct S *)p,default:p)->n++;return 0;}",
        "int side(void);int value=__builtin_ffs((side(),8));",
        "int value=__builtin_ffsll((((unsigned __int128)1<<64)>>1));",
    ] {
        assert!(
            keel_c::compile(
                source,
                "invalid.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "accepted {source}"
        );
    }
}
#[test]
fn bit_search_and_digit_tests_match_native_reference() {
    let source = SOURCE.to_owned();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("integer-builtins");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), &source).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    let export = std::env::var_os("KEEL_INTEGER_BUILTINS_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), &source).unwrap();
        fs::copy(
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("integer-builtins/main.c"),
            dir.join("main.c"),
        )
        .unwrap();
    }
    for target in Target::NATIVE {
        for debug in [false, true] {
            let mut m = compile(&source, target, debug);
            for optimize in [false, true] {
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let assembly = keel_codegen::compile(&m, target).unwrap();
                if Some(target) == Target::host() {
                    fs::write(dir.join("generated.s"), &assembly).unwrap();
                    let output = Command::new("clang")
                        .arg(dir.join("generated.s"))
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(
                        Command::new(dir.join("run")).status().unwrap().success(),
                        "{target:?}: debug={debug}, optimize={optimize}"
                    );
                }
                if target == Target::X86_64LinuxGnu
                    && let Some(dir) = &export
                {
                    fs::write(
                        dir.join(format!("integer-{debug}-{optimize}.o")),
                        keel_asm::x86::assemble_object(&assembly).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    // Clang does not implement GCC's isdigit builtin. Its reference uses a
    // decimal-range expression; generated code is also checked for every byte.
    let result = Command::new("clang")
        .args(["-O2", "-D__builtin_isdigit(x)=((unsigned)(x)-48U<10U)"])
        .arg(dir.join("source.c"))
        .arg(dir.join("main.c"))
        .arg("-o")
        .arg(dir.join("reference"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        Command::new(dir.join("reference"))
            .status()
            .unwrap()
            .success()
    );
}
