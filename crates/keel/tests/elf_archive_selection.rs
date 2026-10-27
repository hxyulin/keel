use keel_link::{
    Input,
    archive::{self, Target},
};
use object::{Object, ObjectSymbol};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("keel-selection-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let fixture = Self(path);
        for (name, source) in [
            (
                "start.o",
                ".text\n.globl _start\n_start:\ncall first\nret\n",
            ),
            ("first.o", ".text\n.globl first\nfirst:\ncall second\nret\n"),
            (
                "second.o",
                ".text\n.globl second\nsecond:\ncall last\nret\n",
            ),
            ("last.o", ".text\n.globl last\nlast:\nret\n"),
            ("unused.o", ".text\n.globl unused\nunused:\nret\n"),
        ] {
            std::fs::write(
                fixture.0.join(name),
                keel_asm::x86::assemble_object(source).unwrap(),
            )
            .unwrap();
        }
        for (name, members) in [
            ("liba.a", vec!["last.o", "first.o"]),
            ("libb.a", vec!["second.o"]),
            ("libunused.a", vec!["unused.o"]),
            ("libduplicate.a", vec!["start.o"]),
        ] {
            let owned = members
                .iter()
                .map(|m| (*m, std::fs::read(fixture.0.join(m)).unwrap()))
                .collect::<Vec<_>>();
            let inputs = owned
                .iter()
                .map(|(name, bytes)| Input { name, bytes })
                .collect::<Vec<_>>();
            std::fs::write(
                fixture.0.join(name),
                archive::build(&inputs, Target::X86_64LinuxGnu).unwrap(),
            )
            .unwrap();
        }
        fixture
    }
    fn keel(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&self.0)
            .args(["link", "--target", "x86_64-unknown-linux-gnu"])
            .args(args)
            .env("PATH", "")
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn symbols(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let mut names = file
        .symbols()
        .filter(|s| s.is_global() && !s.is_undefined())
        .map(|s| s.name().unwrap().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn native_cli_groups_aliases_and_whole_archive() {
    let f = Fixture::new("cli");
    let plain = f.keel(&["start.o", "liba.a", "libb.a", "-o", "plain"]);
    assert!(!plain.status.success());
    assert!(String::from_utf8_lossy(&plain.stderr).contains("undefined symbol `last`"));
    for (start, end) in [("--start-group", "--end-group"), ("-(", "-)")] {
        success(f.keel(&[
            "start.o",
            start,
            "-L",
            ".",
            "-la",
            "-lb",
            end,
            "--whole-archive",
            "libunused.a",
            "--no-whole-archive",
            "libduplicate.a",
            "-o",
            "group",
        ]));
        let names = symbols(&f.0.join("group"));
        for name in ["_start", "first", "second", "last", "unused"] {
            assert!(names.iter().any(|n| n == name), "{names:?}");
        }
    }
    let duplicate = f.keel(&[
        "start.o",
        "--whole-archive",
        "libduplicate.a",
        "-o",
        "duplicate",
    ]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate"));
    assert!(!f.0.join("duplicate").exists());
}

#[test]
fn native_cli_rejects_unbalanced_groups_before_opening_inputs() {
    let f = Fixture::new("bad");
    for (args, expected) in [
        (
            vec!["missing.o", "--end-group"],
            "--end-group without --start-group",
        ),
        (
            vec!["--start-group", "missing.o"],
            "--start-group without --end-group",
        ),
        (vec!["-(", "-)", "-)"], "--end-group without --start-group"),
    ] {
        let result = f.keel(&args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(expected));
    }
}

/// Set KEEL_GNU_LD to a GNU ELF ld executable to run the reference comparisons.
#[test]
fn gnu_ld_archive_selection_comparison() {
    let Some(ld) = std::env::var_os("KEEL_GNU_LD") else {
        eprintln!("GNU ld comparison skipped: set KEEL_GNU_LD");
        return;
    };
    let f = Fixture::new("gnu");
    let cases = [
        (vec!["start.o", "liba.a", "libb.a"], false),
        (
            vec![
                "start.o",
                "--start-group",
                "liba.a",
                "libb.a",
                "--end-group",
            ],
            true,
        ),
        (
            vec!["start.o", "-(", "liba.a", "-(", "libb.a", "-)", "-)"],
            true,
        ),
        (
            vec![
                "start.o",
                "--start-group",
                "-L",
                ".",
                "-la",
                "-lb",
                "--end-group",
                "--whole-archive",
                "libunused.a",
                "--no-whole-archive",
                "libduplicate.a",
            ],
            true,
        ),
        (
            vec![
                "--whole-archive",
                "libduplicate.a",
                "liba.a",
                "libb.a",
                "libunused.a",
            ],
            true,
        ),
        (vec!["start.o", "--whole-archive", "libduplicate.a"], false),
        (
            vec!["--whole-archive", "libduplicate.a", "libduplicate.a"],
            false,
        ),
        (vec!["start.o", "--end-group"], false),
    ];
    for (args, expected) in cases {
        let mut native = args.clone();
        native.extend(["-o", "keel"]);
        let actual = f.keel(&native);
        let reference = Command::new(&ld)
            .current_dir(&f.0)
            .args(["-m", "elf_x86_64"])
            .args(&args)
            .args(["-o", "gnu"])
            .output()
            .unwrap();
        assert_eq!(
            actual.status.success(),
            expected,
            "Keel {args:?}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(
            reference.status.success(),
            expected,
            "GNU {args:?}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        if expected {
            let selected = ["_start", "first", "second", "last", "unused"];
            let relevant = |path: &Path| {
                symbols(path)
                    .into_iter()
                    .filter(|s| selected.contains(&s.as_str()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                relevant(&f.0.join("keel")),
                relevant(&f.0.join("gnu")),
                "{args:?}"
            );
        }
    }
    // GNU warns and closes an unmatched start; Keel deliberately rejects it.
    let reference = Command::new(ld)
        .current_dir(&f.0)
        .args([
            "start.o",
            "--start-group",
            "liba.a",
            "libb.a",
            "-o",
            "gnu-unclosed",
        ])
        .output()
        .unwrap();
    success(reference);
    assert!(
        !f.keel(&["start.o", "--start-group", "liba.a", "libb.a"])
            .status
            .success()
    );
}

#[test]
fn partial_link_groups_preserve_relocations_until_final_link() {
    let fixture = Fixture::new("partial");
    success(fixture.keel(&[
        "-r",
        "start.o",
        "--start-group",
        "liba.a",
        "libb.a",
        "--end-group",
        "-o",
        "partial.o",
    ]));
    let bytes = std::fs::read(fixture.0.join("partial.o")).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.kind(), object::ObjectKind::Relocatable);
    use object::ObjectSection;
    assert_eq!(
        file.sections()
            .map(|s| s.relocations().count())
            .sum::<usize>(),
        3
    );
    success(fixture.keel(&["partial.o", "-o", "final"]));
    assert!(symbols(&fixture.0.join("final")).contains(&"last".to_owned()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(fixture.0.join("partial.o"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}

#[test]
fn thin_archive_paths_are_relative_to_archive_and_missing_reads_are_atomic() {
    let fixture = Fixture::new("thin");
    std::fs::create_dir(fixture.0.join("sub")).unwrap();
    let member =
        keel_asm::x86::assemble_object(".text\n.globl thin_value\nthin_value: ret\n").unwrap();
    std::fs::write(fixture.0.join("sub/member.o"), member).unwrap();
    let mut thin = b"!<thin>\n".to_vec();
    thin.extend_from_slice(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "member.o/", 0, 0, 0, "644", 1
        )
        .as_bytes(),
    );
    std::fs::write(fixture.0.join("sub/thin.a"), thin).unwrap();
    success(fixture.keel(&["-r", "--whole-archive", "sub/thin.a", "-o", "partial.o"]));
    assert!(symbols(&fixture.0.join("partial.o")).contains(&"thin_value".to_owned()));
    std::fs::write(fixture.0.join("untouched"), b"keep existing output").unwrap();
    std::fs::remove_file(fixture.0.join("sub/member.o")).unwrap();
    let failed = fixture.keel(&["-r", "--whole-archive", "sub/thin.a", "-o", "untouched"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("member.o"));
    assert_eq!(
        std::fs::read(fixture.0.join("untouched")).unwrap(),
        b"keep existing output"
    );
}
