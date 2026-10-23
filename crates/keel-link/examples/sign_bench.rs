//! Native macOS benchmark; run with cargo run --release -p keel-link --example sign_bench -- OUTDIR.
//! Private source inclusion deliberately avoids adding a signing API to keel-link.
use std::{
    fs,
    io::{Cursor, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::Instant,
};
type Result<T> = std::result::Result<T, String>;
const PAGE: usize = 0x4000;
fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(s.into())
}
#[path = "../src/sign.rs"]
mod sign;
// The included source's existing unit tests refer to these crate-root names.
#[cfg(test)]
use keel_link::{Dylib, Input, Options, link};

// Matches install_output's no-debug-object path, omitting only signing.
fn unsigned_atomic(bytes: &[u8], path: &Path) {
    let parent = path.parent().unwrap();
    let mut temp = None;
    for n in 0..100 {
        let p = parent.join(format!(".keel-link-{}-{n}", std::process::id()));
        match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(f) => {
                temp = Some((p, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("{e}"),
        }
    }
    let (temp, mut file) = temp.unwrap();
    file.write_all(bytes).unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o755))
        .unwrap();
    drop(file);
    fs::rename(temp, path).unwrap();
}
fn checked(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{c:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("OUTDIR"));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let mut csv = fs::File::create(root.join("raw.csv")).unwrap();
    writeln!(
        csv,
        "size,payload_bytes,unsigned_bytes,signed_bytes,mode,iteration,ns"
    )
    .unwrap();
    for (label, size) in [
        ("tiny", 0),
        ("1MiB", 1 << 20),
        ("16MiB", 16 << 20),
        ("64MiB", 64 << 20),
    ] {
        let dir = root.join(label);
        fs::create_dir_all(&dir).unwrap();
        // Deterministic initialized data, not sparse zero-fill. Generation and linking untimed.
        let payload: Vec<u8> = (0..size)
            .map(|i| ((i as u64 * 131 + (i as u64 >> 8) * 17 + 29) & 255) as u8)
            .collect();
        fs::write(dir.join("payload.bin"), payload).unwrap();
        fs::write(dir.join("main.s"), format!(".text\n.globl _main\n_main:\nmov w0, #0\nret\n.section __TEXT,__const\n.p2align 4\n.incbin \"{}\"\n", dir.join("payload.bin").display())).unwrap();
        checked(
            Command::new("xcrun")
                .args(["clang", "-arch", "arm64", "-c"])
                .arg(dir.join("main.s"))
                .arg("-o")
                .arg(dir.join("main.o")),
        );
        let object = fs::read(dir.join("main.o")).unwrap();
        let exe = keel_link::link(
            &[keel_link::Input {
                name: "main.o",
                bytes: &object,
            }],
            keel_link::Options {
                dylibs: vec![keel_link::Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: Default::default(),
                    weak_exports: Default::default(),
                }],
                ..Default::default()
            },
        )
        .unwrap();
        fs::write(dir.join("unsigned"), &exe.bytes).unwrap();
        let mut memory = Cursor::new(Vec::with_capacity(
            exe.bytes.len() + exe.bytes.len() / 128 + 1024,
        ));
        let modes = [
            "core_memory",
            "core_file",
            "unsigned_atomic",
            "write_signed",
            "apple_sign",
            "apple_atomic",
            "spawn_true",
        ];
        // Three warmup cycles, then 21 measured cycles. Rotate order to spread drift.
        for cycle in 0..24 {
            for step in 0..modes.len() {
                let mode = modes[(step + cycle) % modes.len()];
                let path = dir.join(mode);
                // All inputs are fresh unsigned bytes, never repeated re-signing.
                let mut file = None;
                if mode == "core_memory" {
                    memory.get_mut().clear();
                    memory.get_mut().extend_from_slice(&exe.bytes);
                    memory.set_position(0);
                }
                if mode == "core_file" || mode == "apple_sign" {
                    // Remove before writing: always a fresh inode, avoiding signature cache reuse.
                    let _ = fs::remove_file(&path);
                    fs::write(&path, &exe.bytes).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                    if mode == "core_file" {
                        file = Some(
                            fs::OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open(&path)
                                .unwrap(),
                        );
                    }
                }
                let start = Instant::now();
                match mode {
                    "core_memory" => sign::sign(&mut memory).unwrap(),
                    "core_file" => sign::sign(file.as_mut().unwrap()).unwrap(),
                    "unsigned_atomic" => unsigned_atomic(&exe.bytes, &path),
                    "write_signed" => keel_link::write_signed(&exe, &path).unwrap(),
                    "apple_sign" => checked(
                        Command::new("/usr/bin/codesign")
                            .args(["--force", "--sign", "-"])
                            .arg(&path),
                    ),
                    "apple_atomic" => {
                        let temp = dir.join("apple-temp");
                        let _ = fs::remove_file(&temp);
                        fs::write(&temp, &exe.bytes).unwrap();
                        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755)).unwrap();
                        checked(
                            Command::new("/usr/bin/codesign")
                                .args(["--force", "--sign", "-"])
                                .arg(&temp),
                        );
                        fs::rename(temp, &path).unwrap();
                    }
                    "spawn_true" => checked(&mut Command::new("/usr/bin/true")),
                    _ => unreachable!(),
                }
                let ns = start.elapsed().as_nanos();
                drop(file); // Closing the file is outside core_file, inside atomic APIs.
                std::hint::black_box(memory.get_ref());
                if cycle >= 3 {
                    writeln!(
                        csv,
                        "{label},{size},{},{},{mode},{},{}",
                        exe.bytes.len(),
                        exe.bytes.len() + 128 + exe.bytes.len().div_ceil(4096) * 32,
                        cycle - 3,
                        ns
                    )
                    .unwrap();
                }
            }
        }
        fs::write(dir.join("core_memory"), memory.get_ref()).unwrap();
        fs::set_permissions(dir.join("core_memory"), fs::Permissions::from_mode(0o755)).unwrap();
        for mode in [
            "core_memory",
            "core_file",
            "write_signed",
            "apple_sign",
            "apple_atomic",
        ] {
            checked(
                Command::new("/usr/bin/codesign")
                    .args(["--verify", "--strict", "--verbose=2"])
                    .arg(dir.join(mode)),
            );
        }
        assert_eq!(
            fs::read(dir.join("core_memory")).unwrap(),
            fs::read(dir.join("write_signed")).unwrap()
        );
        assert_eq!(
            fs::read(dir.join("core_file")).unwrap(),
            fs::read(dir.join("write_signed")).unwrap()
        );
        if label == "tiny" {
            for mode in [
                "core_memory",
                "core_file",
                "write_signed",
                "apple_sign",
                "apple_atomic",
            ] {
                checked(&mut Command::new(dir.join(mode)));
            }
        }
        eprintln!(
            "{label}: {} unsigned bytes; 3 warmups + 21 samples/mode; strict verification passed",
            exe.bytes.len()
        );
    }
}
