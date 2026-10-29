//! Compact GNU intervals preserve C switch dispatch and scope exits.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
static int calls, log_value;
static unsigned int selector(unsigned int x){++calls;return x;}
static void record(int *p){log_value=log_value*10+*p;}
int elf(unsigned int x){
 switch(selector(x)){
 case 7: return 7;
 case 0x70000000 ... 0x7fffffff: return 1;
 case 0x80000000U ... 0xfffffffeU: return 2;
 default: return 3;
 }
}
int signed_range(long long x){
 switch(x){
 case (-9223372036854775807LL-1) ... -2: return 1;
 case 0 ... 9223372036854775806LL: return 2;
 default:return 3;
 }
}
int all_signed(long long x){
 switch(x){case (-9223372036854775807LL-1) ... 9223372036854775807LL:return 4;}
 return 0;
}
int unsigned_range(unsigned long long x){
 switch(x){case 0x8000000000000000ULL ... 0xfffffffffffffffeULL:return 5;
 default:return 6;}
}
int all_unsigned(unsigned long long x){
 switch(x){case 0 ... 18446744073709551615ULL:return 8;default:return 0;}
}
int converted(unsigned int x){
 switch(x){case 0x100000001LL ... 0x100000003LL:return 9;
 case -3 ... -1:return 10;default:return 0;}
}
int promoted(unsigned char x){
 switch(x){case -1000000 ... -1:return 1;case 255:return 2;default:return 0;}
}
int cleanup(int n){
 int outer __attribute__((cleanup(record)))=9;
 int result=0;
 switch(n){
 default:{int a __attribute__((cleanup(record)))=5;result=50;break;}
 case -1000000 ... -1:{int a __attribute__((cleanup(record)))=1;result=10;}
 case 0:{int a __attribute__((cleanup(record)))=2;result+=20;break;}
 case 1 ... 1000000:{int a __attribute__((cleanup(record)))=3;return 40;}
 }
 return result;
}
int loop(void){
 for(int i=0;i<3;++i){
  int a __attribute__((cleanup(record)))=i+1;
  switch(i){case 0 ... 1:{int b __attribute__((cleanup(record)))=4;continue;}
  default:break;}
 }
 return log_value;
}
int test(void){
 unsigned int e[]={0,7,0x6fffffff,0x70000000,0x7fffffff,0x80000000U,0xfffffffeU,0xffffffffU};
 int er[]={3,7,3,1,1,2,2,3};
 for(int i=0;i<8;++i){calls=0;if(elf(e[i])!=er[i]||calls!=1)return 1;}
 long long s[]={(-9223372036854775807LL-1),-2,-1,0,9223372036854775806LL,9223372036854775807LL};
 int sr[]={1,1,3,2,2,3};
 for(int i=0;i<6;++i)if(signed_range(s[i])!=sr[i]||all_signed(s[i])!=4)return 2;
 unsigned long long u[]={0,0x7fffffffffffffffULL,0x8000000000000000ULL,0xfffffffffffffffeULL,0xffffffffffffffffULL};
 int ur[]={6,6,5,5,6};
 for(int i=0;i<5;++i)if(unsigned_range(u[i])!=ur[i]||all_unsigned(u[i])!=8)return 3;
 if(converted(0)!=0||converted(1)!=9||converted(3)!=9||converted(4)!=0||converted(0xfffffffcU)!=0||converted(0xfffffffdU)!=10||converted(0xffffffffU)!=10)return 4;
 if(promoted(255)!=2||promoted(0)!=0)return 5;
 int c[]={-1,0,1,1000001},cr[]={30,20,40,50},cl[]={129,29,39,59};
 for(int i=0;i<4;++i){log_value=0;if(cleanup(c[i])!=cr[i]||log_value!=cl[i])return 6;}
 log_value=0;if(loop()!=41423)return 7;
 return 0;
}
"#;
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let m = keel_c::compile(
        source,
        "switch-ranges.c",
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
fn compact_ranges_match_native_reference() {
    let source = SOURCE.to_owned();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("switch-ranges");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), &source).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    for target in Target::NATIVE {
        for debug in [false, true] {
            let mut m = compile(&source, target, debug);
            for optimize in [false, true] {
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let assembly = keel_codegen::compile(&m, target).unwrap();
                if target == Target::X86_64LinuxGnu {
                    keel_asm::x86::assemble_object(&assembly).unwrap();
                }
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

#[test]
fn converted_ranges_reject_overlaps_and_empty_intervals() {
    for (ty, labels) in [
        ("int", "case 1 ... 1000000:; case 1000000:;"),
        ("int", "case 500000:; case 1 ... 1000000:;"),
        ("int", "case 1 ... 1000000:; case 2 ... 999999:;"),
        ("int", "case 2 ... 999999:; case 1 ... 1000000:;"),
        ("int", "case -1000000 ... -1:; case -10 ... 10:;"),
        ("unsigned int", "case 1 ... 3:; case 0x100000002LL:;"),
        ("unsigned int", "case -3 ... -1:; case 0xfffffffeU:;"),
        (
            "unsigned long long",
            "case 0 ... 0xffffffffffffffffULL:; case 7:;",
        ),
        (
            "unsigned long long",
            "case 0x8000000000000000ULL ... 0xffffffffffffffffULL:; case 0x8000000000000001ULL:;",
        ),
    ] {
        let source = format!("int f({ty} x) {{ switch(x) {{ {labels} }} return 0; }}");
        let err = keel_c::compile(&source, "overlap.c", keel_c::Options::default()).unwrap_err();
        assert!(err.message.contains("duplicate case"), "{labels}: {err}");
    }
    for source in [
        "int f(int x){switch(x){case 10 ... 1: return 1;}return 0;}",
        "int f(unsigned int x){switch(x){case -1 ... 1:return 1;}return 0;}",
    ] {
        let err = keel_c::compile(source, "empty.c", keel_c::Options::default()).unwrap_err();
        assert!(err.message.contains("case range is empty"), "{err}");
    }
}

#[test]
fn duplicate_case_names_the_smallest_reused_value() {
    for (ty, labels, value) in [
        ("int", "case 1 ... 5:; case 3 ... 9:;", "3"),
        ("int", "case 4 ... 5:; case 1 ... 9:;", "4"),
        // The new range overlaps two earlier ranges; the lower one wins.
        ("int", "case 7 ... 8:; case 2 ... 3:; case 0 ... 9:;", "2"),
        ("int", "case -5 ... -1:; case 2:; case -9 ... 9:;", "-5"),
        (
            "unsigned int",
            "case 0xfffffff0U:; case 5:; case 1 ... -1:;",
            "5",
        ),
    ] {
        let source = format!("int f({ty} x) {{ switch(x) {{ {labels} }} return 0; }}");
        let err = keel_c::compile(&source, "overlap.c", keel_c::Options::default()).unwrap_err();
        assert_eq!(err.message, format!("duplicate case {value}"), "{labels}");
    }
    // Many disjoint cases stay accepted, and a late duplicate is still found.
    let mut labels: String = (0..8000).map(|i| format!("case {}: x++;", i * 3)).collect();
    let source = format!("int f(int x) {{ switch(x) {{ {labels} return 1; }} return 0; }}");
    keel_c::compile(&source, "many.c", keel_c::Options::default()).unwrap();
    labels.push_str("case 9000 ... 9002: x++;");
    let source = format!("int f(int x) {{ switch(x) {{ {labels} return 1; }} return 0; }}");
    let err = keel_c::compile(&source, "many.c", keel_c::Options::default()).unwrap_err();
    assert_eq!(err.message, "duplicate case 9000");
}
