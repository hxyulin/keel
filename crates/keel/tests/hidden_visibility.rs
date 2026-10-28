//! C `visibility("hidden"/"internal")`: emitted on both targets as text and
//! objects, and hidden symbols link within a program but are not exported.
use object::{Object, ObjectSymbol, SymbolScope};
use std::{fs, path::Path, process::Command};

/// Lua-style internal declarations, whose definitions inherit visibility,
/// and directly annotated definitions of every data kind.
const LIBRARY: &str = r#"
#define INTERNAL extern __attribute__((visibility("internal")))
INTERNAL int hidden_add(int);
INTERNAL int hidden_data;
INTERNAL const char hidden_const[];
int hidden_data = 40;
const char hidden_const[] = "hidden";
__attribute__((visibility("hidden"))) int hidden_zero[4];
__attribute__((visibility("hidden"))) const char *const hidden_table[] = { hidden_const };
__attribute__((visibility("hidden"))) _Thread_local int hidden_tls = 2;
static int helper(int x) __attribute__((visibility("hidden")));
static int helper(int x) { return x + 1; }
extern void hook(void) __attribute__((__weak__));
int hidden_add(int x) { return helper(x) + hidden_data + hidden_zero[1] + hidden_tls - 1; }
__attribute__((visibility("default"))) int exported(void) { return hidden_add(1); }
"#;

const HIDDEN: [&str; 6] = [
    "hidden_add",
    "hidden_data",
    "hidden_const",
    "hidden_zero",
    "hidden_table",
    "hidden_tls",
];

const MAIN: &str = r#"
#include <stdio.h>
#include <string.h>
#define HIDDEN extern __attribute__((visibility("hidden")))
HIDDEN int hidden_add(int);
HIDDEN int hidden_data;
HIDDEN const char hidden_const[];
HIDDEN int hidden_zero[4];
HIDDEN const char *const hidden_table[];
HIDDEN _Thread_local int hidden_tls;
int exported(void);
int main(void) {
    hidden_zero[1] = 1;
    hidden_tls = 3;
    printf("%d %d %s\n", hidden_add(1), exported(), hidden_table[0]);
    return hidden_add(0) == 44 && strcmp(hidden_const, "hidden") == 0 ? 0 : 1;
}
"#;

fn command(c: &mut Command) -> String {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}: {}\n{}\n{}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}

fn keel(dir: &Path, args: &[&str]) {
    command(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(dir)
            .args(args),
    );
}

fn symbol_name(target: &str, name: &str) -> String {
    match target {
        "aarch64-apple-darwin" => format!("_{name}"),
        _ => name.to_owned(),
    }
}

#[test]
fn hidden_and_internal_definitions_are_emitted_on_both_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden-visibility-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("library.c"), LIBRARY).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for optimize in [false, true] {
            let opt: &[&str] = if optimize { &["-O"] } else { &[] };
            let tag = format!("{target}-{optimize}");
            let asm = format!("{tag}.s");
            let obj = format!("{tag}.o");
            keel(
                &dir,
                &[&["cc", "--target", target, "library.c", "-o", &asm], opt].concat(),
            );
            let text = fs::read_to_string(dir.join(&asm)).unwrap();
            let directive = |name: &str| match target {
                "aarch64-apple-darwin" => format!(".private_extern\t_{name}"),
                _ => format!(".hidden\t\"{name}\""),
            };
            for name in HIDDEN {
                assert_eq!(
                    text.lines().filter(|l| l.trim() == directive(name)).count(),
                    1,
                    "{tag} {name}"
                );
            }
            for name in ["exported", "helper", "hook"] {
                assert!(!text.contains(&directive(name)), "{tag} {name}");
            }
            keel(
                &dir,
                &[
                    &["cc", "--target", target, "--emit-object", "library.c"],
                    opt,
                    &["-o", &obj],
                ]
                .concat(),
            );
            let bytes = fs::read(dir.join(&obj)).unwrap();
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let scope = |name: &str| {
                let name = symbol_name(target, name);
                let symbol = file.symbols().find(|s| s.name() == Ok(&name));
                symbol.map(|s| (s.is_undefined(), s.scope()))
            };
            for name in HIDDEN {
                assert_eq!(
                    scope(name),
                    Some((false, SymbolScope::Linkage)),
                    "{tag} {name}"
                );
            }
            assert_eq!(
                scope("exported"),
                Some((false, SymbolScope::Dynamic)),
                "{tag}"
            );
            // Unreferenced weak declarations emit nothing.
            assert_eq!(scope("hook"), None, "{tag}");
            if let Some(helper) = scope("helper") {
                assert_eq!(helper, (false, SymbolScope::Compilation), "{tag}");
            }
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_hidden_symbols_link_across_objects_but_are_not_exported() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden-visibility-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("library.c"), LIBRARY).unwrap();
    fs::write(dir.join("main.c"), MAIN).unwrap();
    for optimize in [false, true] {
        let opt: &[&str] = if optimize { &["-O"] } else { &[] };
        let obj = format!("library-{optimize}.o");
        keel(
            &dir,
            &[&["cc", "--emit-object", "library.c"], opt, &["-o", &obj]].concat(),
        );
        // Export every default-visibility symbol of the executable. (Keel's
        // x86-64 thread-locals are local-exec, so it cannot build a shared
        // library from this object.)
        let exe = format!("program-{optimize}");
        command(Command::new("clang").current_dir(&dir).args([
            "-rdynamic",
            "main.c",
            &obj,
            "-o",
            &exe,
        ]));
        let out = command(&mut Command::new(dir.join(&exe)));
        assert_eq!(out, "45 45 hidden\n");
        let bytes = fs::read(dir.join(&exe)).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let exports: Vec<String> = file
            .exports()
            .unwrap()
            .iter()
            .map(|e| String::from_utf8_lossy(e.name()).into_owned())
            .collect();
        let prefix = if cfg!(target_os = "macos") { "_" } else { "" };
        assert!(
            exports.contains(&format!("{prefix}exported")),
            "{exports:?}"
        );
        for name in HIDDEN {
            assert!(
                !exports.iter().any(|e| e.ends_with(name)),
                "{name}: {exports:?}"
            );
        }
    }
}
