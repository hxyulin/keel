use keel_ir::{InstData, Module};

fn source() -> Module {
    keel_ir::parse(
        r#"
decl @external() noinline
func @helper() alwaysinline section ".text.helper" local {
block0:
 return
}
func @caller() {
block0:
 call @helper()
 call @external()
 return
}
"#,
    )
    .unwrap()
}

fn assert_discarded_contracts(module: &Module) {
    keel_ir::verify_module(module).unwrap();
    assert!(!module.functions.iter().any(|f| f.name == "helper"));
    let helper = module.decls.iter().find(|d| d.name == "helper").unwrap();
    assert!(helper.attrs.section.is_none());
    assert!(helper.attrs.alwaysinline);
    let caller = &module.functions[0];
    // Inlining/removing the call leaves its import in the callee table.
    let imported = caller
        .callees()
        .find(|(_, d)| d.name == "helper")
        .unwrap()
        .1;
    assert_eq!(imported.attrs, helper.attrs);
    let remaining: Vec<_> = caller
        .layout_insts()
        .filter_map(|(_, i)| match caller.inst_data(i) {
            InstData::Call { func, .. } => Some(caller.callee(*func)),
            _ => None,
        })
        .collect();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "external");
    assert!(remaining[0].attrs.noinline);
}

#[test]
fn discard_after_required_inlining_refreshes_retained_imports() {
    let mut module = source();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::required_inline_asm(&mut module).unwrap();
    assert_discarded_contracts(&module);
    // Repeated finalization and later optimization must keep the contract valid.
    keel_opt::required_inline_asm(&mut module).unwrap();
    keel_opt::run_passes(&mut module, &["inline", "simplify-cfg", "dce"]).unwrap();
    assert_discarded_contracts(&module);
}

#[test]
fn discard_before_required_inlining_refreshes_unused_imports() {
    let mut module = source();
    let caller = &mut module.functions[1];
    let call = caller.layout_insts().next().unwrap().1;
    caller.remove_inst(call);
    keel_ir::verify_module(&module).unwrap();
    keel_opt::required_inline_asm(&mut module).unwrap();
    assert_discarded_contracts(&module);
}
