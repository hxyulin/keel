//! Subset emission and native callback/exception oracles; run under fast_opt_locked.py.
use std::{fs, path::Path, process::Command};
const SOURCE: &str = include_str!("fixtures/full_bounded_inline.c");
const ORACLE: &str = include_str!("fixtures/full_bounded_inline_oracle.cpp");
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
fn keel(dir: &Path, args: &[&str]) -> String {
    command(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(dir)
            .args(args),
    )
}
fn passes(bounded: bool) -> String {
    keel_opt::OptimizationTier::Full
        .pipeline()
        .iter()
        .map(|p| {
            if !bounded && matches!(*p, "inline-noninterposable" | "inline-cheap-reserve") {
                "inline"
            } else {
                *p
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn function<'a>(text: &'a str, name: &str) -> &'a str {
    text.split(&format!("func @{name}("))
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap()
}
#[test]
fn both_target_emission_removes_wrappers_and_retains_slow_calls() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("full-bounded-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let mut retained = None;
        for bounded in [false, true] {
            let pipeline = passes(bounded);
            let ir = keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "-g",
                    "-p",
                    &pipeline,
                    "--emit-ir",
                    "source.c",
                ],
            );
            let exercise = function(&ir, "exercise");
            for wrapper in ["poll", "release", "invoke"] {
                assert_eq!(
                    exercise.contains(&format!("call @{wrapper}(")),
                    !bounded,
                    "{target} {bounded} {exercise}"
                );
            }
            for slow in ["poll_slow", "release_slow", "invoke_slow"] {
                assert_eq!(
                    exercise.contains(&format!("call @{slow}(")),
                    bounded,
                    "{target} {bounded} {exercise}"
                );
            }
            if bounded {
                retained = Some(ir);
            }
            let obj = format!("{target}-{bounded}.o");
            keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "-g",
                    "-p",
                    &pipeline,
                    "--emit-object",
                    "source.c",
                    "-o",
                    &obj,
                ],
            );
            assert!(fs::metadata(dir.join(obj)).unwrap().len() > 0);
        }
        fs::write(dir.join(format!("{target}.keel")), retained.unwrap()).unwrap();
        // Exercise the exact retained Lua helper through backend lowering too.
        fs::write(
            dir.join("lua.keel"),
            include_str!("../../keel-opt/tests/fixtures/full-inline-index2value.keel"),
        )
        .unwrap();
        keel(
            &dir,
            &[
                "compile",
                "--target",
                target,
                "-g",
                "-O",
                "--emit-object",
                "lua.keel",
                "-o",
                &format!("lua-{target}.o"),
            ],
        );
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_arm_callbacks_destructors_and_unwinding_match_independent_oracle() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("full-bounded-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(dir.join("oracle.cpp"), ORACLE).unwrap();
    command(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-c",
        "source.c",
        "-o",
        "reference.o",
    ]));
    for tag in ["reference", "control", "bounded"] {
        let obj = format!("{tag}.o");
        if tag != "reference" {
            keel(
                &dir,
                &[
                    "cc",
                    "-g",
                    "-p",
                    &passes(tag == "bounded"),
                    "--emit-object",
                    "source.c",
                    "-o",
                    &obj,
                ],
            );
        }
        command(Command::new("clang++").current_dir(&dir).args([
            "-O2",
            "oracle.cpp",
            &obj,
            "-o",
            tag,
        ]));
        assert_eq!(
            command(&mut Command::new(dir.join(tag))),
            "callbacks, recursive slow calls, byval, exceptions: ok\n"
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_arm_retained_lua_index_paths_match_independent_layout_oracle() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("full-bounded-lua-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("lua.keel"),
        include_str!("../../keel-opt/tests/fixtures/full-inline-index2value.keel"),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        r#"
#include <stddef.h>
#include <stdint.h>
typedef struct { void *object; unsigned char tag, pad[7]; } Cell;
typedef struct { Cell *function; } Call;
typedef struct { char pad[16]; Cell *top; void *global; Call *call; } State;
typedef struct { char pad[64]; Cell registry, nil; } Global;
typedef struct { char pad[32]; Cell upvalues[3]; } Closure;
extern void *lookup(void *, int);
int main(void) {
 _Static_assert(offsetof(State,call)==32 && offsetof(Cell,tag)==8, "layout");
 Cell stack[8]={0}; Global global={0}; Closure closure={0};
 Call call={stack}; State state={{0},stack+5,&global,&call};
 stack[0].object=&closure; stack[0].tag=102; closure.pad[10]=3;
 int indices[]={1,4,5,6,0,-1,-4,-1001000,-1001001,-1001002,-1001003,-1001004};
 void *expected[]={stack+1,stack+4,&global.nil,&global.nil,stack+5,stack+4,stack+1,&global.registry,
  closure.upvalues,closure.upvalues+1,closure.upvalues+2,&global.nil};
 for(unsigned i=0;i<sizeof(indices)/sizeof(*indices);i++)
  if(lookup(&state,indices[i])!=expected[i]) return 1;
 stack[0].tag=0;
 if(lookup(&state,-1001001)!=&global.nil) return 2;
 return 0;
}
"#,
    )
    .unwrap();
    for bounded in [false, true] {
        let obj = format!("lua-{bounded}.o");
        keel(
            &dir,
            &[
                "compile",
                "-g",
                "-p",
                &passes(bounded),
                "--emit-object",
                "lua.keel",
                "-o",
                &obj,
            ],
        );
        let exe = format!("lua-{bounded}");
        command(
            Command::new("clang")
                .current_dir(&dir)
                .args(["-O2", "oracle.c", &obj, "-o", &exe]),
        );
        command(&mut Command::new(dir.join(exe)));
    }
}
