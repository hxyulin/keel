use keel_link::{Dylib, Executable, Input, Options};
use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::BTreeSet;

fn assemble(source: &str) -> Vec<u8> {
    keel_asm::assemble_object(source).unwrap()
}
fn options(gc: bool) -> Options {
    Options {
        dead_strip: gc,
        dylibs: vec![Dylib {
            install_name: "/usr/lib/libSystem.B.dylib".into(),
            exports: ["_puts", "_p0"].into_iter().collect(),
            weak_exports: Default::default(),
        }],
        ..Default::default()
    }
}
fn link(objects: &[Vec<u8>], opts: Options) -> Result<Executable, keel_link::Error> {
    keel_link::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "gc.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        opts,
    )
}
fn names(exe: &Executable) -> BTreeSet<String> {
    object::File::parse(exe.bytes.as_slice())
        .unwrap()
        .symbols()
        .map(|s| s.name().unwrap().to_owned())
        .collect()
}
#[test]
fn dead_cycles_undefined_imports_and_shared_sections() {
    let main = assemble(".globl _main\n_main:\nmov w0,#0\nret\n.globl _shared\n_shared:\nret\n");
    let dead = assemble(".globl _dead\n_dead:\nbl _missing\nbl _puts\nb _cycle\n");
    let cycle = assemble(".globl _cycle\n_cycle:\nb _dead\n");
    let objects = [main, dead, cycle];
    assert!(
        link(&objects, options(false))
            .err()
            .unwrap()
            .contains("_missing")
    );
    let exe = link(&objects, options(true)).unwrap();
    let n = names(&exe);
    assert!(n.contains("_shared"));
    for dead in ["_dead", "_cycle", "_puts", "_missing"] {
        assert!(!n.contains(dead), "{dead}");
    }
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(file.section_by_name("__stubs").unwrap().size(), 0);
    let mut opts = options(true);
    opts.required_exports.push("_dead".into());
    assert!(link(&objects, opts).err().unwrap().contains("_missing"));
}
#[test]
fn roots_local_pointer_edges_commons_bounds_and_data_const_mapping() {
    let objects = [assemble(
        r#"
.section __TEXT,__entry,regular,pure_instructions
.globl _main
_main:
adrp x0,section$start$__DATA_CONST$registry@PAGE
add x0,x0,section$start$__DATA_CONST$registry@PAGEOFF
mov w0,#0
ret
.section __DATA_CONST,registry
.no_dead_strip _registration
_registration:
.quad Lcallback
.section __TEXT,__callback,regular,pure_instructions
Lcallback:
adrp x9,_common@PAGE
add x9,x9,_common@PAGEOFF
ret
.comm _common,16,4
.comm _dead_common,16,4
.section __DATA,__kept
.globl _kept
.no_dead_strip _kept
_kept:
.quad 7
.section __DATA,__used
.globl _used
_used:
.quad 9
"#,
    )];
    let exe = link(&objects, options(true)).unwrap();
    let n = names(&exe);
    assert!(n.contains("_kept") && n.contains("_common"));
    assert!(!n.contains("_used") && !n.contains("_dead_common"));
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let registry = file.section_by_name("registry").unwrap();
    assert_eq!(registry.segment_name().unwrap(), Some("__DATA"));
    assert_eq!(registry.size(), 8);
    assert!(file.section_by_name("__callback").is_some());
    assert!(file.section_by_name("__used").is_none());
}
#[test]
fn commons_from_other_objects_satisfy_references() {
    let main = assemble(
        ".globl _main\n_main:\nadrp x0,_shared@PAGE\nadd x0,x0,_shared@PAGEOFF\nmov w0,#0\nret\n",
    );
    let commons = assemble(".comm _shared,8,3\n.comm _unused,8,3\n.comm _other,8,3\n");
    let other = assemble(".globl _f\n_f:\nadrp x0,_other@PAGE\nret\n");
    let exe = link(&[main, commons, other], options(true)).unwrap();
    let n = names(&exe);
    assert!(n.contains("_shared"));
    for dead in ["_unused", "_other", "_f"] {
        assert!(!n.contains(dead), "{dead}");
    }
}
#[test]
fn boundaries_do_not_root_contributions_and_exports_are_explicit_roots() {
    let main = assemble(
        ".globl _main\n_main:\nadrp x0,section$end$__DATA$registry@PAGE\nadd x0,x0,section$end$__DATA$registry@PAGEOFF\nret\n",
    );
    let record = assemble(".section __DATA,registry\n.globl _record\n_record:\n.quad 42\n");
    let objects = [main, record];
    let exe = link(&objects, options(true)).unwrap();
    assert_eq!(
        object::File::parse(exe.bytes.as_slice())
            .unwrap()
            .section_by_name("registry")
            .unwrap()
            .size(),
        0
    );
    let mut opts = options(true);
    opts.required_exports.push("_record".into());
    assert!(names(&link(&objects, opts).unwrap()).contains("_record"));
    let mut opts = options(true);
    opts.required_exports.push("_absent".into());
    assert!(
        link(&objects, opts)
            .err()
            .unwrap()
            .contains("required export `_absent`")
    );
}
#[test]
fn compact_unwind_owned_by_code_does_not_root_dead_imports() {
    let objects = [
        assemble(".globl _main\n_main:\n.cfi_startproc\nmov w0,#0\nret\n.cfi_endproc\n"),
        assemble(
            ".globl _dead\n_dead:\nret\n.section __LD,__compact_unwind\n.quad _dead\n.long 4\n.long 0x02001000\n.quad _missing_personality\n.quad 0\n",
        ),
    ];
    let exe = link(&objects, options(true)).unwrap();
    assert!(!names(&exe).contains("_dead"));
    assert!(
        link(&objects, options(false))
            .err()
            .unwrap()
            .contains("_missing_personality")
    );
}
#[test]
fn shared_cie_is_rebuilt_after_fde_collection() {
    let source = r#"
.subsections_via_symbols
.section __TEXT,__dead,regular,pure_instructions
.globl _dead
_dead:
.cfi_startproc
.cfi_def_cfa sp,32
ret
.cfi_endproc
.section __TEXT,__entry,regular,pure_instructions
.globl _main
_main:
.cfi_startproc
.cfi_def_cfa sp,32
mov w0,#0
ret
.cfi_endproc
"#;
    let objects = [assemble(source)];
    let full = link(&objects, options(false)).unwrap();
    let gc = link(&objects, options(true)).unwrap();
    let full = object::File::parse(full.bytes.as_slice()).unwrap();
    let stripped = object::File::parse(gc.bytes.as_slice()).unwrap();
    assert!(!names(&gc).contains("_dead"));
    assert!(
        stripped.section_by_name("__eh_frame").unwrap().size()
            < full.section_by_name("__eh_frame").unwrap().size()
    );
}
#[test]
fn weak_winner_edges_ignore_dead_loser_dependencies() {
    let objects = [
        assemble(".globl _main\n_main:\nb _choice\n"),
        assemble(
            ".globl _choice\n.weak_definition _choice\n_choice:\n.cfi_startproc\nb _missing\n.cfi_endproc\n",
        ),
        assemble(".globl _choice\n_choice:\n.cfi_startproc\nmov w0,#0\nret\n.cfi_endproc\n"),
    ];
    let exe = link(&objects, options(true)).unwrap();
    assert!(names(&exe).contains("_choice"));
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let path = std::env::temp_dir().join(format!("keel-gc-weak-{}", std::process::id()));
        keel_link::write_signed(&exe, &path).unwrap();
        assert!(
            std::process::Command::new(&path)
                .status()
                .unwrap()
                .success()
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_shared_cie_backtrace_init_and_boundary_callback_match_apple() {
    use std::{fs, path::PathBuf, process::Command};
    fn run(command: &mut Command) -> std::process::Output {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    let dir = std::env::temp_dir().join(format!("keel-gc-unwind-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = r#"
.subsections_via_symbols
.section __TEXT,__dead,regular,pure_instructions
.globl _dead
_dead:
.cfi_startproc
.cfi_remember_state
.cfi_restore_state
ret
.cfi_endproc
.section __TEXT,__entry,regular,pure_instructions
.globl _main
_main:
.cfi_startproc
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x29,-16
.cfi_offset x30,-8
adrp x9,_state@PAGE
ldr w9,[x9,_state@PAGEOFF]
cmp w9,#7
b.ne Lbad
adrp x9,section$start$__DATA_CONST$registry@PAGE
add x9,x9,section$start$__DATA_CONST$registry@PAGEOFF
ldr x9,[x9]
blr x9
b Ldone
Lbad:
mov w0,#9
Ldone:
ldp x29,x30,[sp],#16
ret
.cfi_endproc
.section __TEXT,__worker,regular,pure_instructions
.globl _worker
_worker:
.cfi_startproc
stp x29,x30,[sp,#-32]!
mov x29,sp
.cfi_def_cfa sp,32
.cfi_offset x29,-32
.cfi_offset x30,-24
.cfi_remember_state
.cfi_restore_state
bl _trace
ldp x29,x30,[sp],#32
ret
.cfi_endproc
.section __TEXT,__boot,regular,pure_instructions
_boot:
adrp x9,_state@PAGE
mov w10,#7
str w10,[x9,_state@PAGEOFF]
ret
.section __DATA,__state
_state:
.long 0
.section __DATA_CONST,registry
.globl _table
.no_dead_strip _table
_table:
.quad _worker
.section __DATA,__mod_init_func,mod_init_funcs
.p2align 3
.quad _boot
"#;
    let bytes = assemble(source);
    fs::write(dir.join("input.o"), &bytes).unwrap();
    fs::write(dir.join("trace.c"), "#include <execinfo.h>\nint trace(void) { void *frames[32]; return backtrace(frames,32) < 3; }\n").unwrap();
    run(Command::new("clang")
        .args(["-O0", "-c"])
        .arg(dir.join("trace.c"))
        .arg("-o")
        .arg(dir.join("trace.o")));
    let trace = fs::read(dir.join("trace.o")).unwrap();
    let sdk = run(Command::new("xcrun").arg("--show-sdk-path"));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    let mut opts = options(true);
    opts.dylibs = vec![Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap()];
    let exe = link(&[bytes, trace], opts).unwrap();
    assert!(!names(&exe).contains("_dead"));
    let native = dir.join("native");
    let apple = dir.join("apple");
    keel_link::write_signed(&exe, &native).unwrap();
    run(Command::new("clang")
        .arg(dir.join("input.o"))
        .arg(dir.join("trace.o"))
        .arg("-Wl,-dead_strip")
        .arg("-o")
        .arg(&apple));
    for path in [&native, &apple] {
        run(Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(path));
        run(&mut Command::new(path));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn addend_prefix_and_subtractor_operands_are_edges_not_symbol_field_guesses() {
    let objects = [assemble(
        r#"
.globl _main
_main:
b _helper+4096
.section __TEXT,__helper,regular,pure_instructions
.globl _helper
_helper:
.space 4096
ret
.section __TEXT,__positive,regular,pure_instructions
.globl _positive
_positive:
ret
.section __TEXT,__negative,regular,pure_instructions
.globl _negative
_negative:
ret
.section __DATA,__delta
.globl _delta
.no_dead_strip _delta
_delta:
.quad _positive-_negative
"#,
    )];
    let exe = link(&objects, options(true)).unwrap();
    let n = names(&exe);
    for name in ["_helper", "_positive", "_negative"] {
        assert!(n.contains(name));
    }
}

#[test]
fn retain_in_unselected_archive_is_not_an_extraction_request() {
    let main = assemble(
        ".globl _main\n_main:\nadrp x0,section$start$__DATA$registry@PAGE\nadd x0,x0,section$start$__DATA$registry@PAGEOFF\nret\n",
    );
    let unused = assemble(
        ".section __DATA,registry\n.globl _registered\n.no_dead_strip _registered\n_registered:\n.quad _missing\n",
    );
    let archive = keel_link::archive::build(
        &[Input {
            name: "unselected.o",
            bytes: &unused,
        }],
        keel_link::archive::Target::Aarch64AppleDarwin,
    )
    .unwrap();
    let exe = link(&[main, archive], options(true)).unwrap();
    assert!(!names(&exe).contains("_registered"));
}

#[test]
fn section_no_dead_strip_attribute_is_a_root() {
    let mut objects = [assemble(
        ".globl _main\n_main:\nret\n.section __DATA,__registration\n.globl _kept\n_kept:\n.quad _target\n.section __DATA,__target\n.globl _target\n_target:\n.quad 19\n",
    )];
    // The assembler intentionally exposes only a small attribute surface. Set
    // S_ATTR_NO_DEAD_STRIP directly to test the independent consumer contract.
    let b = &mut objects[0];
    let word = |b: &[u8], at| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..word(b, 16) {
        if word(b, at) == 0x19 {
            for i in 0..word(b, at + 64) {
                let s = at + 72 + i * 80;
                if &b[s..s + 14] == b"__registration" {
                    let flags = word(b, s + 64) as u32 | 0x10000000;
                    b[s + 64..s + 68].copy_from_slice(&flags.to_le_bytes());
                }
            }
        }
        at += word(b, at + 4);
    }
    let n = names(&link(&objects, options(true)).unwrap());
    assert!(n.contains("_kept") && n.contains("_target"));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn required_export_preserves_runtime_dso_consumer() {
    use std::{fs, path::PathBuf, process::Command};
    fn run(cmd: &mut Command) -> std::process::Output {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    let dir = std::env::temp_dir().join(format!("keel-gc-export-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("plugin.c"),
        "extern int runtime_api(void); int consume(void) { return runtime_api(); }\n",
    )
    .unwrap();
    run(Command::new("clang")
        .arg("-dynamiclib")
        .arg(dir.join("plugin.c"))
        .args(["-undefined", "dynamic_lookup", "-o"])
        .arg(dir.join("plugin.dylib")));
    fs::write(dir.join("main.c"), format!("#include <dlfcn.h>\nint main(void) {{ void *p=dlopen(\"{}\",RTLD_NOW); if (!p) return 3; int (*f)(void)=dlsym(p,\"consume\"); return !f || f()!=42; }}\n", dir.join("plugin.dylib").display())).unwrap();
    run(Command::new("clang")
        .arg("-c")
        .arg(dir.join("main.c"))
        .arg("-o")
        .arg(dir.join("main.o")));
    let objects = [
        fs::read(dir.join("main.o")).unwrap(),
        assemble(".globl _runtime_api\n_runtime_api:\nmov w0,#42\nret\n"),
    ];
    fs::write(dir.join("api.o"), &objects[1]).unwrap();
    let sdk = run(Command::new("xcrun").arg("--show-sdk-path"));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    for required in [false, true] {
        let mut opts = options(true);
        opts.dylibs = vec![Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap()];
        if required {
            opts.required_exports.push("_runtime_api".into());
        }
        let image = link(&objects, opts).unwrap();
        assert_eq!(names(&image).contains("_runtime_api"), required);
        let path = dir.join("native");
        keel_link::write_signed(&image, &path).unwrap();
        assert_eq!(
            Command::new(path).status().unwrap().code(),
            Some(if required { 0 } else { 3 })
        );
    }
    let apple = dir.join("apple");
    run(Command::new("clang")
        .arg(dir.join("main.o"))
        .arg(dir.join("api.o"))
        .args(["-Wl,-dead_strip", "-Wl,-u,_runtime_api", "-o"])
        .arg(&apple));
    run(&mut Command::new(apple));
    fs::remove_dir_all(dir).unwrap();
}

fn relocation_word(bytes: &[u8], kind: u32) -> usize {
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..word(16) {
        if word(at) == 0x19 {
            for i in 0..word(at + 64) {
                let section = at + 72 + i * 80;
                for j in 0..word(section + 60) {
                    let rel = word(section + 56) + j * 8;
                    if word(rel + 4) >> 28 == kind as usize {
                        return rel + 4;
                    }
                }
            }
        }
        at += word(at + 4);
    }
    panic!("missing relocation kind {kind}");
}

#[test]
fn malformed_addend_and_subtractor_pairs_are_diagnosed_before_traversal() {
    let original = assemble(
        ".globl _main\n_main:\nb _target+4\n.section __TEXT,__target,regular,pure_instructions\n.globl _target\n_target:\nnop\nret\n",
    );
    let mut bad = original.clone();
    let at = relocation_word(&bad, 10);
    bad[at + 3] |= 8; // ADDEND is never external
    assert!(
        link(&[bad], options(true))
            .err()
            .unwrap()
            .contains("invalid ARM64_RELOC_ADDEND")
    );
    let mut bad = original;
    let at = relocation_word(&bad, 2);
    bad[at + 3] = (bad[at + 3] & 15) | 0x50; // ADDEND cannot prefix GOT_LOAD_PAGE21
    assert!(
        link(&[bad], options(true))
            .err()
            .unwrap()
            .contains("ADDEND must precede")
    );
    let mut bad = assemble(
        ".globl _main\n_main:\nret\n.section __DATA,__difference\n.no_dead_strip _diff\n_diff:\n.quad _main-_negative\n.section __TEXT,__negative,regular,pure_instructions\n.globl _negative\n_negative:\nret\n",
    );
    let at = relocation_word(&bad, 1);
    bad[at + 3] |= 1; // SUBTRACTOR is never PC-relative
    assert!(
        link(&[bad], options(true))
            .err()
            .unwrap()
            .contains("invalid SUBTRACTOR/UNSIGNED pair")
    );
}

#[test]
fn dead_cie_personality_and_lsda_imports_do_not_become_roots() {
    let bytes = assemble(
        r#"
.subsections_via_symbols
.section __TEXT,__dead,regular,pure_instructions
.globl _dead
_dead:
.cfi_startproc
.cfi_personality 155,_missing_personality
.cfi_lsda 16,_lsda
.cfi_remember_state
.cfi_restore_state
ret
.cfi_endproc
.section __TEXT,__lsda
_lsda:
.quad _missing_typeinfo
.section __TEXT,__entry,regular,pure_instructions
.globl _main
_main:
.cfi_startproc
.cfi_remember_state
.cfi_restore_state
ret
.cfi_endproc
"#,
    );
    let exe = link(&[bytes], options(true)).unwrap();
    assert!(!names(&exe).iter().any(|n| n.contains("missing")));
}

#[test]
fn required_common_export_uses_merged_visibility_in_both_input_orders() {
    let main = assemble(".globl _main\n_main:\nmov w0,#0\nret\n");
    let private = assemble(".private_extern _api\n.comm _api,8,3\n");
    let public = assemble(".comm _api,32,4\n");
    for reverse in [false, true] {
        let mut objects = vec![main.clone(), private.clone(), public.clone()];
        if reverse {
            objects.swap(1, 2);
        }
        let mut opts = options(true);
        opts.required_exports.push("_api".into());
        let exe = link(&objects, opts).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let api = file.symbols().find(|s| s.name() == Ok("_api")).unwrap();
        assert_eq!(api.scope(), object::SymbolScope::Dynamic);
        assert_eq!(file.section_by_name("__common").unwrap().size(), 32);
        assert_eq!(api.address() % 16, 0);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            use std::{fs, process::Command};
            let dir = std::env::temp_dir()
                .join(format!("keel-gc-common-{}-{reverse}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let mut apple = Command::new("clang");
            for (i, bytes) in objects.iter().enumerate() {
                let path = dir.join(format!("{i}.o"));
                fs::write(&path, bytes).unwrap();
                apple.arg(path);
            }
            let apple_path = dir.join("apple");
            let result = apple
                .args(["-Wl,-dead_strip", "-Wl,-u,_api", "-o"])
                .arg(&apple_path)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let apple_bytes = fs::read(&apple_path).unwrap();
            let apple_file = object::File::parse(apple_bytes.as_slice()).unwrap();
            let apple_api = apple_file
                .symbols()
                .find(|s| s.name() == Ok("_api"))
                .unwrap();
            assert_eq!(apple_api.scope(), api.scope());
            assert_eq!(apple_api.address() % 16, 0);
            assert_eq!(apple_file.section_by_name("__common").unwrap().size(), 32);
            let native_path = dir.join("native");
            keel_link::write_signed(&exe, &native_path).unwrap();
            for path in [&native_path, &apple_path] {
                assert!(Command::new(path).status().unwrap().success());
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
    let mut opts = options(true);
    opts.required_exports.push("_api".into());
    assert!(
        link(&[main, private], opts)
            .err()
            .unwrap()
            .contains("private extern")
    );
}

#[test]
fn straddling_unwind_relocation_is_rejected_before_pruning() {
    let mut bytes = assemble(
        ".globl _main\n_main:\nmov w0,#0\nret\n.section __TEXT,__dead,regular,pure_instructions\n.globl _dead\n_dead:\nret\n.section __LD,__compact_unwind\n.quad _dead\n.long 4\n.long 0x02001000\n.quad 0\n.quad _dead\n.quad _main\n.long 8\n.long 0x02001000\n.quad 0\n.quad 0\n",
    );
    let word = |b: &[u8], at| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..word(&bytes, 16) {
        if word(&bytes, at) == 0x19 {
            for i in 0..word(&bytes, at + 64) {
                let section = at + 72 + i * 80;
                if &bytes[section..section + 16] != b"__compact_unwind" {
                    continue;
                }
                for j in 0..word(&bytes, section + 60) {
                    let rel = word(&bytes, section + 56) + j * 8;
                    if word(&bytes, rel) == 24 {
                        // The dead record's LSDA fixup now overwrites four
                        // bytes in the live record's owner field as well.
                        bytes[rel..rel + 4].copy_from_slice(&28u32.to_le_bytes());
                    }
                }
            }
        }
        at += word(&bytes, at + 4);
    }
    assert!(
        link(&[bytes.clone()], options(false))
            .err()
            .unwrap()
            .contains("overlapping relocation")
    );
    assert!(
        link(&[bytes], options(true))
            .err()
            .unwrap()
            .contains("relocation crosses an unwind record boundary")
    );
}

// Move a relocation-free input section and all its symbols together, preserving
// section-relative identities while exercising 64-bit input address arithmetic.
fn rebase_text(bytes: &mut [u8], address: u64) {
    let word = |b: &[u8], at| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    let mut ordinal = 0;
    let mut text = None;
    let mut symtab = None;
    for _ in 0..word(bytes, 16) {
        match word(bytes, at) {
            0x19 => {
                for i in 0..word(bytes, at + 64) {
                    ordinal += 1;
                    let section = at + 72 + i * 80;
                    if &bytes[section..section + 7] == b"__text\0" {
                        let old = u64::from_le_bytes(
                            bytes[section + 32..section + 40].try_into().unwrap(),
                        );
                        bytes[section + 32..section + 40].copy_from_slice(&address.to_le_bytes());
                        text = Some((ordinal, old));
                    }
                }
            }
            2 => symtab = Some((word(bytes, at + 8), word(bytes, at + 12))),
            _ => {}
        }
        at += word(bytes, at + 4);
    }
    let (ordinal, old) = text.unwrap();
    let (symbols, count) = symtab.unwrap();
    for i in 0..count {
        let symbol = symbols + i * 16;
        if bytes[symbol + 5] as usize == ordinal {
            let value = u64::from_le_bytes(bytes[symbol + 8..symbol + 16].try_into().unwrap());
            bytes[symbol + 8..symbol + 16]
                .copy_from_slice(&(address + (value - old)).to_le_bytes());
        }
    }
}

#[test]
fn high_input_addresses_preserve_section_relative_root_identity() {
    let original = assemble(".globl _main\n_main:\nmov w0,#0\nret\n");
    let mut high = original.clone();
    rebase_text(&mut high, u64::MAX - 0x1000);
    for gc in [false, true] {
        let ordinary = link(std::slice::from_ref(&original), options(gc)).unwrap();
        let rebased = link(&[high.clone()], options(gc)).unwrap();
        assert_eq!(ordinary.bytes, rebased.bytes);
    }
}

#[test]
fn overflowing_input_section_range_is_diagnosed_before_collection() {
    let mut bytes = assemble(".globl _main\n_main:\nret\n");
    rebase_text(&mut bytes, u64::MAX - 1);
    let dir = std::env::temp_dir().join(format!("keel-gc-overflow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let destination = dir.join("program");
    std::fs::write(&destination, b"original executable").unwrap();
    for gc in [false, true] {
        let error = link(&[bytes.clone()], options(gc)).err().unwrap();
        assert!(error.contains("section address overflow"), "{error}");
        let error = keel_link::link_signed(
            &[Input {
                name: "overflow.o",
                bytes: &bytes,
            }],
            options(gc),
            &destination,
        )
        .err()
        .unwrap();
        assert!(error.contains("section address overflow"), "{error}");
        assert_eq!(std::fs::read(&destination).unwrap(), b"original executable");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

fn symbol(exe: &Executable, name: &str) -> u64 {
    object::File::parse(exe.bytes.as_slice())
        .unwrap()
        .symbols()
        .find(|s| s.name() == Ok(name))
        .unwrap_or_else(|| panic!("{name} missing"))
        .address()
}
/// `len` bytes of the image at virtual address `address`.
fn bytes_at(exe: &Executable, address: u64, len: usize) -> Vec<u8> {
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let section = file
        .sections()
        .find(|s| (s.address()..s.address() + s.size()).contains(&address))
        .unwrap();
    let at = (address - section.address()) as usize;
    section.data().unwrap()[at..at + len].to_vec()
}
fn section_size(exe: &Executable, name: &str) -> u64 {
    object::File::parse(exe.bytes.as_slice())
        .unwrap()
        .section_by_name(name)
        .unwrap()
        .size()
}

/// With subsections, each function is an atom: a dead one goes, with its
/// references, even beside live code in the same section.
#[test]
fn functions_sharing_a_section_are_stripped_one_by_one() {
    let source = ".globl _main\n_main:\nbl _used\nmov w0,#0\nret\n.globl _used\n_used:\nret\n.globl _dead\n_dead:\nbl _missing\nbl _puts\nret\n";
    let split = [assemble(&format!(".subsections_via_symbols\n{source}"))];
    let exe = link(&split, options(true)).unwrap();
    let n = names(&exe);
    assert!(n.contains("_used"));
    for dead in ["_dead", "_puts", "_missing"] {
        assert!(!n.contains(dead), "{dead}");
    }
    assert_eq!(section_size(&exe, "__text"), 16);
    assert_eq!(section_size(&exe, "__stubs"), 0);
    assert!(
        link(&split, options(false))
            .err()
            .unwrap()
            .contains("_missing")
    );
    // Without the flag, an assembler may have resolved references between
    // the functions, so the section stays whole.
    let whole = [assemble(source)];
    assert!(
        link(&whole, options(true))
            .err()
            .unwrap()
            .contains("_missing")
    );
}

/// Differences between live atoms measure the output distance, and a dead
/// atom's difference with an undefined symbol is never evaluated.
#[test]
fn differences_span_removed_atoms_and_dead_ones_are_skipped() {
    let exe = link(
        &[assemble(
            ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,_table@PAGE\nadd x0,x0,_table@PAGEOFF\nbl _b\nret\n_a:\nret\n_mid:\nnop\nnop\nret\n.globl _b\n_b:\nbl _a\nret\n.data\n.globl _table\n_table:\n.long _b - _a\n.quad _b - _a\n_unused:\n.quad _undefined - _a\n",
        )],
        options(true),
    )
    .unwrap();
    let table = bytes_at(&exe, symbol(&exe, "_table"), 12);
    assert_eq!(u32::from_le_bytes(table[..4].try_into().unwrap()), 4);
    assert_eq!(u64::from_le_bytes(table[4..].try_into().unwrap()), 4);
    // _main, _a and _b; _mid is gone.
    assert_eq!(section_size(&exe, "__text"), 28);
}

/// An extern relocation targets its symbol's atom whatever the addend, even
/// when the addend points past that atom into a dead one.
#[test]
fn an_addend_past_its_atom_stays_relative_to_the_symbol() {
    let exe = link(
        &[assemble(
            ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,_ptrs@PAGE\nldr x0,[x0,_ptrs@PAGEOFF]\nret\n.data\n_before:\n.quad 1\n.globl _arr\n_arr:\n.quad 1, 2\n_next:\n.quad 3\n.globl _ptrs\n_ptrs:\n.quad _arr + 16\n",
        )],
        options(true),
    )
    .unwrap();
    let pointer = bytes_at(&exe, symbol(&exe, "_ptrs"), 8);
    assert_eq!(
        u64::from_le_bytes(pointer.try_into().unwrap()),
        symbol(&exe, "_arr") + 16
    );
    assert_eq!(section_size(&exe, "__data"), 24);
}

/// An atom keeps its offset modulo the section alignment, so a point inside
/// it that was aligned stays aligned after an earlier atom goes.
#[test]
fn atoms_keep_their_alignment_phase() {
    let exe = link(
        &[assemble(
            ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,Linner@PAGE\nadd x0,x0,Linner@PAGEOFF\nret\n.data\n.p2align 4\n_dead:\n.long 1\n.globl _host\n_host:\n.long 2, 3, 4\n.p2align 4\nLinner:\n.quad 5\n",
        )],
        options(true),
    )
    .unwrap();
    let host = symbol(&exe, "_host");
    assert_eq!(host % 16, 4);
    assert_eq!(bytes_at(&exe, host + 12, 8), 5u64.to_le_bytes());
}

/// Aliases share an atom. A symbol at a section's end is a zero-size atom
/// that follows the last live atom.
#[test]
fn aliases_share_an_atom_and_the_section_end_follows_live_atoms() {
    let exe = link(
        &[assemble(
            ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,_x@PAGE\nadrp x1,_keep@PAGE\nadrp x2,_end@PAGE\nret\n.data\n.globl _x\n.globl _y\n_x:\n_y:\n.quad 7\n.globl _z\n_z:\n.quad 8\n.globl _keep\n_keep:\n.quad 9\n.globl _end\n_end:\n",
        )],
        options(true),
    )
    .unwrap();
    let n = names(&exe);
    assert!(n.contains("_x") && n.contains("_y"));
    assert!(!n.contains("_z"));
    assert_eq!(symbol(&exe, "_x"), symbol(&exe, "_y"));
    assert_eq!(symbol(&exe, "_end"), symbol(&exe, "_keep") + 8);
}

/// No-dead-strip symbols keep their own atoms, so a registration section
/// keeps only its marked entries.
#[test]
fn no_dead_strip_symbols_keep_only_their_atoms() {
    let exe = link(
        &[assemble(
            ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,section$start$__DATA$__reg@PAGE\nadrp x1,section$end$__DATA$__reg@PAGE\nret\n.section __DATA,__reg\n.no_dead_strip _r1\n_r1:\n.quad 1\n_r2:\n.quad 2\n.no_dead_strip _r3\n_r3:\n.quad 3\n",
        )],
        options(true),
    )
    .unwrap();
    assert_eq!(section_size(&exe, "__reg"), 16);
}

/// Clang objects linked by keel and by Apple's linker with dead stripping
/// keep the same functions, data and strings, and both images run.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_atoms_match_apple_dead_stripping() {
    use std::{fs, path::PathBuf, process::Command};
    fn run(command: &mut Command) -> std::process::Output {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    let dir = std::env::temp_dir().join(format!("keel-gc-atoms-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("program.c"),
        r#"#include <stdio.h>
int dead_table[64] = {1};
int live_table[4] = {1, 2, 3, 4};
const char *dead_message(void) { return "only dead code names this string"; }
__attribute__((noinline)) int dead_call(int x) {
    printf("%s %d\n", dead_message(), dead_table[x]);
    return x;
}
__attribute__((noinline)) int used(int x) { return x * live_table[2]; }
int alt_inner(void);
extern void *whole_ref;
int main(int argc, char **argv) {
    (void)argv;
    return used(argc) == 3 && alt_inner() == 0 && whole_ref ? 0 : 1;
}
"#,
    )
    .unwrap();
    // An alternate entry shares its host's atom. Without the subsections
    // flag, a whole section stays when any of it lives.
    fs::write(
        dir.join("alt.s"),
        ".subsections_via_symbols\n.text\n.globl _alt_host\n_alt_host:\nnop\n.globl _alt_inner\n.alt_entry _alt_inner\n_alt_inner:\nmov w0, #0\nret\n.globl _alt_dead\n_alt_dead:\nret\n",
    )
    .unwrap();
    fs::write(
        dir.join("whole.s"),
        ".text\n.globl _whole_live\n_whole_live:\nret\n.globl _whole_dead\n_whole_dead:\nret\n.data\n.globl _whole_ref\n_whole_ref:\n.quad _whole_live\n",
    )
    .unwrap();
    for (source, object) in [("alt.s", "alt.o"), ("whole.s", "whole.o")] {
        run(Command::new("clang")
            .arg("-c")
            .arg(dir.join(source))
            .arg("-o")
            .arg(dir.join(object)));
    }
    let sdk = run(Command::new("xcrun").arg("--show-sdk-path"));
    let sdk = PathBuf::from(String::from_utf8(sdk.stdout).unwrap().trim());
    let defined = |path: &PathBuf| -> BTreeSet<String> {
        let nm = run(Command::new("nm").args(["-g", "-U", "-j"]).arg(path));
        String::from_utf8(nm.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    };
    for flags in [&["-O0"][..], &["-O2"], &["-O1", "-g"]] {
        run(Command::new("clang")
            .args(flags)
            .arg("-c")
            .arg(dir.join("program.c"))
            .arg("-o")
            .arg(dir.join("program.o")));
        let objects = ["program.o", "alt.o", "whole.o"].map(|o| dir.join(o));
        let inputs = objects.clone().map(|o| fs::read(o).unwrap());
        let mut opts = options(true);
        opts.dylibs = vec![Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk).unwrap()];
        let exe = link(&inputs, opts).unwrap();
        let native = dir.join("native");
        let apple = dir.join("apple");
        keel_link::write_signed(&exe, &native).unwrap();
        run(Command::new("clang")
            .args(&objects)
            .args(["-Wl,-dead_strip", "-o"])
            .arg(&apple));
        let names = [
            "_main",
            "_used",
            "_live_table",
            "_dead_call",
            "_dead_message",
            "_dead_table",
            "_alt_host",
            "_alt_inner",
            "_alt_dead",
            "_whole_live",
            "_whole_dead",
        ];
        let (ours, theirs) = (defined(&native), defined(&apple));
        for name in names {
            assert_eq!(
                ours.contains(name),
                theirs.contains(name),
                "{name} with {flags:?}"
            );
        }
        for dead in ["_dead_call", "_dead_message", "_dead_table", "_alt_dead"] {
            assert!(!theirs.contains(dead), "Apple keeps {dead} with {flags:?}");
        }
        assert!(ours.contains("_whole_dead"));
        let text = fs::read(&native).unwrap();
        let dead_string = b"only dead code names this string";
        assert!(!text.windows(dead_string.len()).any(|w| w == dead_string));
        for path in [&native, &apple] {
            run(Command::new("codesign")
                .args(["--verify", "--strict"])
                .arg(path));
            run(&mut Command::new(path));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
