use keel_ir::{InstData, MemoryEffects};

#[test]
fn declared_memory_effects_control_forwarding_without_removing_asm() {
    for (constraints, args, effects, loads) in [
        ("~{cc}", "", MemoryEffects::None, 1),
        ("~{rcx}", "", MemoryEffects::None, 1),
        ("m", "v0", MemoryEffects::Read, 1),
        ("mwrite", "v0", MemoryEffects::Write, 2),
        ("mreadwrite", "v0", MemoryEffects::ReadWrite, 2),
        ("~{memory}", "", MemoryEffects::Barrier, 2),
    ] {
        for pass in ["memory-forward", "load-reuse"] {
            let source = format!(
                "func @f(ptr) -> i64 {{
entry(v0:ptr):
 v1 = load.i64 v0
 asm \"\", \"{constraints}\"({args})
 v2 = load.i64 v0
 v3 = iadd v1, v2
 return v3
}}"
            );
            let mut module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let function = &module.functions[0];
            let asm = function
                .layout_insts()
                .find_map(|(_, i)| {
                    let data = function.inst_data(i);
                    matches!(data, InstData::InlineAsm { .. }).then_some(data)
                })
                .unwrap();
            assert_eq!(asm.memory_effects(), effects, "{constraints}");
            keel_opt::run_passes(&mut module, &[pass, "dce"]).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let function = &module.functions[0];
            assert_eq!(
                function
                    .layout_insts()
                    .filter(|(_, i)| matches!(function.inst_data(*i), InstData::Load { .. }))
                    .count(),
                loads,
                "{constraints}: {pass}"
            );
            assert_eq!(
                function
                    .layout_insts()
                    .filter(|(_, i)| matches!(function.inst_data(*i), InstData::InlineAsm { .. }))
                    .count(),
                1,
                "{constraints}: {pass}"
            );
        }
    }
}

#[test]
fn reserved_stack_templates_remain_barriers_without_a_memory_clobber() {
    let mut module = keel_ir::parse(
        r#"
func @f(ptr) -> i64 {
entry(v0:ptr):
 v1 = load.i64 v0
 v2 = intrinsic stack_save()
 v3 = ptrtoint.i64 v2
 asm "", "rsp"(v3)
 v4 = load.i64 v0
 v5 = iadd v1, v4
 return v5
}
"#,
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    let asm = f
        .layout_insts()
        .find_map(|(_, i)| {
            matches!(f.inst_data(i), InstData::InlineAsm { .. }).then_some(f.inst_data(i))
        })
        .unwrap();
    assert_eq!(asm.memory_effects(), MemoryEffects::Barrier);
    keel_opt::run_passes(&mut module, &["memory-forward", "load-reuse"]).unwrap();
    assert_eq!(
        module.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(module.functions[0].inst_data(*i), InstData::Load { .. }))
            .count(),
        2
    );
}
