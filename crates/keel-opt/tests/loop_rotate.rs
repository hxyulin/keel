fn source() -> &'static str {
    "func @scan(ptr,i64) -> i64 {
block0(v0: ptr,v1: i64):
 jump block1(v1)
block1(v2: i64):
 v3 = iconst.i64 0
 v4 = icmp.ne v2,v3
 brif v4,block2,block4
block2:
 v5 = load.i64 v0
 v6 = icmp.ne v5,v3
 brif v6,block3,block4
block3:
 v7 = iconst.i64 1
 v8 = isub v2,v7
 store v8,v0
 jump block1(v8)
block4:
 return v2
}"
}
#[test]
fn guarded_single_exit_rotation_verifies_and_is_idempotent() {
    let mut m = keel_ir::parse(source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let pass = keel_opt::find_pass("loop-rotate").unwrap();
    assert!(pass.run(&mut m.functions[0]));
    keel_ir::verify_module(&m).unwrap();
    let text = m.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    assert!(!pass.run(&mut m.functions[0]));
    assert_eq!(text, m.to_string());
    assert!(keel_opt::DEFAULT_PIPELINE.contains(&"loop-rotate"));
}
#[test]
fn rejects_debug_calls_header_loads_and_multiple_exits() {
    for src in [
        source().replace(" v3 = iconst.i64 0", " v3 = load.i64 v0"),
        format!(
            "decl @effect()\n{}",
            source().replace(" v5 =", " call @effect()\n v5 =")
        ),
        source()
            .replace("brif v6,block3,block4", "brif v6,block3,block5")
            .replace("block4:", "block5:\n return v2\nblock4:"),
    ] {
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(
            !keel_opt::find_pass("loop-rotate")
                .unwrap()
                .run(&mut m.functions[0])
        );
        assert_eq!(before, m.to_string());
    }
    let mut m = keel_ir::parse(source()).unwrap();
    m.functions[0].debug_scopes.push(None);
    assert!(
        !keel_opt::find_pass("loop-rotate")
            .unwrap()
            .run(&mut m.functions[0])
    );
}
#[test]
fn calls_and_locations_outside_the_loop_do_not_block_rotation() {
    let outside = [
        format!(
            "decl @effect()\n{}",
            source()
                .replace(" jump block1(v1)", " call @effect()\n jump block1(v1)")
                .replace(" return v2", " call @effect()\n return v2")
        ),
        format!(
            "file f0 = \"t.c\"\n{}",
            source().replace(" return v2", " return v2 !f0:2:1")
        ),
    ];
    for src in &outside {
        let mut m = keel_ir::parse(src).unwrap();
        assert!(
            keel_opt::find_pass("loop-rotate")
                .unwrap()
                .run(&mut m.functions[0]),
            "{src}"
        );
        keel_ir::verify_module(&m).unwrap();
    }
    // A location inside the loop still keeps it as it is.
    let src = format!(
        "file f0 = \"t.c\"\n{}",
        source().replace(" v8 = isub v2,v7", " v8 = isub v2,v7 !f0:1:1")
    );
    let mut m = keel_ir::parse(&src).unwrap();
    let before = m.to_string();
    assert!(
        !keel_opt::find_pass("loop-rotate")
            .unwrap()
            .run(&mut m.functions[0])
    );
    assert_eq!(before, m.to_string());
}
