#[path = "sections/global_symbols.rs"]
mod global_symbols;
#[path = "sections/weak.rs"]
mod weak;
#[path = "sections/weak_odr.rs"]
mod weak_odr;

use keel_ir::{Target, parse, section::validate_module, verify_module};
#[test]
fn placement_metadata_roundtrips_and_invalid_storage_diagnoses() {
    for target in Target::NATIVE {
        let section = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => ".custom",
            Target::Aarch64AppleDarwin => "__DATA,__custom",
        };
        let code = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => ".custom_code",
            Target::Aarch64AppleDarwin => "__TEXT,__custom",
        };
        let text = format!(
            "data @x = local retain section \"{section}\" align 32 {{ i32 7 }}\nfunc @f() used retain section \"{code}\" local {{\nblock0:\n return\n}}\n"
        );
        let m = parse(&text).unwrap();
        verify_module(&m).unwrap();
        validate_module(&m, target).unwrap();
        let canonical = m.to_string();
        assert_eq!(parse(&canonical).unwrap().to_string(), canonical);
        assert!(m.functions[0].attrs.retain);
        assert!(m.data[0].retain);
        {
            let storage = "weak_odr";
            let text =
                format!("data @x = {storage} retain section \"{section}\" align 4 {{ i32 1 }}\n");
            let m = parse(&text).unwrap();
            assert!(verify_module(&m).is_err());
            assert!(validate_module(&m, target).is_err());
        }
        let m = parse(&format!(
            "func @f() retain section \"{code}\" weak_odr {{\nblock0:\n return\n}}\n"
        ))
        .unwrap();
        assert!(verify_module(&m).is_err());
        assert!(validate_module(&m, target).is_err());
        let m = parse(&format!(
            "data @x = retain section \"{section}\" align 3 {{ i32 1 }}\n"
        ))
        .unwrap();
        assert!(verify_module(&m).is_err());
        assert!(validate_module(&m, target).is_err());
    }
    for attrs in ["retain", "section \".custom\""] {
        let m = parse(&format!("decl @f() {attrs}\n")).unwrap();
        assert!(verify_module(&m).is_err());
    }
    for name in ["", "bad\\nname", "a,b,c", "bad;name"] {
        let text = format!("data @x = section \"{name}\" align 4 {{ i32 1 }}\n");
        if let Ok(m) = parse(&text) {
            assert!(
                Target::NATIVE
                    .into_iter()
                    .all(|t| validate_module(&m, t).is_err())
            );
        }
    }
}
#[test]
fn repeated_attributes_and_unsafe_names_are_rejected() {
    for attr in ["retain retain", "section \".a\" section \".b\""] {
        assert!(parse(&format!("func @f() {attr} {{\nblock0:\n return\n}}\n")).is_err());
    }
    assert!(keel_ir::SectionName::new(&"a".repeat(129)).is_err());
    assert!(keel_ir::SectionName::new(&"a".repeat(128)).is_ok());
}

#[test]
fn gnu_underscore_metadata_names_are_not_custom_elf_storage() {
    for name in [".gnu_debuglink", ".gnu_debugaltlink", ".gnu_debugdata"] {
        // The generic name syntax is safe; target storage validation must still
        // reject a metadata name, including IR that bypasses the C frontend.
        let m = parse(&format!(
            "data @x = retain section \"{name}\" align 4 {{ i32 1 }}\n"
        ))
        .unwrap();
        verify_module(&m).unwrap();
        assert!(
            validate_module(&m, Target::X86_64LinuxGnu)
                .unwrap_err()
                .contains("special/TLS/COMDAT")
        );
    }
}

#[test]
fn static_readonly_relocations_and_ref_data_validate_without_c_frontend() {
    let mut module = parse(
        r#"
        data @anchor = align 4 { i32 1 }
        data @plain = section ".init.rodata" align 8 { i64 3 }
        data @pointer = section ".init.rodata" align 8 { ptr @anchor }
        data @zero = section ".ref.data" align 8 { zero 8 }
        data @value = section ".ref.data" align 8 { i64 5 }
    "#,
    )
    .unwrap();
    module.data[1].init.as_mut().unwrap().constant = true;
    module.data[2].init.as_mut().unwrap().constant = true;
    verify_module(&module).unwrap();
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        validate_module(&module, target).unwrap();
        module.data[1].init.as_mut().unwrap().constant = false;
        assert!(validate_module(&module, target).is_err());
        module.data[1].init.as_mut().unwrap().constant = true;
        module.data[2].retain = true;
        assert!(
            validate_module(&module, target)
                .unwrap_err()
                .contains("incompatible storage or retention")
        );
        module.data[2].retain = false;
    }
}
