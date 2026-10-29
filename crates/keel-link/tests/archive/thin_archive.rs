use keel_link::archive::{self, Target};
use object::read::archive::ArchiveFile;

fn thin(names: &[&str]) -> Vec<u8> {
    let mut out = b"!<thin>\n".to_vec();
    for name in names {
        out.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                "644",
                1
            )
            .as_bytes(),
        );
    }
    out
}
#[test]
fn snapshot_reads_current_bytes_in_order_and_keeps_duplicate_members() {
    let a = keel_asm::x86::assemble_object(".text\n.globl a\na: ret\n").unwrap();
    let b = keel_asm::x86::assemble_object(".text\n.globl b\nb: ret\n").unwrap();
    let mut seen = Vec::new();
    let bytes = archive::materialize_thin(
        &thin(&["a.o", "b.o", "a.o"]),
        Target::X86_64LinuxGnu,
        |name| {
            seen.push(name.to_owned());
            Ok(if name == "a.o" { a.clone() } else { b.clone() })
        },
    )
    .unwrap();
    assert_eq!(seen, ["a.o", "b.o", "a.o"]);
    let archive = ArchiveFile::parse(bytes.as_slice()).unwrap();
    assert!(!archive.is_thin());
    let members = archive.members().map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(members.len(), 3);
    for (member, expected) in members.iter().zip([&a, &b, &a]) {
        assert_eq!(member.data(bytes.as_slice()).unwrap(), expected);
    }
}
#[test]
fn missing_members_invalid_objects_and_regular_archives_reject() {
    let error = archive::materialize_thin(&thin(&["absent.o"]), Target::X86_64LinuxGnu, |_| {
        Err("not found".into())
    })
    .unwrap_err();
    assert!(error.contains("absent.o: not found"));
    assert!(
        archive::materialize_thin(&thin(&["bad.o"]), Target::X86_64LinuxGnu, |_| Ok(
            b"not ELF".to_vec()
        ))
        .is_err()
    );
    assert!(
        archive::materialize_thin(b"!<arch>\n", Target::X86_64LinuxGnu, |_| panic!(
            "must not read a regular archive"
        ))
        .unwrap_err()
        .contains("expected a GNU thin archive")
    );
}
