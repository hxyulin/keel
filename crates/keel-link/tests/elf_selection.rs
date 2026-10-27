use keel_link::{
    Input,
    archive::{self, Target},
    elf,
    elf_selection::{Request, Selection},
};
use object::{Object, ObjectSymbol};

fn object(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}

fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
    archive::build(
        &members
            .iter()
            .map(|(name, bytes)| Input { name, bytes })
            .collect::<Vec<_>>(),
        Target::X86_64LinuxGnu,
    )
    .unwrap()
}

fn file<'a>(name: &'a str, bytes: &'a [u8]) -> Request<'a> {
    Request::File { name, bytes }
}

fn selected_names(requests: &[Request<'_>]) -> Vec<String> {
    Selection::prepare(requests, "_start")
        .unwrap()
        .inputs()
        .iter()
        .map(|i| i.name.into())
        .collect()
}

fn link(requests: &[Request<'_>]) -> keel_link::Result<keel_link::elf::Executable> {
    let selected = Selection::prepare(requests, "_start")?;
    elf::link(&selected.inputs(), Default::default())
}

#[test]
fn circular_groups_rescan_only_their_archives_and_support_nesting() {
    let start = object(".text\n.globl _start\n_start:\ncall first\nret\n");
    let first = object(".text\n.globl first\nfirst:\ncall second\nret\n");
    let second = object(".text\n.globl second\nsecond:\ncall last\nret\n");
    let last = object(".text\n.globl last\nlast:\nret\n");
    let a = archive(&[("same.o", &last), ("same.o", &first)]);
    let b = archive(&[("second.o", &second)]);
    let plain = [file("start.o", &start), file("a.a", &a), file("b.a", &b)];
    assert!(
        link(&plain)
            .unwrap_err()
            .contains("undefined symbol `last`")
    );
    for nested in [false, true] {
        let mut requests = vec![
            file("start.o", &start),
            Request::StartGroup,
            file("a.a", &a),
        ];
        if nested {
            requests.push(Request::StartGroup);
        }
        requests.push(file("b.a", &b));
        if nested {
            requests.push(Request::EndGroup);
        }
        requests.push(Request::EndGroup);
        assert_eq!(
            selected_names(&requests),
            ["start.o", "a.a(same.o)", "b.a(second.o)", "a.a(same.o)"]
        );
        link(&requests).unwrap();
    }
    // A group does not reopen an archive preceding it.
    assert!(
        link(&[
            file("a.a", &a),
            file("start.o", &start),
            Request::StartGroup,
            file("b.a", &b),
            Request::EndGroup
        ])
        .unwrap_err()
        .contains("undefined symbol `first`")
    );
    // Nor does a demand appearing after the group reopen it.
    assert!(
        link(&[
            Request::StartGroup,
            file("a.a", &a),
            file("b.a", &b),
            Request::EndGroup,
            file("start.o", &start)
        ])
        .unwrap_err()
        .contains("undefined symbol `first`")
    );
}

#[test]
fn whole_archive_toggles_include_unreferenced_members_in_order() {
    let start = object(".text\n.globl _start\n_start:\nret\n");
    let unused = object(".text\n.globl unused\nunused:\nret\n");
    let other = object(".text\n.globl other\nother:\nret\n");
    let a = archive(&[("same.o", &unused), ("same.o", &other)]);
    let b = archive(&[("conflict.o", &start)]);
    let requests = [
        Request::WholeArchive,
        file("start.o", &start),
        Request::StartGroup,
        file("a.a", &a),
        Request::NoWholeArchive,
        file("b.a", &b),
        Request::EndGroup,
    ];
    assert_eq!(
        selected_names(&requests),
        ["start.o", "a.a(same.o)", "a.a(same.o)"]
    );
    let image = link(&requests).unwrap();
    let elf = object::File::parse(image.bytes.as_slice()).unwrap();
    for name in ["unused", "other"] {
        assert!(elf.symbols().any(|s| s.name().unwrap() == name), "{name}");
    }
    assert_eq!(
        selected_names(&[file("start.o", &start), file("a.a", &a)]),
        ["start.o"]
    );
}

#[test]
fn whole_archive_reports_strong_duplicates_and_repeated_occurrences() {
    let start = object(".text\n.globl _start\n_start:\nret\n");
    let duplicate = archive(&[("same.o", &start), ("same.o", &start)]);
    let error = link(&[Request::WholeArchive, file("duplicate.a", &duplicate)]).unwrap_err();
    assert!(
        error.contains("duplicate") && error.contains("_start"),
        "{error}"
    );
    let single = archive(&[("start.o", &start)]);
    let requests = [
        Request::StartGroup,
        Request::WholeArchive,
        file("a.a", &single),
        file("a.a", &single),
        Request::EndGroup,
    ];
    assert_eq!(selected_names(&requests).len(), 2);
    assert!(link(&requests).unwrap_err().contains("duplicate"));
}

#[test]
fn whole_archive_preserves_comdat_and_discarded_references() {
    let start = object(".text\n.globl _start\n_start:\ncall answer\nret\n");
    let first = object(
        ".section .text.answer,\"axG\",@progbits,answer,comdat\n.globl answer\nanswer:\nret\n",
    );
    let second = object(
        ".section .text.answer,\"axG\",@progbits,answer,comdat\n.globl answer\nanswer:\ncall absent\nret\n",
    );
    let a = archive(&[("first.o", &first), ("second.o", &second)]);
    let requests = [
        file("start.o", &start),
        Request::WholeArchive,
        file("a.a", &a),
    ];
    assert_eq!(selected_names(&requests).len(), 3);
    link(&requests).unwrap();
}

#[test]
fn group_objects_are_processed_once_and_weak_undefined_is_lazy() {
    let start = object(".text\n.globl _start\n.weak unused\n_start:\n.quad unused\nret\n");
    let unused = object(".text\n.globl unused\nunused:\nret\n");
    let a = archive(&[("unused.o", &unused)]);
    let requests = [
        Request::StartGroup,
        file("start.o", &start),
        file("a.a", &a),
        Request::EndGroup,
    ];
    assert_eq!(selected_names(&requests), ["start.o"]);
    link(&requests).unwrap();
    assert!(
        Selection::prepare(&[Request::StartGroup, Request::EndGroup], "_start")
            .unwrap()
            .inputs()
            .is_empty()
    );
}

#[test]
fn malformed_groups_are_diagnosed_before_inputs_and_thin_archives_are_explicit() {
    for (requests, expected) in [
        (
            vec![file("bad", b"invalid"), Request::EndGroup],
            "--end-group without --start-group",
        ),
        (
            vec![Request::StartGroup, file("bad", b"invalid")],
            "--start-group without --end-group",
        ),
        (
            vec![Request::StartGroup, Request::EndGroup, Request::EndGroup],
            "--end-group without --start-group",
        ),
    ] {
        assert_eq!(
            Selection::prepare(&requests, "_start").unwrap_err(),
            expected
        );
    }
    let error = Selection::prepare(
        &[Request::WholeArchive, file("thin.a", b"!<thin>\n")],
        "_start",
    )
    .unwrap_err();
    assert!(error.contains("thin archives are unsupported"), "{error}");
}

// Promote a linked fixture's tables to a DSO interface without a host toolchain.
fn shared_interface() -> Vec<u8> {
    use object::ObjectSection;
    let obj = object(
        ".text\n.globl _start\n_start:\nret\n.globl needed\nneeded:\nret\n.section .dynamic,\"aw\",@progbits\n.quad 0\n.quad 0\n",
    );
    let mut image = elf::link(
        &[Input {
            name: "dso.o",
            bytes: &obj,
        }],
        Default::default(),
    )
    .unwrap()
    .bytes;
    let file = object::File::parse(image.as_slice()).unwrap();
    let dynamic = file.section_by_name(".dynamic").unwrap().index().0;
    let symbols = file.section_by_name(".symtab").unwrap().index().0;
    let strings = file.section_by_name(".strtab").unwrap().index().0;
    let symoff = file
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let needed = file
        .symbols()
        .find(|s| s.name().unwrap() == "needed")
        .unwrap()
        .index()
        .0;
    let shoff = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + symbols * 64 + 4..shoff + symbols * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image[symoff + needed * 24 + 6..symoff + needed * 24 + 8].fill(0);
    image[symoff + needed * 24 + 8..symoff + needed * 24 + 16].fill(0);
    image
}

#[test]
fn group_dso_demands_trigger_rescans_and_keep_library_order_and_deduplication() {
    let start = object(".text\n.globl _start\n_start:\nret\n");
    let needed = object(".text\n.globl needed\nneeded:\nret\n");
    let deps = archive(&[("needed.o", &needed)]);
    let dso = shared_interface();
    let requests = [
        file("start.o", &start),
        Request::StartGroup,
        file("deps.a", &deps),
        Request::WholeArchive,
        file("one.so", &dso),
        file("two.so", &dso),
        file("one.so", &dso),
        Request::EndGroup,
    ];
    assert_eq!(
        selected_names(&requests),
        ["start.o", "one.so", "two.so", "deps.a(needed.o)"]
    );
    link(&requests).unwrap();
}
