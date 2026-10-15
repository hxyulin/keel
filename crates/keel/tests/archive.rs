//! Native correctness/tool interoperability; no timing assertions.
use std::{
    path::Path,
    process::{Command, Output},
};
fn keel(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(args)
        .output()
        .unwrap()
}
fn success(out: Output) -> Output {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn archive_cli_native_linkers_tools_and_preserved_failures() {
    let dir = std::env::temp_dir().join(format!("keel-archive-driver-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("one")).unwrap();
    std::fs::create_dir_all(dir.join("two")).unwrap();
    let linux = cfg!(all(target_os = "linux", target_arch = "x86_64"));
    let darwin = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    if !linux && !darwin {
        return;
    }
    let target = if linux {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-apple-darwin"
    };
    let sources = if linux {
        [
            ".text\n.globl _start\n_start:\ncall first\nmov edi,eax\nmov eax,60\nsyscall\n",
            ".text\n.globl first\nfirst:\njmp second\n",
            ".text\n.globl second\nsecond:\nmov eax,42\nret\n",
        ]
    } else {
        [
            ".text\n.globl _main\n_main:\nb _first\n",
            ".text\n.globl _first\n_first:\nb _second\n",
            ".text\n.globl _second\n_second:\nmov w0,#42\nret\n",
        ]
    };
    let paths = [
        dir.join("entry.o"),
        dir.join("one/same.o"),
        dir.join("two/same.o"),
    ];
    for (i, source) in sources.iter().enumerate() {
        let assembly = dir.join(format!("{i}.s"));
        std::fs::write(&assembly, source).unwrap();
        success(keel(&[
            "asm",
            "--target",
            target,
            path(&assembly),
            "-o",
            path(&paths[i]),
        ]));
    }
    let long = dir.join("an unused conflicting member with long name.o");
    std::fs::copy(&paths[0], &long).unwrap();
    let archive = dir.join("library.a");
    let args = [
        "archive",
        "--target",
        target,
        "-o",
        path(&archive),
        path(&paths[2]),
        path(&long),
        path(&paths[1]),
    ];
    success(keel(&args));
    let bytes = std::fs::read(&archive).unwrap();
    success(keel(&args));
    assert_eq!(bytes, std::fs::read(&archive).unwrap());
    let listing = success(
        Command::new("ar")
            .args(["t", path(&archive)])
            .output()
            .unwrap(),
    );
    let listing = String::from_utf8(listing.stdout).unwrap();
    assert_eq!(
        listing
            .lines()
            .filter(|name| *name != "__.SYMDEF")
            .collect::<Vec<_>>(),
        [
            "same.o",
            long.file_name().unwrap().to_str().unwrap(),
            "same.o"
        ]
    );
    success(Command::new("nm").arg(&archive).output().unwrap());
    let native = dir.join("native");
    success(keel(&[
        "link",
        "--target",
        target,
        path(&paths[0]),
        path(&archive),
        "-o",
        path(&native),
    ]));
    assert_eq!(Command::new(&native).status().unwrap().code(), Some(42));
    let system = dir.join("system");
    let mut clang = Command::new("clang");
    if linux {
        clang.args(["-nostdlib", "-no-pie", "-Wl,-e,_start"]);
    }
    success(
        clang
            .arg(&paths[0])
            .arg(&archive)
            .arg("-o")
            .arg(&system)
            .output()
            .unwrap(),
    );
    assert_eq!(Command::new(&system).status().unwrap().code(), Some(42));
    // Compiler-produced commons and weak definitions must also be discoverable
    // through the symbol index, while undefined references stay unindexed.
    let symbols_c = dir.join("symbols.c");
    let consumer_c = dir.join("consumer.c");
    let symbols_o = dir.join("symbols.o");
    let symbols_a = dir.join("symbols.a");
    std::fs::write(
        &symbols_c,
        "int common; __attribute__((weak)) int weak_answer(void) { return 42; }\n",
    )
    .unwrap();
    std::fs::write(&consumer_c, "extern int common; extern int weak_answer(void); int main(void) { return weak_answer() + common; }\n").unwrap();
    success(
        Command::new("clang")
            .args(["-c", "-fcommon"])
            .arg(&symbols_c)
            .arg("-o")
            .arg(&symbols_o)
            .output()
            .unwrap(),
    );
    success(keel(&[
        "archive",
        "--target",
        target,
        "-o",
        path(&symbols_a),
        path(&symbols_o),
    ]));
    let symbols_exe = dir.join("symbols-exe");
    success(
        Command::new("clang")
            .arg(&consumer_c)
            .arg(&symbols_a)
            .arg("-o")
            .arg(&symbols_exe)
            .output()
            .unwrap(),
    );
    assert_eq!(
        Command::new(&symbols_exe).status().unwrap().code(),
        Some(42)
    );
    let bad = dir.join("bad.o");
    std::fs::write(&bad, b"not an object").unwrap();
    for member in [&bad, &paths[0]] {
        let wrong_target = if member == &bad {
            target
        } else if linux {
            "aarch64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        assert!(
            !keel(&[
                "archive",
                "--target",
                wrong_target,
                "-o",
                path(&archive),
                path(member)
            ])
            .status
            .success()
        );
        assert_eq!(bytes, std::fs::read(&archive).unwrap());
    }
    for args in [
        vec!["archive"],
        vec!["archive", "--target"],
        vec!["archive", "--target", "bogus"],
        vec!["archive", "-o"],
        vec!["archive", "-o", "x.a"],
        vec!["archive", "--thin"],
    ] {
        assert!(!keel(&args).status.success());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
