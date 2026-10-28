use keel_link::{Dylib, Input, Options};
use object::{Object, ObjectSymbol};
use std::collections::BTreeSet;

fn options() -> Options {
    Options {
        dylibs: vec![Dylib {
            install_name: "/usr/lib/libSystem.B.dylib".into(),
            exports: BTreeSet::new(),
            weak_exports: BTreeSet::new(),
        }],
        ..Options::default()
    }
}
fn object(source: &str) -> Vec<u8> {
    keel_asm::assemble_object(source).unwrap()
}
// Read the dynamic export trie; object::Object::exports uses LC_DYSYMTAB instead,
// which also contains non-exported private externs in Keel executables.
fn exports(bytes: &[u8]) -> BTreeSet<String> {
    fn u32le(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
    }
    fn uleb(b: &[u8], at: &mut usize) -> usize {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let v = b[*at];
            *at += 1;
            result |= usize::from(v & 127) << shift;
            if v & 128 == 0 {
                return result;
            }
            shift += 7;
            assert!(shift < 64);
        }
    }
    fn visit(trie: &[u8], offset: usize, prefix: String, out: &mut BTreeSet<String>, depth: usize) {
        assert!(depth < 1024);
        let mut at = offset;
        let size = uleb(trie, &mut at);
        if size != 0 {
            out.insert(prefix.clone());
        }
        at += size;
        let count = trie[at];
        at += 1;
        for _ in 0..count {
            let end = at + trie[at..].iter().position(|b| *b == 0).unwrap();
            let edge = std::str::from_utf8(&trie[at..end]).unwrap();
            at = end + 1;
            let child = uleb(trie, &mut at);
            visit(trie, child, format!("{prefix}{edge}"), out, depth + 1);
        }
    }
    let mut at = 32;
    for _ in 0..u32le(bytes, 16) {
        let cmd = u32le(bytes, at);
        let pair = match cmd {
            0x22 | 0x80000022 => Some((u32le(bytes, at + 40), u32le(bytes, at + 44))),
            0x80000033 => Some((u32le(bytes, at + 8), u32le(bytes, at + 12))),
            _ => None,
        };
        if let Some((offset, size)) = pair
            && size != 0
        {
            let mut out = BTreeSet::new();
            visit(&bytes[offset..offset + size], 0, String::new(), &mut out, 0);
            return out;
        }
        at += u32le(bytes, at + 4);
    }
    panic!("missing export trie");
}
fn fixtures() -> (Vec<u8>, Vec<u8>) {
    (
        object(".globl _main\n_main: b _helper\n"),
        object(
            ".private_extern _helper, _hidden_data\n_helper:\nadrp x9,_hidden_data@PAGE\nldr w0,[x9,_hidden_data@PAGEOFF]\nadd w0,w0,#37\nret\n.comm _hidden_data,4,2\n",
        ),
    )
}

#[test]
fn private_definitions_and_commons_resolve_but_are_not_exported() {
    let (a, b) = fixtures();
    let exe = keel_link::link(
        &[
            Input {
                name: "caller.o",
                bytes: &a,
            },
            Input {
                name: "hidden.o",
                bytes: &b,
            },
        ],
        options(),
    )
    .unwrap();
    let exported = exports(&exe.bytes);
    assert!(exported.contains("_main"));
    assert!(!exported.contains("_helper"));
    assert!(!exported.contains("_hidden_data"));
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    for name in ["_helper", "_hidden_data"] {
        let symbol = file.symbols().find(|s| s.name() == Ok(name)).unwrap();
        assert!(!symbol.is_undefined());
        assert_eq!(symbol.scope(), object::SymbolScope::Linkage);
    }
    // Linkage-unit visibility still participates in duplicate checking.
    let duplicate = object(".private_extern _helper\n_helper: ret\n");
    let error = keel_link::link(
        &[
            Input {
                name: "caller.o",
                bytes: &a,
            },
            Input {
                name: "hidden.o",
                bytes: &b,
            },
            Input {
                name: "duplicate.o",
                bytes: &duplicate,
            },
        ],
        options(),
    )
    .err()
    .unwrap();
    assert!(error.contains("duplicate definition `_helper`"), "{error}");
}

#[test]
fn public_common_or_definition_takes_precedence_over_private_common() {
    let (a, b) = fixtures();
    for source in [
        ".comm _hidden_data,8,3\n",
        ".globl _hidden_data\n.data\n_hidden_data: .long 0\n",
    ] {
        let public = object(source);
        for reverse in [false, true] {
            let mut inputs = vec![
                Input {
                    name: "caller.o",
                    bytes: &a,
                },
                Input {
                    name: "hidden.o",
                    bytes: &b,
                },
                Input {
                    name: "public.o",
                    bytes: &public,
                },
            ];
            if reverse {
                inputs.reverse();
            }
            let exe = keel_link::link(&inputs, options()).unwrap();
            assert!(exports(&exe.bytes).contains("_hidden_data"));
            assert!(!exports(&exe.bytes).contains("_helper"));
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn private_cross_object_call_runs_with_native_and_system_linkers() {
    let temp = std::env::temp_dir().join(format!("keel-private-extern-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let (a, b) = fixtures();
    let exe = keel_link::link(
        &[
            Input {
                name: "caller.o",
                bytes: &a,
            },
            Input {
                name: "hidden.o",
                bytes: &b,
            },
        ],
        options(),
    )
    .unwrap();
    let native = temp.join("native");
    keel_link::write_signed(&exe, &native).unwrap();
    let caller = temp.join("caller.o");
    let hidden = temp.join("hidden.o");
    std::fs::write(&caller, a).unwrap();
    std::fs::write(&hidden, b).unwrap();
    let system = temp.join("system");
    let out = std::process::Command::new("clang")
        .arg(&caller)
        .arg(&hidden)
        .arg("-Wl,-export_dynamic")
        .arg("-o")
        .arg(&system)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        exports(&std::fs::read(&system).unwrap()),
        exports(&exe.bytes)
    );
    for path in [&native, &system] {
        assert_eq!(
            std::process::Command::new(path).status().unwrap().code(),
            Some(37)
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
