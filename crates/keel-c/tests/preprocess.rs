//! Differential token comparisons ignore formatting and line-marker choices.
use keel_c::{PreprocessOptions, preprocess};
use std::io::Write;
use std::process::{Command, Stdio};

fn clang(src: &str) -> String {
    let mut child = Command::new("clang")
        .args(["-E", "-P", "-std=c11", "-x", "c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = src.to_owned();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

// Have Clang lex each output too, so removing whitespace cannot accidentally
// equate `+ +` with `++`, or discard spaces inside string literals.
fn tokens(src: &str) -> Vec<String> {
    let mut child = Command::new("clang")
        .args([
            "-fsyntax-only",
            "-Xclang",
            "-dump-raw-tokens",
            "-x",
            "c",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = src
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with("#pragma")
                && !line
                    .strip_prefix("# ")
                    .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    String::from_utf8(out.stderr)
        .unwrap()
        .lines()
        .filter(|l| l.contains("Loc=<") && !l.starts_with("eof "))
        .filter(|l| {
            l.split_once(" '").is_some_and(|(kind, _)| {
                !kind.is_empty() && kind.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_')
            }) && !l.starts_with("comment ")
        })
        .filter(|l| {
            !l.starts_with("unknown '")
                || !l
                    .split('\t')
                    .next()
                    .unwrap()
                    .trim_start_matches("unknown '")
                    .trim_end_matches('\'')
                    .trim()
                    .is_empty()
        })
        .map(|l| {
            l.split("\t")
                .next()
                .unwrap()
                .to_string()
                .replace("'<:'", "'['")
                .replace("':>'", "']'")
                .replace("'<%'", "'{'")
                .replace("'%>'", "'}'")
                .replace("'%:'", "'#'")
                .replace("'%:%:'", "'##'")
        })
        .collect()
}
fn parity(src: &str) {
    let got = preprocess(src, "<stdin>", &PreprocessOptions::default()).unwrap();
    assert_eq!(
        tokens(&got),
        tokens(&clang(src)),
        "source:\n{src}\nKeel:\n{got}"
    );
}

#[test]
fn object_function_and_rescanning() {
    parity(
        r#"
#define A 42
#define EMPTY
#define F(x) ((x)+(x))
#define ALIAS F
#define SELF SELF
#define X Y
#define Y X
A EMPTY F(A) ALIAS (2) F(F(1)) SELF X
#define f(a) a*g
#define g(a) f(a)
f(2)(9)
#define h(x) x
h(h)(1)
#define ID(x) x
ID(F)(4)
#define LP (
F LP 3)
"#,
    );
}

#[test]
fn stringify_paste_and_variadic() {
    parity(
        r#"
#define VALUE 12
#define STR(x) #x
#define XSTR(x) STR(x)
#define CAT(x,y) x ## y
#define XCAT(x,y) CAT(x,y)
#define CAT3(x,y,z) x ## y ## z
#define xy 99
STR(<: %: %:%:) STR( VALUE  + /* comment */ "a\\b" ) XSTR(VALUE)
CAT(<,:) CAT(%,:) CAT(x,y) XCAT(VALUE,3) CAT(,x) CAT(x,) CAT(,) CAT3(,,) CAT3(x,,y)
#define ARGS(x, ...) x + __VA_ARGS__
ARGS(1,2,3) ARGS(1)
#define LOG(x, ...) call(x, ## __VA_ARGS__)
LOG(1) LOG(1,) LOG(1,2,3)
#define IGNORE(x) 5
IGNORE(__COUNTER__) __COUNTER__
#define TWICE(x) x x
TWICE(__COUNTER__) __COUNTER__
#define ID(x) x
ID(a ## b)
"#,
    );
}

#[test]
fn conditional_expressions() {
    parity(
        r#"
#define A 4
#define ADD(x,y) ((x)+(y))
#if defined A && defined(A) && !defined missing && ADD(A,2) == 6
one
#elif 1/0
bad
#else
bad
#endif
#if 0
#if invalid syntax ! &&
#error ignored
#endif
#elif 1
ok
#endif
#if (0 && 1/0) || (1 || 1/0)
short_circuit
#endif
#if (1 ? 7 : 1/0) == 7 && (0 ? 1/0 : 8) == 8
conditional
#endif
#if -1 < 1U
bad
#else
unsigned_conversion
#endif
#if (0xffffffffffffffffULL >> 63) == 1 && (-4 >> 1) == -2 && '\n' == 10
integers
#endif
#define DRIVERKIT defined(__DRIVERKIT_VERSION_MIN_REQUIRED)
#if !DRIVERKIT
macro_defined
#endif
#if UNDEFINED == 0 && (2 + 3 * 4 == 14) && (8 >> 1 + 1) == 2
precedence
#endif
#if (1 ? -1 : 0U) < 0
bad
#else
conditional_unsigned
#endif
"#,
    );
}

#[test]
fn splicing_comments_multiline_and_locations() {
    parity(
        "#define ABC 4\\\n2\n#define F(x) x+x\nABC /* c */ F\n(3\n)\n#define STR(x) #x\nSTR(a/**/b)\n#line 100 \"virtual.c\"\n__FILE__ __LINE__\n#define LOC __LINE__\nLOC\n",
    );
    parity("%:define X 7\nX <: 1 :> <% 2 %>\n");
}

#[test]
fn include_search_guards_once_and_macro_names() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-includes");
    std::fs::create_dir_all(dir.join("search")).unwrap();
    std::fs::create_dir_all(dir.join("later")).unwrap();
    std::fs::write(
        dir.join("value.h"),
        "#ifndef VALUE_H\n#define VALUE_H\n#define VALUE 42\nint local;\n#endif\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("search/value.h"),
        "int search;\n#if __has_include_next(<value.h>)\n#include_next <value.h>\n#endif\n",
    )
    .unwrap();
    std::fs::write(dir.join("later/value.h"), "int later;\n").unwrap();
    std::fs::write(dir.join("once.h"), "#pragma once\nint once;\n").unwrap();
    let src = "#define H \"value.h\"\n#include H\n#include H\n#include <value.h>\n#include \"once.h\"\n#include \"once.h\"\n#if __has_include(\"value.h\") && !__has_include(<missing-header.h>)\nint answer = VALUE;\n#endif\n";
    let opts = PreprocessOptions {
        include_dirs: vec![dir.join("search"), dir.join("later")],
        ..Default::default()
    };
    let got = preprocess(src, dir.join("main.c").to_str().unwrap(), &opts).unwrap();
    assert_eq!(
        tokens(&got),
        tokens("int local; int search; int later; int once; int answer = 42;")
    );
}

#[test]
fn rejects_invalid_input_with_source_location() {
    for (src, message) in [
        ("#if 1\n", "unterminated conditional"),
        ("#else\n", "conditional without"),
        ("#if 1\n#else\n#else\n#endif", "after #else"),
        ("#if 1/0\n#endif", "division by zero"),
        ("#if 1 << 64\n#endif", "invalid shift"),
        ("#if (\n#endif", "expected operand"),
        (
            "#define EMPTY\n#if EMPTY\n#endif",
            "expected #if expression",
        ),
        ("#define F(x) x\nF(1,2)", "wrong number"),
        ("#define F(x) x\nF(", "unterminated macro"),
        ("#define F(x) #missing", "must precede"),
        ("#define F(x) x ## +\nF(a)", "does not form"),
        ("#define F(x,x) x", "duplicate macro"),
        ("#include \"not-a-real-header.h\"", "header not found"),
        ("/*", "unterminated comment"),
        ("#error intentional", "intentional"),
        ("#not_a_directive", "unsupported preprocessing"),
    ] {
        let error = preprocess(src, "broken.c", &PreprocessOptions::default()).unwrap_err();
        assert!(error.message.contains(message), "{src}: {error}");
        assert_eq!(error.file, "broken.c");
        assert!(error.line >= 1);
    }
}

#[test]
fn target_environment_matches_abi_and_queries() {
    for target in keel_ir::Target::ALL {
        let opts = PreprocessOptions {
            target,
            ..Default::default()
        };
        let got = preprocess("#ifndef __clang__\n#error not clang\n#endif\n__SIZEOF_POINTER__ __SIZEOF_LONG__ __has_builtin(__builtin_expect) __has_feature(c_atomic) __SIZEOF_LONG_DOUBLE__\n#if defined(__aarch64__)\narm\n#elif defined(__x86_64__)\nx86\n#else\n#error missing target architecture\n#endif\n", "test.c", &opts).unwrap();
        let expected = match target {
            keel_ir::Target::Aarch64AppleDarwin => "8 8 1 1 8 arm",
            keel_ir::Target::X86_64LinuxGnu => "8 8 1 1 16 x86",
        };
        assert_eq!(tokens(&got), tokens(expected));
    }
}

#[test]
fn raw_c_compiles_and_preserves_diagnostics_and_pack() {
    let src = "#define RET(x) return (x)\n#if 1\nint main(void) { RET(42); }\n#endif\n";
    let pp = preprocess(src, "main.c", &PreprocessOptions::default()).unwrap();
    let module = keel_c::compile(&pp, "main.c", Default::default()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pp = preprocess(
        "#line 30 \"original.c\"\nint x = missing;",
        "main.c",
        &PreprocessOptions::default(),
    )
    .unwrap();
    let error = keel_c::compile(&pp, "main.c", Default::default()).unwrap_err();
    assert_eq!((error.file.as_str(), error.line), ("original.c", 30));
    let pp = preprocess("#define PACK _Pragma(\"pack(push, 1)\")\nPACK\nstruct S { char c; int n; };\n#pragma pack(pop)\n_Static_assert(sizeof(struct S) == 5, \"packed\");\n", "pack.c", &PreprocessOptions::default()).unwrap();
    keel_c::compile(&pp, "pack.c", Default::default()).unwrap();
}

#[test]
fn standard_rescan_example() {
    parity(
        r#"
#define x 3
#define f(a) f(x * (a))
#undef x
#define x 2
#define g f
#define z z[0]
#define h g(~
#define m(a) a(w)
#define w 0,1
#define t(a) a
f(y+1) + f(f(z)) % t(t(g)(0) + t)(1);
g(x+(3,4)-w) | h 5) & m(f)^m(m);
"#,
    );
}

#[test]
fn builtin_override_and_command_line_order() {
    parity(
        "#define __LINE__ 88\n__LINE__\n#undef __LINE__\n#if defined(__LINE__)\nwrong\n#endif\n__LINE__\n#define __COUNTER__ 4\n__COUNTER__\n",
    );
    let opts = PreprocessOptions {
        defines: vec![
            ("A".into(), Some("1".into())),
            ("A".into(), None),
            ("A".into(), Some("42".into())),
            ("F(x)".into(), Some("x+x".into())),
        ],
        ..Default::default()
    };
    let got = preprocess("F(A)", "test.c", &opts).unwrap();
    assert_eq!(tokens(&got), tokens("42+42"));
}
