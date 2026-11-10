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
        // Clang 23 pads with spaces where older releases used tabs. Keep
        // the token kind and quoted spelling, excluding flags and locations.
        .map(|l| {
            let prefix = l.rsplit_once("Loc=<").unwrap().0;
            &prefix[..=prefix.rfind('\'').unwrap()]
        })
        .filter(|l| {
            l.split_once(" '").is_some_and(|(kind, _)| {
                !kind.is_empty() && kind.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_')
            }) && !l.starts_with("comment ")
        })
        .filter(|l| {
            !l.starts_with("unknown '")
                || !l
                    .trim_start_matches("unknown '")
                    .trim_end_matches('\'')
                    .trim()
                    .is_empty()
        })
        .map(|l| {
            l.to_string()
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
#define VSTR(x, ...) #__VA_ARGS__
VSTR(0, a, b,   c) VSTR(0, a , b) VSTR(0,a,b) VSTR(0, (a,b) ,c)
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
#if 0x7fffffffffffffff + 1 < 0 && (-0x7fffffffffffffff - 1) / -1 < 0 && (1 << 63) < 0
wraps
#endif
#if (-0x7fffffffffffffff - 1) % -1 == 0 && -7 / 2 == -3 && -7 % 2 == -1 && 7u / 2 == 3
division
#endif
#if 0xffffffffffffffff == -1 && -1 > 0u && (1u << 63 >> 63) == 1 && (-1 >> 63) == -1
unsigned_bits
#endif
#if (0 && (1 << 64)) || (1 || 1 >> -1) && (0 ? 1 << 64 : 3) == 3
dead_shifts
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
fn short_line_gaps_use_newlines_instead_of_markers() {
    let src = "int a;\n\nint b;\n#define F(x) x\nint c = F(1\n);\n\n\n\n\n\n\n\n\nint d;\n\n\n\n\n\n\n\n\n\nint e;\n";
    let got = preprocess(src, "gap.c", &PreprocessOptions::default()).unwrap();
    let body = got.lines().skip_while(|l| !l.starts_with("int a"));
    assert_eq!(
        body.collect::<Vec<_>>().join("\n"),
        "int a ; \n\nint b ; \n\nint c = 1 \n; \n\n\n\n\n\n\n\n\nint d ; \n# 25 \"gap.c\"\nint e ; "
    );
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
fn expansion_limit_counts_only_macro_tokens() {
    // An `xxd -i` style array: 1.2M tokens between directives, no macros.
    let mut src = String::from("unsigned char blob[] = {\n");
    for i in 0..600_000 {
        src.push_str(&format!("0x{:02x},", i % 256));
        if i % 12 == 11 {
            src.push('\n');
        }
    }
    src.push_str("};\n");
    let out = preprocess(&src, "blob.c", &PreprocessOptions::default()).unwrap();
    assert_eq!(out.matches("0x").count(), 600_000);
    // Doubling 21 times gives 2M tokens, which still exceeds the limit.
    let mut runaway = String::from("#define A0 x\n");
    for i in 1..=21 {
        runaway.push_str(&format!("#define A{i} A{0} A{0}\n", i - 1));
    }
    runaway.push_str("A21\n");
    let error = preprocess(&runaway, "runaway.c", &PreprocessOptions::default()).unwrap_err();
    assert!(error.message.contains("macro expansion limit"), "{error}");
}

#[test]
fn rejects_invalid_input_with_source_location() {
    for (src, message) in [
        ("#if 1\n", "unterminated conditional"),
        ("#else\n", "conditional without"),
        ("#if 1\n#else\n#else\n#endif", "after #else"),
        ("#if 1/0\n#endif", "division by zero"),
        ("#if 1 << 64\n#endif", "invalid shift"),
        ("#if 1 >> -1\n#endif", "invalid shift"),
        ("#if 1u << 0xffffffffffffffff\n#endif", "invalid shift"),
        ("#if 1 % 0\n#endif", "division by zero"),
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
    for target in keel_ir::Target::NATIVE {
        let opts = PreprocessOptions {
            target,
            ..Default::default()
        };
        let got = preprocess("#ifndef __clang__\n#error not clang\n#endif\n__SIZEOF_POINTER__ __SIZEOF_LONG__ __has_builtin(__builtin_expect) __has_feature(c_atomic) __SIZEOF_LONG_DOUBLE__\n#if defined(__aarch64__)\narm\n#elif defined(__x86_64__)\nx86\n#else\n#error missing target architecture\n#endif\n", "test.c", &opts).unwrap();
        let expected = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            keel_ir::Target::Aarch64AppleDarwin => "8 8 1 1 8 arm",
            keel_ir::Target::X86_64LinuxGnu => "8 8 1 1 16 x86",
            keel_ir::Target::Aarch64LinuxGnu => "8 8 1 1 16 arm",
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

#[test]
fn scanner_searches_preserve_tokens_at_vector_boundaries() {
    let mut source = String::from("#define STR(x) #x\n#define JOIN(a,b) a ## b\n");
    // Exercise short tails and delimiters on either side of 16/32/64-byte
    // boundaries, both quote styles, escapes, UTF-8, splices, and long comments.
    for padding in 0..80 {
        let text = "a".repeat(padding);
        source.push_str(&format!("STR({text} /* {text}\n{text} */ tail)\n"));
        source.push_str(&format!("\"{text}\\\"tail\" '{text}\\\\' // {text}\n"));
        source.push_str(&format!("JOIN(pre\\\nfix,{text})\n"));
    }
    source.push_str("#define α 17\nα\n#line 71 \"virtual.c\"\n__LINE__ __FILE__\n");
    parity(&source);
}

#[test]
fn spliced_diagnostics_keep_physical_line_and_column() {
    for (source, line, column, message) in [
        (" \\\n  \"unfinished", 2, 3, "unterminated literal"),
        ("\\\r\n\\\n   /* unfinished", 3, 4, "unterminated comment"),
        ("\\\n\"tail\\", 2, 1, "unterminated literal"),
    ] {
        let error = preprocess(source, "physical.c", &PreprocessOptions::default()).unwrap_err();
        assert_eq!(
            (error.file.as_str(), error.line, error.col),
            ("physical.c", line, column)
        );
        assert!(error.message.contains(message));
    }
}

#[test]
fn cached_headers_reexpand_with_current_macros_and_file_spelling() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-cached-state");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("state.h"),
        "__FILE__ __INCLUDE_LEVEL__ MODE __COUNTER__\n#line 100 \"virtual.h\"\n__LINE__ MODE\n",
    )
    .unwrap();
    let source = "#define MODE 1\n#include \"state.h\"\n#undef MODE\n#define MODE 2\n#include \"./state.h\"\n";
    let path = dir.join("main.c");
    let opts = PreprocessOptions::default();
    let got = preprocess(source, path.to_str().unwrap(), &opts).unwrap();
    let expected = format!(
        "{:?} 1 1 0 100 1 {:?} 1 2 1 100 2",
        dir.join("state.h").to_str().unwrap(),
        dir.join("./state.h").to_str().unwrap()
    );
    assert_eq!(tokens(&got), tokens(&expected));
    #[cfg(feature = "profile")]
    {
        let (profiled, stats) =
            keel_c::preprocess_with_stats(source, path.to_str().unwrap(), &opts).unwrap();
        assert_eq!(profiled, got);
        assert_eq!(stats.include_requests, 2);
        assert_eq!(stats.file_opens, 1);
        assert_eq!(stats.scanned_files, 2); // root + one physical header
        assert_eq!(stats.source_cache_hits, 1);
        assert_eq!(stats.token_cache_hits, 1);
    }
    // Invocation-local caching must not hide a subsequent file change.
    std::fs::write(dir.join("state.h"), "changed\n").unwrap();
    let fresh = preprocess(source, path.to_str().unwrap(), &opts).unwrap();
    assert_eq!(tokens(&fresh), tokens("changed changed"));
}

#[test]
fn cached_guard_detection_respects_undef_else_and_trailing_tokens() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-cached-guards");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [
        (
            "guard.h",
            "#ifndef GUARD_H\n#define GUARD_H\nVALUE\n#endif\n",
        ),
        (
            "else.h",
            "#ifndef ELSE_H\n#define ELSE_H\nfirst\n#else\nagain\n#endif\n",
        ),
        (
            "tail.h",
            "#ifndef TAIL_H\n#define TAIL_H\ninside\n#endif\noutside\n",
        ),
    ] {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let source = "#define VALUE 1\n#include \"guard.h\"\n#include \"guard.h\"\n#undef GUARD_H\n#undef VALUE\n#define VALUE 2\n#include \"guard.h\"\n#include \"else.h\"\n#include \"else.h\"\n#include \"tail.h\"\n#include \"tail.h\"\n";
    let got = preprocess(
        source,
        dir.join("main.c").to_str().unwrap(),
        &PreprocessOptions::default(),
    )
    .unwrap();
    assert_eq!(
        tokens(&got),
        tokens("1 2 first again inside outside outside")
    );
    #[cfg(feature = "profile")]
    {
        let (_, stats) = keel_c::preprocess_with_stats(
            source,
            dir.join("main.c").to_str().unwrap(),
            &PreprocessOptions::default(),
        )
        .unwrap();
        assert_eq!(stats.file_opens, 3);
        assert_eq!(stats.guard_skips, 1);
        // guard.h is scanned again after #undef. The outer #else rules out a
        // guard in else.h, which is scanned on both inclusions. tail.h's
        // second inclusion processes the kept line after its guard group.
        assert_eq!(stats.scanned_files, 6);
        assert_eq!(stats.token_cache_hits, 1);
    }
}

#[test]
fn released_header_tokens_are_scanned_again_after_guard_undef() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-released-headers");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [
        (
            "guard.h",
            "#ifndef GUARD_H\n#define GUARD_H\nguard VALUE __LINE__\n#endif\n",
        ),
        ("once.h", "#pragma once\nonce VALUE __LINE__\n"),
        // The inner inclusion releases the tokens while the outer one is
        // still processing them.
        (
            "nested.h",
            "#ifndef NESTED_H\n#define NESTED_H\n#ifndef INNER\n#define INNER\nbefore __LINE__\n#undef NESTED_H\n#include \"nested.h\"\nafter __LINE__ __FILE__\n#else\ninner __INCLUDE_LEVEL__\n#endif\n#endif\n",
        ),
    ] {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let source = "#define VALUE 1\n#define HDR \"guard.h\"\n#include HDR\n#if 1\n#include \"guard.h\"\n#endif\n#undef GUARD_H\n#undef VALUE\n#define VALUE 2\n#if 1\n#include HDR\n#endif\n#include \"once.h\"\n#include \"./once.h\"\n#include \"nested.h\"\n#include \"nested.h\"\n#undef NESTED_H\n#undef INNER\n#include \"./nested.h\"\n__LINE__\n";
    let path = dir.join("main.c");
    let path = path.to_str().unwrap();
    let opts = PreprocessOptions::default();
    let got = preprocess(source, path, &opts).unwrap();
    let expected = format!(
        "guard 1 3 guard 2 3 once 2 2 before 5 inner 2 after 8 {:?} before 5 inner 2 after 8 {:?} 20",
        dir.join("nested.h").to_str().unwrap(),
        dir.join("./nested.h").to_str().unwrap()
    );
    assert_eq!(tokens(&got), tokens(&expected));
    let mut cache = keel_c::PreprocessCache::new();
    for _ in 0..2 {
        assert_eq!(cache.preprocess(source, path, &opts).unwrap(), got);
    }
    #[cfg(feature = "profile")]
    {
        let (profiled, stats) = keel_c::preprocess_with_stats(source, path, &opts).unwrap();
        assert_eq!(profiled, got);
        // The root, three headers, guard.h again, and the inner inclusion of
        // nested.h, which happens before its first inclusion ends, twice.
        assert_eq!(stats.scanned_files, 8);
        assert_eq!(stats.guard_skips, 2);
        assert_eq!(stats.once_skips, 1);
    }
}

#[cfg(unix)]
#[test]
fn cached_aliases_preserve_conditional_pragma_once() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-cached-aliases");
    std::fs::create_dir_all(&dir).unwrap();
    let header = dir.join("once.h");
    std::fs::write(&header, "#if MARK\n#pragma once\n#endif\nVALUE\n").unwrap();
    let alias = dir.join("alias.h");
    let _ = std::fs::remove_file(&alias);
    std::os::unix::fs::symlink("once.h", &alias).unwrap();
    let source = "#define MARK 0\n#define VALUE 7\n#include \"once.h\"\n#undef MARK\n#define MARK 1\n#undef VALUE\n#define VALUE 8\n#include \"alias.h\"\n#undef VALUE\n#define VALUE 9\n#include \"once.h\"\n";
    let got = preprocess(
        source,
        dir.join("main.c").to_str().unwrap(),
        &PreprocessOptions::default(),
    )
    .unwrap();
    assert_eq!(tokens(&got), tokens("7 8"));
    #[cfg(feature = "profile")]
    {
        let (_, stats) = keel_c::preprocess_with_stats(
            source,
            dir.join("main.c").to_str().unwrap(),
            &PreprocessOptions::default(),
        )
        .unwrap();
        assert_eq!(stats.file_opens, 1);
        assert_eq!(stats.once_skips, 1);
    }
}

#[cfg(unix)]
#[test]
fn cached_include_next_keeps_each_search_origin() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-cached-next");
    for name in ["a", "b", "c"] {
        std::fs::create_dir_all(dir.join(name)).unwrap();
    }
    std::fs::write(dir.join("a/chain.h"), "head __INCLUDE_LEVEL__ __COUNTER__\n#if __has_include_next(<chain.h>)\n#include_next <chain.h>\n#endif\n").unwrap();
    let alias = dir.join("b/chain.h");
    let _ = std::fs::remove_file(&alias);
    std::os::unix::fs::symlink("../a/chain.h", alias).unwrap();
    std::fs::write(dir.join("c/chain.h"), "tail __INCLUDE_LEVEL__\n").unwrap();
    let opts = PreprocessOptions {
        include_dirs: ["a", "b", "c"].map(|n| dir.join(n)).into(),
        ..Default::default()
    };
    let source = "#include <chain.h>\n";
    let got = preprocess(source, dir.join("main.c").to_str().unwrap(), &opts).unwrap();
    assert_eq!(tokens(&got), tokens("head 1 0 head 2 1 tail 3"));
    #[cfg(feature = "profile")]
    {
        let (_, stats) =
            keel_c::preprocess_with_stats(source, dir.join("main.c").to_str().unwrap(), &opts)
                .unwrap();
        assert_eq!(stats.file_opens, 2);
        // The alias is scanned again: the first inclusion has not ended.
        assert_eq!(stats.scanned_files, 4);
    }
}

#[test]
fn top_level_text_is_independent_of_same_named_disk_header() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-cached-root");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.c");
    std::fs::write(&path, "from_disk\n").unwrap();
    let got = preprocess(
        "#include \"main.c\"\n",
        path.to_str().unwrap(),
        &PreprocessOptions::default(),
    )
    .unwrap();
    assert_eq!(tokens(&got), tokens("from_disk"));
}

#[test]
fn predefined_header_guard_keeps_nested_conditional_diagnostics() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-guard-diagnostics");
    std::fs::create_dir_all(&dir).unwrap();
    let opts = PreprocessOptions {
        defines: vec![("GUARDED".into(), Some("1".into()))],
        ..Default::default()
    };
    for invalid in ["#else\n#else\n", "#else\n#elif 1\n"] {
        std::fs::write(
            dir.join("guard.h"),
            format!("#ifndef GUARDED\n#define GUARDED\n#if 1\n{invalid}#endif\n#endif\n"),
        )
        .unwrap();
        let error = preprocess(
            "#include \"guard.h\"\n",
            dir.join("main.c").to_str().unwrap(),
            &opts,
        )
        .unwrap_err();
        assert!(error.message.contains("after #else"), "{error}");
    }
    let deep = format!(
        "#ifndef GUARDED\n#define GUARDED\n{}{}#endif\n",
        "#if 0\n".repeat(40),
        "#endif\n".repeat(40)
    );
    std::fs::write(dir.join("guard.h"), deep).unwrap();
    assert!(
        preprocess(
            "#include \"guard.h\"\n",
            dir.join("main.c").to_str().unwrap(),
            &opts
        )
        .is_ok()
    );
}

#[test]
fn expansion_prefix_replacements_keep_suffix_arguments_and_token_order() {
    let mut src = String::from(
        "#define ID(x) x\n#define F(a,b) a + b\n#define PREFIX F(1,\n#define ERASE(x)\n#define CAT(a,b) a ## b\n#define NAME F\n#define STR(x) #x\n",
    );
    for _ in 0..128 {
        src.push_str(
            "before PREFIX ID(2)) after ID(NAME)(3,4) CAT(N,AME)(5,6) ERASE((7,8)) STR(a b)\n",
        );
    }
    parity(&src);
}

#[test]
fn header_resolution_cache_keeps_quoted_callers_and_search_kinds_separate() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-header-resolution");
    for (name, value) in [("a", "10"), ("b", "20"), ("search", "30")] {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(dir.join(name).join("value.h"), value).unwrap();
    }
    for name in ["a", "b"] {
        std::fs::write(
            dir.join(name).join("entry.h"),
            "#include \"value.h\"\n#include <value.h>\n__has_include(\"only.h\")\n",
        )
        .unwrap();
    }
    std::fs::write(dir.join("a/only.h"), "").unwrap();
    let _ = std::fs::remove_file(dir.join("b/only.h"));
    let opts = PreprocessOptions {
        include_dirs: vec![dir.join("search")],
        ..Default::default()
    };
    let source = "#include \"a/entry.h\"\n#include \"b/entry.h\"\n#include \"a/entry.h\"\n#include \"b/entry.h\"\n";
    let path = dir.join("main.c");
    let got = preprocess(source, path.to_str().unwrap(), &opts).unwrap();
    assert_eq!(tokens(&got), tokens("10 30 1 20 30 0 10 30 1 20 30 0"));
    // Negative query results also expire at the end of the invocation.
    std::fs::write(dir.join("b/only.h"), "").unwrap();
    let fresh = preprocess(source, path.to_str().unwrap(), &opts).unwrap();
    assert_eq!(tokens(&fresh), tokens("10 30 1 20 30 1 10 30 1 20 30 1"));
}

#[cfg(unix)]
#[test]
fn directory_canonicalization_preserves_symlink_dotdot_and_spelling() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-directory-aliases");
    std::fs::create_dir_all(dir.join("real/nested")).unwrap();
    std::fs::write(dir.join("real/once.h"), "#pragma once\nint real;\n").unwrap();
    std::fs::write(dir.join("once.h"), "#pragma once\nint other;\n").unwrap();
    std::fs::write(dir.join("real/name.h"), "__FILE__ __LINE__\n").unwrap();
    let alias = dir.join("alias");
    let _ = std::fs::remove_file(&alias);
    std::os::unix::fs::symlink("real/nested", &alias).unwrap();
    let file_alias = dir.join("file-alias.h");
    let _ = std::fs::remove_file(&file_alias);
    std::os::unix::fs::symlink("real/once.h", &file_alias).unwrap();
    let src = "#include \"alias/../once.h\"\n#include \"real/once.h\"\n#include \"file-alias.h\"\n#include \"once.h\"\n#include \"real/name.h\"\n#include \"alias/../name.h\"\n";
    let got = preprocess(
        src,
        dir.join("main.c").to_str().unwrap(),
        &PreprocessOptions::default(),
    )
    .unwrap();
    let expected = format!(
        "int real; int other; \"{}/real/name.h\" 1 \"{}/alias/../name.h\" 1",
        dir.display(),
        dir.display()
    );
    assert_eq!(tokens(&got), tokens(&expected));
    #[cfg(feature = "profile")]
    {
        let (_, stats) = keel_c::preprocess_with_stats(
            src,
            dir.join("main.c").to_str().unwrap(),
            &PreprocessOptions::default(),
        )
        .unwrap();
        assert_eq!(stats.file_opens, 3);
        assert_eq!(stats.once_skips, 2);
        assert_eq!(stats.token_cache_hits, 1);
    }
}

// Plain preprocessing skips inactive header lines without tokens; the batch
// cache keeps every header token, so it scans them all.
#[test]
fn skipped_inactive_lines_match_scanned_ones() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pp-skip-inactive");
    std::fs::create_dir_all(&dir).unwrap();
    let valid = "#ifndef SKIP_H\n#define SKIP_H\n#if 0\nit's /* don't */ \"a 'b\" 'c' // e's\nx /* spans\n#endif inside a comment */ still skipped\n#define BAD /* spans\n*/ #if 1\nnested\n# else\n#endif\ny /* one */ # define Z\n## not a directive\n%:%: nor this\n#define S \"#endif\"\n#  include <missing.h>\n#\n# 7 \"line.c\"\n#elif 1\n%:if 0\ntaken_not\n%:else\ntaken __LINE__\n%:endif\n#endif\n#ifdef UNDEFINED\n#if 1\n#else\n#endif\na /*\n*/ #else\nb __LINE__\n#endif\nafter __LINE__\n#endif\n";
    let errors = [
        "#ifndef E\n#define E\n#if 0\nit's\n#endif\n#endif\n",
        "#ifndef E\n#define E\n#if 0\nx \"open\n#endif\n#endif\n",
        "#ifndef E\n#define E\n#if 0\n/* never closed\n#endif\n#endif\n",
        "#ifndef E\n#define E\n#if 0\n#if 1\n#else\n#else\n#endif\n#endif\n#endif\n",
        "#ifndef E\n#define E\n#ifdef E\n#if 0\n#endif\n",
    ];
    let opts = PreprocessOptions::default();
    for (n, text) in std::iter::once(valid).chain(errors).enumerate() {
        let header = dir.join(format!("skip{n}.h"));
        std::fs::write(&header, text).unwrap();
        let src = format!("#include \"skip{n}.h\"\n");
        let path = dir.join(format!("skip{n}.c"));
        let path = path.to_str().unwrap();
        let skipped = preprocess(&src, path, &opts).map_err(|e| e.to_string());
        let scanned = keel_c::PreprocessCache::new()
            .preprocess(&src, path, &opts)
            .map_err(|e| e.to_string());
        assert_eq!(skipped, scanned, "{text:?}");
        assert_eq!(skipped.is_ok(), n == 0, "{skipped:?}");
    }
    let got = preprocess(
        "#include \"skip0.h\"\n",
        dir.join("main.c").to_str().unwrap(),
        &opts,
    )
    .unwrap();
    let got: Vec<_> = (got.lines())
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .collect();
    assert_eq!(got, ["taken", "23", "b", "32", "after", "34"]);
}

#[test]
fn batch_cache_rebinds_alias_locations_and_does_not_retain_once_or_macros() {
    let root = std::env::temp_dir().join(format!("keel-pp-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let header = root.join("shared.h");
    std::fs::write(&header, "#pragma once\n#define FROM_HEADER 7\n__FILE__ __BASE_FILE__ __FILE_NAME__ __LINE__ __INCLUDE_LEVEL__ __COUNTER__\n#line 72 \"virtual.h\"\n__FILE__ __LINE__\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&header, root.join("alias.h")).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(&header, root.join("alias.h")).unwrap();
    let opts = PreprocessOptions {
        include_dirs: vec![root.clone()],
        ..Default::default()
    };
    let mut cache = keel_c::PreprocessCache::new();
    for name in ["shared.h", "alias.h"] {
        let src = format!(
            "#ifdef FROM_HEADER\n#error leaked macro\n#endif\n#include <{name}>\n#include <{name}>\nFROM_HEADER __COUNTER__\n"
        );
        let path = root
            .join(format!("{name}.c"))
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            cache.preprocess(&src, &path, &opts).unwrap(),
            preprocess(&src, &path, &opts).unwrap()
        );
    }
    #[cfg(unix)]
    assert_eq!(cache.lexical_hits(), 1);
    // Invalid lexical content must not return stale tokens from the old entry.
    std::fs::write(&header, "/* unterminated").unwrap();
    for name in ["shared.h", "alias.h"] {
        let src = format!("#include <{name}>\n");
        let plain = preprocess(&src, "bad.c", &opts).unwrap_err();
        let reused = cache.preprocess(&src, "bad.c", &opts).unwrap_err();
        assert_eq!(plain.to_string(), reused.to_string());
        assert!(reused.file.ends_with(name));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn batch_cache_keeps_target_sysroot_and_missing_header_state_private() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("batch-cache-environment");
    for (name, value) in [("first", 11), ("second", 29)] {
        let include = root.join(name).join("usr/include");
        std::fs::create_dir_all(&include).unwrap();
        std::fs::write(include.join("value.h"), format!("#define VALUE {value}\n")).unwrap();
    }
    let mut cache = keel_c::PreprocessCache::new();
    let source =
        "#include <stdint.h>\n#include <value.h>\nVALUE __SIZEOF_LONG__ __APPLE__ __linux__\n";
    for (target, sdk, value) in [
        (keel_ir::Target::Aarch64AppleDarwin, "first", "11"),
        (keel_ir::Target::X86_64LinuxGnu, "second", "29"),
        (keel_ir::Target::Aarch64AppleDarwin, "first", "11"),
    ] {
        let options = PreprocessOptions {
            target,
            sysroot: Some(root.join(sdk)),
            ..Default::default()
        };
        let plain = preprocess(source, "caller.c", &options).unwrap();
        assert_eq!(
            cache.preprocess(source, "caller.c", &options).unwrap(),
            plain
        );
        assert!(plain.contains(value));
        let header = root.join(sdk).join("usr/include/value.h");
        let text = std::fs::read_to_string(&header).unwrap();
        std::fs::remove_file(&header).unwrap();
        let plain = preprocess(source, "caller.c", &options).unwrap_err();
        let reused = cache.preprocess(source, "caller.c", &options).unwrap_err();
        assert_eq!(reused.to_string(), plain.to_string());
        std::fs::write(&header, text).unwrap();
        assert_eq!(
            cache.preprocess(source, "caller.c", &options).unwrap(),
            preprocess(source, "caller.c", &options).unwrap()
        );
    }
    assert!(cache.lexical_hits() > 0);
    std::fs::remove_dir_all(root).unwrap();
}
