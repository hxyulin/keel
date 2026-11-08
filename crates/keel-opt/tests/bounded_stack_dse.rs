use keel_ir::{InstData, Module};

fn optimize(middle: &str, first: &str, last: &str, escape: &str) -> Module {
    let source = format!(
        "decl @observe(ptr)\nfunc @f(i64) -> i64 {{\n ss0 = stack_slot 16, align 8\nblock0(v0: i64):\n v1 = stack_addr ss0\n {first}\n {middle}\n {last}\n {escape}\n v2 = load.i64 v1\n return v2\n}}"
    );
    let mut m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["bounded-stack-dse"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn stores(m: &Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Store { .. }))
        .count()
}
const STORE: &str = "store notrap v0, v1";
#[test]
fn removes_exact_and_covering_overwrites() {
    assert_eq!(stores(&optimize("", STORE, STORE, "")), 1);
    assert_eq!(
        stores(&optimize(
            "",
            "v3 = iconst.i32 7\n store notrap v3, v1",
            STORE,
            ""
        )),
        1
    );
    assert_eq!(
        stores(&optimize(
            "",
            STORE,
            "v3 = iconst.i32 7\n store notrap v3, v1",
            ""
        )),
        2
    );
}
#[test]
fn retains_faulting_observable_and_escaped_stores() {
    for (first, last, middle, escape) in [
        ("store v0, v1", STORE, "", ""),
        (STORE, "store v0, v1", "", ""),
        ("store volatile notrap v0, v1", STORE, "", ""),
        (STORE, STORE, "v3 = load.i64 v1", ""),
        (STORE, STORE, "fence seq_cst", ""),
        (STORE, STORE, "call @observe(v1)", ""),
        (STORE, STORE, "", "call @observe(v1)"),
        (STORE, STORE, "intrinsic lifetime_start(v1)", ""),
        (STORE, STORE, "intrinsic lifetime_end(v1)", ""),
    ] {
        assert_eq!(
            stores(&optimize(middle, first, last, escape)),
            2,
            "{first}; {middle}; {last}; {escape}"
        );
    }
}
