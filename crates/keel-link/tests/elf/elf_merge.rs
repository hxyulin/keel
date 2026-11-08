use super::{link_with, object as assemble, value, word};
use keel_link::elf;
use object::{Object, ObjectSection, ObjectSymbol};

fn link(objects: &[Vec<u8>], enabled: bool, gc: bool) -> elf::Executable {
    link_with(
        "merge.o",
        objects,
        elf::Options {
            merge_constants: enabled,
            gc_sections: gc,
            ..Default::default()
        },
    )
    .unwrap()
}
fn rodata(bytes: &[u8]) -> u64 {
    object::File::parse(bytes)
        .unwrap()
        .sections()
        .filter(|s| s.name().unwrap().starts_with(".rodata"))
        .map(|s| s.size())
        .sum()
}

#[test]
fn records_strings_and_absolute_section_addends_share_actual_entries() {
    let a = assemble(
        ".text\n.globl _start\n_start: ret\n.section .rodata.cst8,\"aM\",@progbits,8\n.p2align 3\n.globl first\n.type first,@object\nfirst: .quad 42\n.size first,8\n.quad 99\n.quad 42\n.section .rodata.str,\"aMS\",@progbits,1\n.globl hello\nhello: .asciz \"hello\"\n.asciz \"other\"\n.asciz \"hello\"\n.data\n.globl pointers\npointers: .quad .rodata.cst8+16\n.quad first+8\n.quad .rodata.str+14\n.quad hello+2\n",
    );
    let b = assemble(
        ".section .rodata.other,\"aM\",@progbits,8\n.p2align 3\n.globl second\nsecond: .quad 42\n.quad 123\n.section .rodata.more,\"aMS\",@progbits,1\n.globl hello2\nhello2: .asciz \"hello\"\n",
    );
    let objects = [a, b];
    let baseline = link(&objects, false, false);
    let candidate = link(&objects, true, false);
    assert_eq!(candidate.bytes, link(&objects, true, false).bytes);
    let file = object::File::parse(candidate.bytes.as_slice()).unwrap();
    assert_eq!(value(&file, "first"), value(&file, "second"));
    assert_eq!(value(&file, "first") % 8, 0);
    assert_eq!(value(&file, "hello"), value(&file, "hello2"));
    let ptr = value(&file, "pointers");
    assert_eq!(word(&candidate.bytes, ptr), value(&file, "first"));
    assert_eq!(word(&candidate.bytes, ptr + 8), value(&file, "first") + 8);
    assert_eq!(word(&candidate.bytes, ptr + 16), value(&file, "hello") + 2);
    assert_eq!(word(&candidate.bytes, ptr + 24), value(&file, "hello") + 2);
    assert_eq!(rodata(&baseline.bytes), 64);
    assert_eq!(rodata(&candidate.bytes), 36);
    eprintln!(
        "merge fixture: rodata {} -> {}; image {} -> {}",
        rodata(&baseline.bytes),
        rodata(&candidate.bytes),
        baseline.bytes.len(),
        candidate.bytes.len()
    );
}

#[test]
fn wide_strings_use_whole_character_terminators_and_keep_interior_symbols() {
    let input = assemble(
        ".text\n.globl _start\n_start: ret\n.section .rodata.wide,\"aMS\",@progbits,2\n.p2align 1\n.globl wide\nwide: .short 256\n.local interior\ninterior: .short 257,0\n.globl duplicate\nduplicate: .short 256,257,0\n.globl end\nend:\n.data\n.globl pointers\npointers: .quad wide+6\n.quad interior\n.quad end\n",
    );
    let image = link(&[input], true, false);
    let file = object::File::parse(image.bytes.as_slice()).unwrap();
    assert_eq!(rodata(&image.bytes), 6);
    assert_eq!(value(&file, "duplicate"), value(&file, "wide"));
    assert_eq!(value(&file, "interior"), value(&file, "wide") + 2);
    let p = value(&file, "pointers");
    assert_eq!(word(&image.bytes, p), value(&file, "wide") + 6);
    assert_eq!(word(&image.bytes, p + 8), value(&file, "interior"));
    assert_eq!(word(&image.bytes, p + 16), value(&file, "wide") + 6);
}

#[test]
fn unsupported_forms_preserve_the_entire_contribution() {
    for body in [
        // A sized array crosses independently mergeable records.
        ".globl array\n.type array,@object\narray: .quad 7,7\n.size array,16\n",
        // Relocations in the records require relocation-aware equality.
        ".quad _start,_start\n",
    ] {
        let input = assemble(&format!(
            ".text\n.globl _start\n_start: ret\n.section .rodata.negative,\"aM\",@progbits,8\n{body}"
        ));
        assert_eq!(
            link(std::slice::from_ref(&input), false, false).bytes,
            link(&[input], true, false).bytes
        );
    }
    let input = assemble(
        ".text\n.globl _start\n_start: lea rax,[rip+constant]\nret\n.section .rodata.pc,\"aM\",@progbits,8\nconstant: .quad 7,7\n",
    );
    assert_eq!(
        link(std::slice::from_ref(&input), false, false).bytes,
        link(&[input], true, false).bytes
    );
    let input = assemble(
        ".text\n.globl _start\n_start: ret\n.section .rodata.unterminated,\"aMS\",@progbits,2\n.short 1,1\n",
    );
    assert_eq!(
        link(std::slice::from_ref(&input), false, false).bytes,
        link(&[input], true, false).bytes
    );
}

#[test]
fn gc_and_comdat_keep_selected_identities_before_pooling() {
    let start = assemble(
        ".text\n.globl _start\n_start: ret\n.section .data,\"awR\",@progbits\n.quad selected\n.section .rodata.dead,\"aM\",@progbits,8\n.quad 99,99\n",
    );
    let choice = |n| {
        assemble(&format!(
            ".section .rodata.choice,\"aMG\",@progbits,8,key,comdat\n.globl selected\nselected: .quad {n},{n}\n"
        ))
    };
    let image = link(&[start, choice(42), choice(99)], true, true);
    let file = object::File::parse(image.bytes.as_slice()).unwrap();
    assert_eq!(rodata(&image.bytes), 8);
    assert_eq!(word(&image.bytes, value(&file, "selected")), 42);
    assert_eq!(
        file.symbols()
            .filter(|s| s.name().unwrap() == "selected")
            .count(),
        1
    );
}

#[test]
fn repeated_constant_tables_reduce_the_file_image() {
    let source = format!(
        ".text\n.globl _start\n_start: ret\n.section .rodata.table,\"aM\",@progbits,8\n.p2align 3\n{}",
        ".quad 0x3ff0000000000000\n.quad 0x4000000000000000\n".repeat(2048)
    );
    let input = assemble(&source);
    let old = link(std::slice::from_ref(&input), false, false);
    let new = link(&[input], true, false);
    assert_eq!(rodata(&old.bytes), 32768);
    assert_eq!(rodata(&new.bytes), 16);
    assert!(new.bytes.len() < old.bytes.len());
    eprintln!(
        "constant table: rodata {} -> {}; image {} -> {}",
        rodata(&old.bytes),
        rodata(&new.bytes),
        old.bytes.len(),
        new.bytes.len()
    );
}

#[test]
fn unsupported_layouts_are_diagnosed() {
    let input = assemble(".text\n.globl _start\n_start: ret\n");
    for options in [
        elf::Options {
            merge_constants: true,
            machine: elf::Machine::Aarch64,
            ..Default::default()
        },
        elf::Options {
            merge_constants: true,
            script: Some("SECTIONS { .text : { *(.text) } }".into()),
            ..Default::default()
        },
    ] {
        assert!(
            link_with("input.o", std::slice::from_ref(&input), options)
                .unwrap_err()
                .to_string()
                .contains("constant merging requires")
        );
    }
}

#[test]
fn clang_objects_match_an_independent_elf_linker_and_export_remapped_addresses() {
    use std::{path::PathBuf, process::Command};
    let linker = std::env::var_os("KEEL_TEST_ELF_LD")
        .map(PathBuf::from)
        .or_else(|| {
            let out = Command::new("rustc")
                .args(["--print", "sysroot"])
                .output()
                .ok()?;
            let root = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
            let version = Command::new("rustc").arg("-vV").output().ok()?;
            let version = String::from_utf8(version.stdout).ok()?;
            let host = version.lines().find_map(|s| s.strip_prefix("host: "))?;
            let path = root.join("lib/rustlib").join(host).join("bin/rust-lld");
            path.is_file().then_some(path)
        });
    let Some(linker) = linker else {
        keel_test_support::skip!("no ELF linker; set KEEL_TEST_ELF_LD");
    };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("merge-oracle");
    std::fs::create_dir_all(&dir).unwrap();
    let source = ".text\n.globl _start\n_start: ret\n.section .rodata.cst8,\"aM\",@progbits,8\n.p2align 3\n.globl first,second\nfirst: .quad 42\n.quad 99\nsecond: .quad 42\n.section .rodata.str,\"aMS\",@progbits,1\n.globl text,duplicate\ntext: .asciz \"hello\"\n.asciz \"other\"\nduplicate: .asciz \"hello\"\n.data\n.globl pointers\npointers: .quad .rodata.cst8+16\n.quad .rodata.str+14\n.quad first+8\n";
    std::fs::write(dir.join("input.s"), source).unwrap();
    std::fs::write(dir.join("helper.s"), ".text\n.globl helper\nhelper: ret\n").unwrap();
    let Ok(output) = Command::new("clang")
        .current_dir(&dir)
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "input.s",
            "helper.s",
        ])
        .output()
    else {
        keel_test_support::skip!("clang unavailable");
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut ld = Command::new(&linker);
    if linker.file_name().unwrap() == "rust-lld" {
        ld.args(["-flavor", "gnu"]);
    }
    let output = ld
        .current_dir(&dir)
        .args(["-O1", "input.o", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle = std::fs::read(dir.join("oracle")).unwrap();
    let input = std::fs::read(dir.join("input.o")).unwrap();
    let candidate = link(std::slice::from_ref(&input), true, false);
    for bytes in [&oracle, &candidate.bytes] {
        let file = object::File::parse(bytes.as_slice()).unwrap();
        assert_eq!(value(&file, "first"), value(&file, "second"));
        assert_eq!(value(&file, "text"), value(&file, "duplicate"));
        let pointers = file.section_by_name(".data").unwrap().data().unwrap();
        let pointer = |i| u64::from_le_bytes(pointers[i..i + 8].try_into().unwrap());
        assert_eq!(pointer(0), value(&file, "first"));
        assert_eq!(pointer(8), value(&file, "text") + 2);
        assert_eq!(pointer(16), value(&file, "first") + 8);
    }
    let mut ld = Command::new(&linker);
    if linker.file_name().unwrap() == "rust-lld" {
        ld.args(["-flavor", "gnu"]);
    }
    let output = ld
        .current_dir(&dir)
        .args(["-shared", "helper.o", "-o", "helper.so"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let library = std::fs::read(dir.join("helper.so")).unwrap();
    let candidate = link(&[input, library], true, true);
    let file = object::File::parse(candidate.bytes.as_slice()).unwrap();
    for name in ["first", "second", "text", "duplicate"] {
        let exported = file
            .dynamic_symbols()
            .find(|s| s.name().unwrap() == name)
            .unwrap();
        assert_eq!(exported.address(), value(&file, name), "{name}");
    }
    assert_eq!(value(&file, "first"), value(&file, "second"));
    assert_eq!(value(&file, "text"), value(&file, "duplicate"));
}

#[test]
fn entry_budget_falls_back_without_partial_pooling() {
    let input = assemble(
        ".text\n.globl _start\n_start: ret\n.section .rodata.large,\"aM\",@progbits,1\n.zero 131073\n",
    );
    assert_eq!(
        link(std::slice::from_ref(&input), false, false).bytes,
        link(&[input], true, false).bytes
    );
}
