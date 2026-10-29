use keel_ir::{InstData, Target};

#[test]
fn required_inline_discard_preserves_remaining_c_calls_and_placement() {
    let source = r#"
__attribute__((noinline)) int external(int);
static inline __attribute__((always_inline, section(".text.common")))
int common(int x) { asm volatile("" ::: "memory"); return external(x + 1); }
static inline __attribute__((always_inline, used, section(".text.kept")))
int kept(int x) { return x + 2; }
int lock(int x) { return common(x); }
int ww_lock(int x) { return common(x + 3); }
"#;
    let mut module = keel_c::compile(
        source,
        "required-inline-discard.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(!module.functions.iter().any(|f| f.name == "common"));
    let common = module.decls.iter().find(|d| d.name == "common").unwrap();
    assert!(common.attrs.section.is_none());
    let kept = module.functions.iter().find(|f| f.name == "kept").unwrap();
    assert!(kept.local && kept.attrs.used);
    assert_eq!(kept.attrs.section.unwrap().as_str(), ".text.kept");
    for name in ["lock", "ww_lock"] {
        let caller = module.functions.iter().find(|f| f.name == name).unwrap();
        let imported = caller
            .callees()
            .find(|(_, d)| d.name == "common")
            .unwrap()
            .1;
        assert_eq!(imported.attrs, common.attrs);
        let calls: Vec<_> = caller
            .layout_insts()
            .filter_map(|(_, i)| match caller.inst_data(i) {
                InstData::Call { func, .. } => Some(caller.callee(*func)),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 1, "{name}");
        assert_eq!(calls[0].name, "external");
        assert!(calls[0].attrs.noinline);
    }
    keel_opt::run_passes(&mut module, &["inline", "simplify-cfg", "dce"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
}
