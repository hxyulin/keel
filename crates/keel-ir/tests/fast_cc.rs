use keel_ir::{CallConv, parse, verify_module};

const PRIVATE: &str = "func @narrow(i8) -> i8 cc(fast) local {
block0(v0: i8):
 return v0
}
";

#[test]
fn private_fast_roundtrip_and_direct_call() {
    let source = format!(
        "{PRIVATE}func @entry(i8 uext) -> i8 uext {{\nblock0(v0: i8):\n v1 = call @narrow(v0)\n return v1\n}}\n"
    );
    let mut m = parse(&source).unwrap();
    verify_module(&m).unwrap();
    assert_eq!(m.functions[0].sig.cc, CallConv::Fast);
    let printed = m.to_string();
    assert_eq!(parse(&printed).unwrap().to_string(), printed);
    let callee = m.functions[1].callees().next().unwrap().0;
    m.functions[1].callee_mut(callee).sig.cc = CallConv::C;
    assert!(
        verify_module(&m).is_err(),
        "a C/Fast mismatch must not silently cross the boundary"
    );
}

fn rejects(source: &str, diagnostic: &str) {
    let m = parse(source).unwrap();
    let errors = verify_module(&m).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains(diagnostic)),
        "{errors:?}"
    );
}

#[test]
fn fast_requires_contained_fixed_direct_calls() {
    rejects("decl @outside() cc(fast)", "local definition");
    rejects(&PRIVATE.replace(" local", ""), "local definition");
    rejects(
        "func @v(...) cc(fast) local {\nblock0:\n return\n}",
        "fixed",
    );
    rejects(
        "func @v() cc(fast) returns_twice local {\nblock0:\n return\n}",
        "fixed",
    );
    rejects(
        &format!(
            "{PRIVATE}func @address() -> ptr {{\nblock0:\n v0 = symbol_addr @narrow\nreturn v0\n}}"
        ),
        "address taken",
    );
    rejects(
        &format!("{PRIVATE}data @escaped = align 8 {{ ptr @narrow }}"),
        "address taken",
    );
    rejects(
        "func @indirect(ptr, i8) -> i8 uext {\nblock0(v0: ptr, v1: i8):\n v2 = call_indirect v0(v1) : (i8) -> i8 cc(fast)\nreturn v2\n}",
        "indirect calls",
    );
}
