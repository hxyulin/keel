//! Pure byte swaps and single-symbol integer address addends are static constants.
use keel_ir::{DataItem, Target};
use std::{fs, path::Path, process::Command};

const SWAPS: &str = r#"
enum { IP = __builtin_bswap16(0x0800), IPV6 = __builtin_bswap16(0x86dd) };
_Static_assert(IP == 8 && IPV6 == 0xdd86,"network constants");
_Static_assert(__builtin_bswap16(0x123456) == 0x5634,"truncate first");
_Static_assert(__builtin_bswap32(0x123456789UL) == 0x89674523U,"32-bit truncate");
_Static_assert(__builtin_bswap64(0x0123456789abcdefUL) == 0xefcdab8967452301UL,"64 bits");
_Static_assert(__builtin_bswap16(-2) == 0xfeff,"negative conversion");
_Static_assert(__builtin_bswap32(-2) == 0xfeffffffU,"negative conversion 32");
_Static_assert(__builtin_bswap64(-2) == 0xfeffffffffffffffUL,"negative conversion 64");
_Static_assert(__builtin_bswap16(__builtin_bswap16(0x1234)) == 0x1234,"nested");
_Static_assert(__builtin_bswap16(1 ? 0x0800 : 0) == 8,"selected value");
unsigned short stored16=__builtin_bswap16(0x1234);
unsigned int stored32=__builtin_bswap32(0x12345678);
unsigned long stored64=__builtin_bswap64(0x0123456789abcdefUL);
int bounded[__builtin_bswap16(0x0400)];
struct Bits { unsigned flag:__builtin_bswap16(0x0100); };
static unsigned effects;
static unsigned effect(void) {++effects;return 0x1234;}
unsigned short runtime16(unsigned short n){return __builtin_bswap16(n);}
unsigned int runtime32(unsigned int n){return __builtin_bswap32(n);}
unsigned long runtime64(unsigned long n){return __builtin_bswap64(n);}
int protocol(unsigned short p){switch(p){case IP:return 1;case IPV6:return 2;default:return 0;}}
int large_case(unsigned long x){switch(x){case __builtin_bswap64(0x0123456789abcdefUL):return 1;default:return 0;}}
int check_swaps(void) {
    if(stored16!=0x3412 || stored32!=0x78563412 || stored64!=0xefcdab8967452301UL) return 1;
    if(sizeof(bounded)!=4*sizeof(int) || sizeof(struct Bits)!=sizeof(unsigned)) return 2;
    if(protocol(8)!=1 || protocol(0xdd86)!=2 || protocol(0)!=0 || !large_case(stored64)) return 3;
    unsigned long inputs[]={0,1,0x80,0xffff,0x80000000UL,0x0123456789abcdefUL,0xffffffffffffffffUL};
    for(unsigned i=0;i<7;++i) {
        unsigned long n=inputs[i],expected=0;
        for(unsigned j=0;j<8;++j) expected=(expected<<8)|((n>>(8*j))&255);
        if(runtime64(n)!=expected || runtime32(n)!=(unsigned int)(expected>>32) || runtime16(n)!=(unsigned short)(expected>>48)) return 4;
    }
    effects=0;
    if(__builtin_constant_p(__builtin_bswap16((effect(),0x0800))) || effects) return 5;
    if(!__builtin_constant_p(__builtin_bswap16(0x0800))) return 6;
    if(__builtin_bswap16((effect(),0x0800))!=8 || effects!=1) return 7;
    return 0;
}
"#;
const ADDRESSES: &str = r#"
unsigned long stack[4];
unsigned long top=(unsigned long)&stack + sizeof(stack) - 0;
unsigned long backtracked=(unsigned long)&stack + sizeof(stack) - 8;
unsigned long commuted=16 + (unsigned long)&stack;
unsigned long *through_pointer=(unsigned long *)((unsigned long)&stack+8)+1;
int check_addresses(void) {
    if(top!=(unsigned long)(stack+4) || backtracked!=(unsigned long)(stack+3) || commuted!=(unsigned long)(stack+2) || through_pointer!=stack+2) return 10;
    return 0;
}
"#;
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let m = keel_c::compile(
        source,
        "constant-expressions.c",
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
fn byte_swaps_are_integer_constants_on_native_targets() {
    for target in Target::NATIVE {
        compile(SWAPS, target, false);
    }
}
#[test]
fn integer_addresses_preserve_single_symbol_relocations() {
    for target in Target::NATIVE {
        let m = compile(ADDRESSES, target, false);
        for (name, addend) in [
            ("top", 32),
            ("backtracked", 24),
            ("commuted", 16),
            ("through_pointer", 16),
        ] {
            let data = m.data.iter().find(|d| d.name == name).unwrap();
            assert!(
                matches!(&data.init.as_ref().unwrap().items[..],[DataItem::Ptr{symbol,addend:a}] if symbol=="stack" && *a==addend),
                "{target:?}: {name}"
            );
        }
    }
}
#[test]
fn direct_pointer_truth_and_weak_relocation_binding_stay_distinct() {
    let source = "struct S{int a,b;};struct S object;int direct=&object.b?1:0;extern long optional __attribute__((weak));unsigned long plus=(unsigned long)&optional+4;";
    for target in Target::NATIVE {
        let m = compile(source, target, false);
        let optional = m.data.iter().find(|d| d.name == "optional").unwrap();
        assert!(optional.weak && optional.init.is_none());
        let plus = m.data.iter().find(|d| d.name == "plus").unwrap();
        assert!(
            matches!(&plus.init.as_ref().unwrap().items[..],[DataItem::Ptr{symbol,addend:4}] if symbol=="optional")
        );
    }
}
#[test]
fn effects_variables_wide_intermediates_and_unrepresentable_addresses_reject() {
    for source in [
        "unsigned effect(void);int x=__builtin_bswap16((effect(),0x0800));",
        "unsigned n;int x=__builtin_bswap16(n);",
        "unsigned effect(void);int f(int n){switch(n){case __builtin_bswap16((effect(),0x0800)):return 1;}return 0;}",
        "_Static_assert(__builtin_bswap64((((unsigned __int128)1<<64)>>1))==0,\"unsupported wide intermediate\");",
        "int x;int v=__builtin_bswap16((unsigned long)&x);",
        "int a,b;unsigned long value=(unsigned long)&a+(unsigned long)&b;",
        "int a;unsigned long value=(unsigned long)&a-8;",
        "int a;struct S{unsigned long bits:1;};struct S value={(unsigned long)&a+4};",
        "int a;unsigned long value=(unsigned long)&a+4294967296UL;",
        "int a;unsigned int value=(unsigned int)&a+4;",
        "int a;unsigned long value=((unsigned long)&a+4)&~7UL;",
        "int a;int value=((unsigned long)&a+4)?1:0;",
        "int a;int value=!((unsigned long)&a+4);",
        "int a;int value=(void *)((unsigned long)&a+4)?1:0;",
        "void *f(void *p){return __builtin_assume_aligned(p,__builtin_bswap16((0,0x0800)));}",
        "int a;extern int weak __attribute__((weak));unsigned long value=&weak?(unsigned long)&a+4:0;",
        "int a;void effect(void);unsigned long value=(effect(),(unsigned long)&a+4);",
    ] {
        assert!(
            keel_c::compile(
                source,
                "not-constant.c",
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
fn constants_and_runtime_operations_match_clang_and_native_execution() {
    let source = format!(
        "{SWAPS}\n{ADDRESSES}\nint test(void){{int n=check_swaps();return n?n:check_addresses();}}"
    );
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("constant-expressions");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), &source).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    let export = std::env::var_os("KEEL_CONSTANT_EXPRESSIONS_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), &source).unwrap();
        fs::copy(
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("constant-expressions/main.c"),
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
                        dir.join(format!("constant-{debug}-{optimize}.o")),
                        keel_asm::x86::assemble_object(&assembly).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    let result = Command::new("clang")
        .args(["-O2"])
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
