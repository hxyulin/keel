#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use keel_link::{Dylib, Input, Options};
use std::{path::PathBuf, process::Command};
fn checked(cmd: &mut Command) -> std::process::Output {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
#[test]
fn signed_pie_imports_data_initializers_and_unwind_match_apple() {
    let temp = std::env::temp_dir().join(format!("keel-link-native-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let sdk = checked(Command::new("xcrun").args(["--show-sdk-path"]));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    let system = Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap();
    let source = r#"
.globl _main
_main:
.cfi_startproc
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x29,-16
.cfi_offset x30,-8
adrp x0,_message@PAGE
add x0,x0,_message@PAGEOFF
bl _puts
adrp x9,_initialized@PAGE
ldr w0,[x9,_initialized@PAGEOFF]
ldp x29,x30,[sp],#16
ret
.cfi_endproc
 .globl _init
_init:
adrp x9,_initialized@PAGE
mov w10,#7
str w10,[x9,_initialized@PAGEOFF]
ret
.section __TEXT,__cstring,cstring_literals
_message:
.asciz "native linker"
.data
_initialized:
.long 0
"#;
    let bytes = keel_asm::assemble_object(source).unwrap();
    let obj = temp.join("fixture.o");
    std::fs::write(&obj, &bytes).unwrap();
    // The Keel assembler has no mod_init_funcs directive yet; use Apple's
    // assembler for this one section and exercise foreign-object ingestion.
    let init_source = temp.join("init.s");
    let init_object = temp.join("init.o");
    std::fs::write(
        &init_source,
        ".section __DATA,__mod_init_func,mod_init_funcs\n.p2align 3\n.quad _init\n",
    )
    .unwrap();
    checked(
        Command::new("clang")
            .arg("-c")
            .arg(&init_source)
            .arg("-o")
            .arg(&init_object),
    );
    let init_bytes = std::fs::read(&init_object).unwrap();
    let exe = keel_link::link(
        &[
            Input {
                name: "fixture.o",
                bytes: &bytes,
            },
            Input {
                name: "init.o",
                bytes: &init_bytes,
            },
        ],
        Options {
            dylibs: vec![system],
            ..Options::default()
        },
    )
    .unwrap();
    let native = temp.join("native");
    let apple = temp.join("apple");
    keel_link::write_signed(&exe, &native).unwrap();
    checked(
        Command::new("clang")
            .arg(&obj)
            .arg(&init_object)
            .arg("-o")
            .arg(&apple),
    );
    for exe in [&native, &apple] {
        checked(
            Command::new("codesign")
                .args(["--verify", "--strict"])
                .arg(exe),
        );
    }
    let a = Command::new(&native).output().unwrap();
    let b = Command::new(&apple).output().unwrap();
    assert_eq!(
        a.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&a.stderr)
    );
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(a.stdout, b"native linker\n");
    checked(Command::new("otool").arg("-l").arg(&native));
    checked(
        Command::new("dyld_info")
            .args(["-imports", "-exports", "-fixups"])
            .arg(&native),
    );
    std::fs::remove_dir_all(temp).unwrap();
}
