//! A lifetime start writes its own slot: facts on the slot do not cross it,
//! and facts on other slots do. An end leaves facts alone, since a load
//! after it is undefined.
use keel_ir::{InstData, Module};

fn forwarded(source: &str) -> (Module, usize) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let loads = f
        .layout_insts()
        .filter(|&(_, i)| matches!(f.inst_data(i), InstData::Load { .. }))
        .count();
    (m, loads)
}

fn function(body: &str) -> String {
    format!(
        "func @f(i8) -> i64 {{
 ss0 = stack_slot 8, align 8
 ss1 = stack_slot 8, align 8
block0(v9: i8):
 v0 = stack_addr ss0
 v1 = stack_addr ss1
 v2 = iconst.i64 5
 {body}
}}"
    )
}

#[test]
fn facts_cross_another_slots_markers_but_not_their_own() {
    for (between, kept) in [
        (
            "intrinsic lifetime_start(v1)\n intrinsic lifetime_end(v1)",
            0,
        ),
        (
            "intrinsic lifetime_end(v0)\n intrinsic lifetime_start(v0)",
            1,
        ),
    ] {
        let source = function(&format!(
            "intrinsic lifetime_start(v0)
 store v2, v0
 {between}
 v3 = load.i64 v0
 intrinsic lifetime_end(v0)
 return v3"
        ));
        let (m, loads) = forwarded(&source);
        assert_eq!(loads, kept, "{m}");
    }
}

#[test]
fn cross_block_facts_allow_a_start_before_the_store_and_an_end_after_the_load() {
    // The start precedes the store in its block and the end follows the
    // load in its block, so neither can run between the two.
    let allowed = function(
        "intrinsic lifetime_start(v0)
 store v2, v0
 brif v9, block1, block2
block1:
 jump block2
block2:
 v3 = load.i64 v0
 intrinsic lifetime_end(v0)
 return v3",
    );
    assert_eq!(forwarded(&allowed).1, 0);
    // A start between them makes the bytes indeterminate again.
    let restarted = function(
        "intrinsic lifetime_start(v0)
 store v2, v0
 jump block1
block1:
 intrinsic lifetime_start(v0)
 v3 = load.i64 v0
 intrinsic lifetime_end(v0)
 return v3",
    );
    assert_eq!(forwarded(&restarted).1, 1);
    // So does an end that a loop carries back to the load.
    let looped = function(
        "intrinsic lifetime_start(v0)
 store v2, v0
 jump block1
block1:
 v3 = load.i64 v0
 brif v9, block2, block3
block2:
 intrinsic lifetime_end(v0)
 intrinsic lifetime_start(v0)
 jump block1
block3:
 return v3",
    );
    assert_eq!(forwarded(&looped).1, 1);
}

#[test]
fn starts_that_dominate_the_store_and_ends_elsewhere_keep_facts() {
    // The inliner's shape: a start in a block that dominates the store,
    // and the end in a later block that the load does not reach.
    let inlined = function(
        "intrinsic lifetime_start(v0)
 jump block1
block1:
 store v2, v0
 brif v9, block2, block3
block2:
 v3 = load.i64 v0
 jump block4(v3)
block3:
 jump block4(v2)
block4(v4: i64):
 intrinsic lifetime_end(v0)
 return v4",
    );
    assert_eq!(forwarded(&inlined).1, 0);
    // A start in a block between the store and the load does not dominate
    // the store's block, so the fact stops there.
    let between = function(
        "jump block1
block1:
 store v2, v0
 jump block2
block2:
 intrinsic lifetime_start(v0)
 jump block3
block3:
 v3 = load.i64 v0
 return v3",
    );
    assert_eq!(forwarded(&between).1, 1);
}
