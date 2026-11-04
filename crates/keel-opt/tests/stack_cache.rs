use keel_ir::InstData as D;
const FIXTURE: &str = include_str!("stack_cache/fields.keel");
#[test]
fn promotion_requires_disjoint_reads_and_keeps_initializer_values() {
    // The initializer's returned pointer may alias the slot. Entry pointer
    // parameters, in contrast, predate this activation's fixed objects.
    for external in [false, true] {
        let text = FIXTURE
            .replace("(i64, i64) -> i64", "(i64, i64, ptr) -> i64")
            .replace("v1: i64):", "v1: i64, v30: ptr):");
        let text = if external {
            text.replace(" v3\n", " v30\n")
        } else {
            text
        };
        let mut m = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        keel_opt::run_passes(&mut m, &["stack-promote", "phi-simplify", "dce"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for f in &m.functions {
            let stores = f
                .layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), D::Store { .. }))
                .count();
            assert_eq!(stores, usize::from(!external), "{m}");
            assert_eq!(
                f.layout_insts()
                    .filter(|&(_, i)| matches!(f.inst_data(i), D::Call { .. }))
                    .count(),
                1
            );
            if external {
                let entry = f.entry_block().unwrap();
                assert!(f.layout_insts().any(|(b, i)| b == entry
                    && matches!(f.inst_data(i), D::Load { flags, .. } if flags.notrap)));
                assert!(!f.layout_insts().any(|(b, i)| b != entry
                    && matches!(f.inst_data(i), D::Load { flags, .. } if flags.notrap)));
            }
        }
        if !external {
            assert_eq!(before, m.to_string());
        }
    }
}

#[test]
fn captured_pointer_fields_use_the_target_pointer_width() {
    let source = "decl @init(ptr,ptr)
func @p(i64,ptr) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: i64,v1: ptr):
 v2 = stack_addr ss0
 call @init(v2,v1)
 v3 = iconst.i64 0
 jump block1(v3)
block1(v4: i64):
 v5 = icmp.ult v4,v0
 brif v5,block2,block3
block2:
 v6 = load.ptr v2
 store v1,v2
 v7 = iconst.i64 1
 v8 = iadd v4,v7
 jump block1(v8)
block3:
 v9 = load.ptr v2
 return v9
}";
    let mut m = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut m, &["stack-promote", "phi-simplify", "dce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let entry = f.entry_block().unwrap();
    assert!(
        f.layout_insts().any(|(b, i)| b == entry
            && matches!(
                f.inst_data(i),
                D::Load {
                    ty: keel_ir::Type::Ptr,
                    ..
                }
            )),
        "{m}"
    );
    assert!(
        !f.layout_insts()
            .any(|(b, i)| b != entry && matches!(f.inst_data(i), D::Load { .. })),
        "{m}"
    );
}
