//! Switch exits unwind only the cleanup scopes they leave.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
static int log_value;
static void record(int *p){log_value=log_value*10+*p;}
int choose(int n){
 int outer __attribute__((cleanup(record)))=9;
 int result=0;
 switch(n){
 case 0:{int a __attribute__((cleanup(record)))=1;result=10;break;}
 case 1:{int a __attribute__((cleanup(record)))=2;return 20;}
 case 2:{int a __attribute__((cleanup(record)))=3;result=30;}
 case 3:{int a __attribute__((cleanup(record)))=4;result+=40;break;}
 default:{int a __attribute__((cleanup(record)))=5;result=50;}
 }
 return result;
}
int loop_switch(void){
 for(int i=0;i<3;++i){
  int outer __attribute__((cleanup(record)))=i+1;
  switch(i){case 0:{int x __attribute__((cleanup(record)))=4;continue;}
  case 1:{int x __attribute__((cleanup(record)))=5;break;}
  default:{int x __attribute__((cleanup(record)))=6;}}
 }
 return log_value;
}
int test(void){
 int results[]={10,20,70,40,50},logs[]={19,29,349,49,59};
 for(int i=0;i<5;++i){log_value=0;if(choose(i)!=results[i]||log_value!=logs[i])return i+1;}
 log_value=0;if(loop_switch()!=415263)return 6;
 return 0;
}
"#;
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let m = keel_c::compile(
        source,
        "cleanup-switch.c",
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
fn switch_cleanups_match_native_reference() {
    let source = SOURCE.to_owned();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cleanup-switch");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), &source).unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int test(void);int main(void){return test();}",
    )
    .unwrap();
    let export = std::env::var_os("KEEL_CLEANUP_SWITCH_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), &source).unwrap();
        fs::copy(
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("cleanup-switch/main.c"),
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
                        dir.join(format!("switch-{debug}-{optimize}.o")),
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
