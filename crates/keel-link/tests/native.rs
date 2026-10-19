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

#[test]
fn debug_map_snapshots_in_memory_inputs_for_standalone_dsym() {
    let temp = std::env::temp_dir().join(format!("keel-link-debug-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let source = temp.join("memory.c");
    let object = temp.join("original.o");
    std::fs::write(&source, "int twice(int value) {\n int local = value * 2;\n return local;\n}\nint main(void) { return twice(21)!=42; }\n").unwrap();
    checked(
        Command::new("clang")
            .args(["-g", "-O0", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&object),
    );
    let bytes = std::fs::read(&object).unwrap();
    std::fs::remove_file(object).unwrap();
    let sdk = checked(Command::new("xcrun").arg("--show-sdk-path"));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    let exe = keel_link::link(
        &[Input {
            name: "in-memory.o",
            bytes: &bytes,
        }],
        Options {
            dylibs: vec![Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap()],
            debug_directory: Some(temp.join("snapshots")),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(exe.debug_objects.len(), 1);
    assert_eq!(exe.debug_objects[0].bytes, bytes);
    assert!(exe.warnings.is_empty());
    let executable = temp.join("memory");
    keel_link::write_signed(&exe, &executable).unwrap();
    let buffered = std::fs::read(&executable).unwrap();
    let warnings = keel_link::link_signed(
        &[Input {
            name: "in-memory.o",
            bytes: &bytes,
        }],
        Options {
            dylibs: vec![Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap()],
            debug_directory: Some(temp.join("snapshots")),
            ..Options::default()
        },
        &executable,
    )
    .unwrap();
    assert!(warnings.is_empty());
    assert_eq!(buffered, std::fs::read(&executable).unwrap());
    checked(&mut Command::new(&executable));
    checked(Command::new("dsymutil").arg(&executable));
    std::fs::remove_dir_all(temp.join("snapshots")).unwrap();
    let lldb = checked(Command::new("lldb").arg("-b").arg(&executable).args([
        "-o",
        "breakpoint set --file memory.c --line 3",
        "-o",
        "run",
        "-o",
        "frame variable",
        "-o",
        "bt",
        "-o",
        "continue",
    ]));
    let output = String::from_utf8_lossy(&lldb.stdout);
    assert!(
        output.contains("local = 42") && output.contains("value = 21") && output.contains("`main"),
        "{output}"
    );
    std::fs::remove_dir_all(temp).unwrap();
}
