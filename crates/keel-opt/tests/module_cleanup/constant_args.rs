use keel_ir::{Module, parse, verify_module};

const SOURCE: &str = "decl @missing()\nfunc @helper(i8) noinline local {\nblock0(v0: i8):\n brif v0, block1, block2\nblock1:\n call @missing()\n return\nblock2:\n return\n}\nfunc @entry() {\nblock0:\n v0 = iconst.i8 0\n call @helper(v0)\n return\n}";

fn optimize(source: &str) -> Module {
    let mut module = parse(source).unwrap();
    verify_module(&module).unwrap();
    let passes = ["constant-args", "simplify-cfg", "dce"];
    keel_opt::run_passes(&mut module, &passes).unwrap();
    verify_module(&module).unwrap();
    let once = module.to_string();
    keel_opt::run_passes(&mut module, &passes).unwrap();
    assert_eq!(once, module.to_string());
    module
}

#[test]
fn specializes_contained_noinline_body_without_changing_abi_or_calls() {
    let module = optimize(SOURCE);
    assert!(!module.functions[0].to_string().contains("call @missing"));
    assert_eq!(module.functions[0].sig.params.len(), 1);
    assert!(module.functions[0].attrs.noinline);
    assert!(module.functions[1].to_string().contains("call @helper"));
}

#[test]
fn conflicting_calls_addresses_and_assembly_keep_the_unknown_argument() {
    for extra in [
        "func @other(i8) {\nblock0(v0: i8):\n call @helper(v0)\n return\n}",
        "func @other() {\nblock0:\n v0 = iconst.i8 1\n call @helper(v0)\n return\n}",
        "data @callback = align 8 { ptr @helper }",
        "func @address() -> ptr {\nblock0:\n v0 = symbol_addr @helper\n return v0\n}",
        "module_asm \".macro call_it name\\n call \\\\name\\n.endm\\ncall_it helper\"",
        "module_asm \".macro call_it suffix\\n call hel\\\\suffix\\n.endm\\ncall_it per\"",
    ] {
        let module = optimize(&format!("{SOURCE}\n{extra}"));
        assert!(
            module.functions[0].to_string().contains("call @missing"),
            "{extra}"
        );
    }
    for attr in ["used", "retain", "weak"] {
        let source = if attr == "weak" {
            SOURCE.replace("noinline local", "noinline weak")
        } else {
            SOURCE.replace("noinline local", &format!("noinline {attr} local"))
        };
        assert!(
            optimize(&source).functions[0]
                .to_string()
                .contains("call @missing")
        );
    }
}

#[test]
fn ordinary_macro_substitution_does_not_keep_unrelated_bodies_or_arguments() {
    let assembly = "module_asm \".macro encode reg:req type:req\\n.ifc \\\\reg, %rax\\n.long \\\\type\\n.endif\\n.endm\\nencode reg=%rax,type=17\\n.long 17 /* numeric data */\"";
    let mut module = optimize(&format!(
        "{SOURCE}\n{assembly}\nfunc @unused() local {{\nblock0:\n call @missing()\n return\n}}"
    ));
    assert!(!module.functions[0].to_string().contains("call @missing"));
    keel_opt::run_passes(&mut module, &["unused-functions"]).unwrap();
    verify_module(&module).unwrap();
    assert!(!module.functions.iter().any(|f| f.name == "unused"));
}

#[test]
fn generated_labels_and_unknown_substitution_syntax_remain_conservative() {
    for assembly in [
        "module_asm \".macro invoke n\\n call \\\\n\\\\()\\n.endm\"",
        "module_asm \".altmacro\"",
        "module_asm \"call hel%=per\"",
    ] {
        let module = optimize(&format!("{SOURCE}\n{assembly}"));
        assert!(
            module.functions[0].to_string().contains("call @missing"),
            "{assembly}"
        );
    }
}

#[test]
fn operand_built_identifiers_and_bare_register_spellings_remain_roots() {
    for (assembly, name) in [
        ("call hel%c0per", "helper"),
        ("call $hel%c0per@PLT", "helper"),
        ("call hel%[operand]per", "helper"),
        ("call %0", "rax"),
        ("bl %0", "x0"),
    ] {
        let source = SOURCE.replace("helper", name);
        let source = format!("{source}\nmodule_asm \"{assembly}\"");
        let module = optimize(&source);
        assert!(
            module.functions[0].to_string().contains("call @missing"),
            "{assembly}"
        );
    }
}
