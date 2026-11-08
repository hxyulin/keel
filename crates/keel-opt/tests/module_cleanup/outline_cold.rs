use keel_ir::{InstData, parse, verify_module};

const SOURCE: &str = "decl @panic(ptr, i64, i64)
func @f(i64, i64) -> i64 cc(fast) local {
 ss0 = stack_slot 64, align 16
block0(v0: i64, v1: i64):
 v2 = iconst.i64 8
 v3 = icmp.ult v0, v1
 brif v3, block1, block2
block1:
 return v0
block2:
 v4 = stack_addr ss0
 store v2, v4
 call @panic(v4, v1, v0)
 trap
}";
fn apply(src: &str) -> keel_ir::Module {
    let mut m = parse(src).unwrap();
    verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["outline-cold"]).unwrap();
    verify_module(&m).unwrap();
    m
}
#[test]
fn outlines_private_stack_records_with_ordered_liveins_and_local_literals() {
    let mut m = apply(SOURCE);
    assert_eq!(m.functions.len(), 2);
    let helper = &m.functions[1];
    assert!(helper.local && helper.attrs.cold && helper.attrs.noinline && helper.attrs.noreturn);
    assert_eq!(helper.sig.params.len(), 2, "literal is cloned, not passed");
    assert!(helper.to_string().contains("iconst.i64 8"));
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::StackAddr { .. }))
    );
    let once = m.to_string();
    keel_opt::run_passes(&mut m, &["outline-cold"]).unwrap();
    assert_eq!(once, m.to_string());
}
#[test]
fn shared_stack_objects_and_frame_observers_prevent_outlining() {
    let shared = SOURCE.replace(
        "block1:\n return v0",
        "block1:\n v5 = stack_addr ss0\n store v0, v5\n return v0",
    );
    assert_eq!(apply(&shared).functions.len(), 1);
    let observed = SOURCE.replace(
        " store v2, v4",
        " intrinsic lifetime_start(v4)\n store v2, v4",
    );
    assert_eq!(apply(&observed).functions.len(), 1);
}
#[test]
fn preserves_ordinary_c_functions_and_existing_names() {
    assert_eq!(
        apply(&SOURCE.replace("cc(fast)", "cc(c)")).functions.len(),
        1
    );
    let collision = format!("{SOURCE}\nfunc @f.cold.block2() {{\nblock0:\n return\n}}");
    assert_eq!(apply(&collision).functions.len(), 2);
    let declared = format!("decl @f.cold.block2()\n{SOURCE}");
    assert_eq!(apply(&declared).functions.len(), 1);
}

#[test]
fn caller_frame_addresses_cannot_escape_through_a_tail_outlined_helper() {
    let src = SOURCE
        .replace(
            "ss0 = stack_slot 64, align 16",
            "ss0 = stack_slot 64, align 16\n ss1 = stack_slot 8, align 8",
        )
        .replace(
            " v2 = iconst.i64 8",
            " v8 = stack_addr ss1\n store v0, v8\n v2 = iconst.i64 8",
        )
        .replace("call @panic(v4, v1, v0)", "call @panic(v8, v1, v0)");
    let m = apply(&src);
    assert_eq!(m.functions.len(), 1);
    assert!(!m.to_string().contains("return_call"));
}
