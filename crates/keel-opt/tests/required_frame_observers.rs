use keel_ir::{InstData, Intrinsic, Module};

fn source(body: &str) -> Module {
    keel_ir::parse(&format!(
        "func @observer() -> ptr cc(c) alwaysinline local {{\nblock0:\n{body}\n}}\n\
         func @caller() -> ptr cc(c) {{\nblock0:\n v0 = call @observer()\n return v0\n}}"
    ))
    .unwrap()
}

fn calls_observer(module: &Module) -> bool {
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .unwrap();
    caller.layout_insts().any(|(_, inst)| {
        matches!(caller.inst_data(inst), InstData::Call { func, .. } if caller.callee(*func).name == "observer")
    })
}

#[test]
fn c_required_inlining_rebinds_observers_without_relaxing_ir_inlining() {
    for (name, intrinsic) in [
        ("frame_address", Intrinsic::FrameAddress),
        ("stack_save", Intrinsic::StackSave),
    ] {
        let original = source(&format!(" v0 = intrinsic {name}()\n return v0"));
        for pass in ["inline", "inline-noninterposable"] {
            let mut module = original.clone();
            keel_opt::run_passes(&mut module, &[pass]).unwrap();
            assert!(
                calls_observer(&module),
                "{pass} must preserve {name} frames"
            );
        }
        let mut module = original;
        keel_opt::required_inline_asm(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(
            !calls_observer(&module),
            "C required inline must expand {name}"
        );
        let caller = module
            .functions
            .iter()
            .find(|f| f.name == "caller")
            .unwrap();
        assert!(caller.layout_insts().any(|(_, inst)| {
            matches!(caller.inst_data(inst), InstData::Intrinsic { id, .. } if *id == intrinsic)
        }));
    }
}

#[test]
fn c_required_inlining_keeps_stack_mutation_restrictions() {
    for body in [
        " v0 = iconst.i64 32\n v1 = intrinsic stack_alloc(v0)\n return v1",
        " v0 = intrinsic stack_save()\n intrinsic stack_restore(v0)\n return v0",
    ] {
        let mut module = source(body);
        let error = keel_opt::required_inline_asm(&mut module).unwrap_err();
        assert!(error.contains("unsupported_callee_intrinsic"), "{error}");
    }
}
