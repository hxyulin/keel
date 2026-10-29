use keel_ir::{parse, verify_module};

#[test]
fn dollar_suffixed_platform_symbols_roundtrip() {
    let source = "\
decl @realpath$DARWIN_EXTSN(ptr, ptr) -> ptr
data @$data.local = local const align 1 { b\"x\\0\" }
func @helper$local(ptr, ptr) -> ptr {
block0(v0: ptr, v1: ptr):
 v2 = call @realpath$DARWIN_EXTSN(v0, v1)
 return v2
}
";
    let module = parse(source).unwrap();
    verify_module(&module).unwrap();
    let printed = module.to_string();
    let roundtrip = parse(&printed).unwrap();
    verify_module(&roundtrip).unwrap();
    assert_eq!(roundtrip.decls[0].name, "realpath$DARWIN_EXTSN");
    assert_eq!(roundtrip.functions[0].name, "helper$local");
    assert_eq!(roundtrip.to_string(), printed);
}

#[test]
fn dollar_does_not_extend_local_identifier_syntax() {
    assert!(parse("func @f() {\nblock$0:\n return\n}\n").is_err());
    assert!(parse("func @f() {\nblock0:\n v$0 = iconst.i64 0\n return\n}\n").is_err());
}
