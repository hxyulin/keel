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
    std::fs::write(&source, "int gone(void) __attribute__((section(\"__TEXT,__gone\"))); int gone(void) { return 91; }\nint twice(int value) {\n int local = value * 2;\n return local;\n}\nint main(void) { return twice(21)!=42; }\n").unwrap();
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
            dead_strip: true,
            ..Options::default()
        },
    )
    .unwrap();
    {
        use object::{Object, ObjectSymbol};
        let image = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert!(!image.symbols().any(|s| s.name().unwrap() == "_gone"));
    }
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
            dead_strip: true,
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
        "breakpoint set --file memory.c --line 4",
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

#[test]
fn compact_lsda_and_dwarf_exception_frames_work_after_a_regular_page_boundary() {
    use object::{Object, ObjectSection};
    let temp = std::env::temp_dir().join(format!("keel-link-unwind-pages-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let mut source = String::from(".text\n");
    for i in 0..512 {
        source.push_str(&format!(
            "_padding{i}:\n.cfi_startproc\nret\n.cfi_endproc\n"
        ));
    }
    // Force only the throwing frame to DWARF without changing its machine
    // instructions. The catch frame retains compact register rules and LSDA.
    source.push_str(&include_str!("fixtures/compact.s").replace(
        "_compact_throw:\n.cfi_startproc",
        "_compact_throw:\n.cfi_startproc\n.cfi_remember_state\n.cfi_restore_state",
    ));
    let bytes = keel_asm::assemble_object(&source).unwrap();
    let obj = temp.join("mixed.o");
    std::fs::write(&obj, &bytes).unwrap();
    let driver = temp.join("driver.o");
    checked(
        Command::new("clang++")
            .args(["-O2", "-c"])
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compact.cpp"))
            .arg("-o")
            .arg(&driver),
    );
    let driver_bytes = std::fs::read(&driver).unwrap();
    let sdk = checked(Command::new("xcrun").arg("--show-sdk-path"));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    let exe = keel_link::link(
        &[
            Input {
                name: "mixed.o",
                bytes: &bytes,
            },
            Input {
                name: "driver.o",
                bytes: &driver_bytes,
            },
        ],
        Options {
            dead_strip: true,
            dylibs: ["libSystem.tbd", "libc++.tbd"]
                .iter()
                .map(|name| Dylib::from_tbd(&sdk.join("usr/lib").join(name), &sdk).unwrap())
                .collect(),
            ..Options::default()
        },
    )
    .unwrap();
    let image = object::File::parse(exe.bytes.as_slice()).unwrap();
    let unwind = image
        .section_by_name("__unwind_info")
        .unwrap()
        .data()
        .unwrap();
    let word = |at| u32::from_le_bytes(unwind[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(word(24), 3, "two regular pages plus sentinel");
    let index = word(20);
    let second = word(index + 16);
    assert_eq!(word(second), 2);
    let count = u16::from_le_bytes(unwind[second + 6..second + 8].try_into().unwrap()) as usize;
    let encodings: Vec<_> = (0..count).map(|i| word(second + 12 + i * 8)).collect();
    assert!(encodings.iter().any(|e| e & 0x0f000000 == 0x03000000));
    assert!(encodings.iter().any(|e| e & 0x40000000 != 0));
    assert!(image.section_by_name("__eh_frame").unwrap().size() > 0);
    let native = temp.join("native");
    let apple = temp.join("apple");
    keel_link::write_signed(&exe, &native).unwrap();
    checked(
        Command::new("clang++")
            .arg(&obj)
            .arg(&driver)
            .arg("-o")
            .arg(&apple),
    );
    for executable in [&native, &apple] {
        checked(
            Command::new("codesign")
                .args(["--verify", "--strict"])
                .arg(executable),
        );
        // Each run already gets a fresh ASLR slide.
        checked(&mut Command::new(executable));
    }
    std::fs::remove_dir_all(temp).unwrap();
}
