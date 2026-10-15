const SCAN: &str = "func @ascii(ptr,ptr) -> ptr {
block0(v0: ptr,v1: ptr):
 jump block1(v0)
block1(v2: ptr):
 v3 = icmp.ult v2,v1
 brif v3,block2,block4
block2:
 v4 = load.i8 v2
 v5 = uext.i32 v4
 v6 = iconst.i32 128
 v7 = icmp.slt v5,v6
 brif v7,block3,block4
block3:
 v8 = iconst.i64 1
 v9 = ptradd v2,v8
 jump block1(v9)
block4:
 return v2
}";

fn transform(source: &str) -> String {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["loop-vectorize"]).unwrap();
    let text = module.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    text
}

#[test]
fn bounded_ascii_scans_preserve_pointer_liveouts_and_are_idempotent() {
    for source in [
        SCAN.to_owned(),
        SCAN.replace("icmp.ult v2,v1", "icmp.ugt v1,v2"),
        SCAN.replace("icmp.ult v2,v1", "icmp.uge v2,v1")
            .replace("brif v3,block2,block4", "brif v3,block4,block2"),
        SCAN.replace("icmp.slt v5,v6", "icmp.sge v5,v6")
            .replace("brif v7,block3,block4", "brif v7,block4,block3"),
        SCAN.replace(" v5 = uext.i32 v4\n", "")
            .replace("iconst.i32 128", "iconst.i8 128")
            .replace("icmp.slt v5,v6", "icmp.ult v4,v6"),
        SCAN.replace(" v5 = uext.i32 v4\n", "")
            .replace("iconst.i32 128", "iconst.i8 0")
            .replace("icmp.slt v5,v6", "icmp.sge v4,v6"),
    ] {
        let text = transform(&source);
        assert!(text.contains("load.i64x2"), "{text}");
        assert!(text.contains("load.i8"));
        assert_eq!(text, transform(&text));
    }
}

#[test]
fn scans_reject_unbounded_volatile_effectful_and_inexact_predicates() {
    for source in [
        SCAN.replace("load.i8", "load.i8 volatile"),
        SCAN.replace("icmp.ult v2,v1", "icmp.ule v2,v1"),
        SCAN.replace("iconst.i32 128", "iconst.i32 127"),
        SCAN.replace("uext.i32", "sext.i32"),
        SCAN.replace("iconst.i64 1", "iconst.i64 2"),
        format!(
            "decl @effect()\n{}",
            SCAN.replace(" v4 = load", " call @effect()\n v4 = load")
        ),
        SCAN.replace(" v8 = iconst", " store v4,v2\n v8 = iconst"),
        SCAN.replace("ptr,ptr) -> ptr", "ptr,ptr,ptr) -> ptr")
            .replace("v1: ptr):", "v1: ptr,v10: ptr):")
            .replace("return v2", "store v3,v10\n return v2"),
    ] {
        assert!(!transform(&source).contains("load.i64x2"), "{source}");
    }
}
