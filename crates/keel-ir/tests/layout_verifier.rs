use keel_ir::{
    DataItem, DataLayout, EntityRef, Module, SourceFile, SourceLoc, Type, VerifierError, parse,
    verify_module, verify_module_with_layout,
};

fn errors(module: &Module, layout: DataLayout) -> Vec<String> {
    verify_module_with_layout(module, layout)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn wasm32_adjacent_pointers_and_i32_records_verify_without_rewriting() {
    // Packed stream: pointers at 0, 4, 12, and a sentinel at 16.
    let module = parse(
        "data @bytes = align 1 { b\"abc\\0\" }\n\
         data @records = align 4 { ptr @bytes, ptr @bytes + 3, i32 7, ptr @bytes, b\"\\xa5\" }",
    )
    .unwrap();
    let before = module.to_string();
    assert_eq!(
        verify_module_with_layout(&module, DataLayout::WASM32),
        Ok(())
    );
    assert_eq!(module.to_string(), before);
    assert_eq!(
        errors(&module, DataLayout::NATIVE64),
        [
            "@records: holds pointers, so it needs align 8 or more, got 4",
            "@records: ptr @bytes is at offset 20, which is not a multiple of 8",
        ]
    );
    assert_eq!(
        module.data[1].init.as_ref().unwrap().items.last(),
        Some(&DataItem::Bytes(vec![0xa5]))
    );
}

#[test]
fn alignment_and_cursor_follow_only_the_selected_pointer_storage() {
    for (prefix, wasm_offset, native_offset) in [("i32 7, ", 4, 4), ("ptr @bytes, i32 7, ", 8, 12)]
    {
        let module = parse(&format!(
            "data @bytes = align 1 {{ b\"x\" }}\n\
             data @p = align 8 {{ {prefix}ptr @bytes }}"
        ))
        .unwrap();
        assert!(
            errors(&module, DataLayout::WASM32).is_empty(),
            "{wasm_offset}"
        );
        assert_eq!(
            errors(&module, DataLayout::NATIVE64),
            [format!(
                "@p: ptr @bytes is at offset {native_offset}, which is not a multiple of 8"
            )]
        );
    }
    let module = parse("decl data @bytes\ndata @p = align 2 { ptr @bytes, ptr @bytes }").unwrap();
    assert_eq!(
        errors(&module, DataLayout::WASM32),
        ["@p: holds pointers, so it needs align 4 or more, got 2"]
    );
    let module = parse("decl data @bytes\ndata @p = align 8 { i8 1, ptr @bytes }").unwrap();
    assert_eq!(
        errors(&module, DataLayout::WASM32),
        ["@p: ptr @bytes is at offset 1, which is not a multiple of 4"]
    );
}

#[test]
fn relocation_target_size_uses_layout_and_allows_one_past() {
    for addend in [0, 7, 8, 9, 11, 12, 13] {
        let module = parse(&format!(
            "decl data @external\n\
             data @target = align 8 {{ ptr @external, i32 7 }}\n\
             data @ref = align 8 {{ ptr @target + {addend} }}"
        ))
        .unwrap();
        for (layout, size) in [(DataLayout::WASM32, 8), (DataLayout::NATIVE64, 12)] {
            let expected = if addend > size {
                vec![format!(
                    "@ref: @target + {addend} is past the end of @target ({size} bytes)"
                )]
            } else {
                vec![]
            };
            assert_eq!(errors(&module, layout), expected);
        }
    }
}

#[test]
fn relocation_symbol_kinds_keep_the_existing_native_rules() {
    let module = parse(
        "decl @function()\n\
         decl data @external\n\
         data @tls = thread_local align 1 { i8 1 }\n\
         data @refs = align 8 { ptr @function, ptr @function + 4294967295, ptr @external + 4294967295, ptr @missing, ptr @tls }",
    )
    .unwrap();
    let expected = [
        "@refs: points to @missing, which is not declared",
        "@refs: points to thread-local @tls, whose address differs per thread",
    ];
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(errors(&module, layout), expected);
    }
    let module = parse(
        "func @fast() cc(fast) local {\nblock0:\nreturn\n}\n\
         data @ref = align 8 { ptr @fast }",
    )
    .unwrap();
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(
            errors(&module, layout),
            ["@ref: cc(fast) function @fast cannot have its address taken"]
        );
    }
}

#[test]
fn rtti_keeps_native_size_alignment_and_data_symbol_requirement() {
    let module = parse(
        "data @name = align 1 { b\"name\\0\" }\n\
         data @rtti = align 8 { rtti_name @name, ptr @name }",
    )
    .unwrap();
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert!(errors(&module, layout).is_empty());
    }
    let module = parse(
        "decl @function()\n\
         data @rtti = align 4 { i32 0, rtti_name @function }",
    )
    .unwrap();
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(
            errors(&module, layout),
            [
                "@rtti: rtti_name requires a data symbol, got @function",
                "@rtti: ptr @function is at offset 4, which is not a multiple of 8",
                "@rtti: holds pointers, so it needs align 8 or more, got 4",
            ]
        );
    }
    // RTTI still consumes 8 bytes under WASM32: a later RTTI starts at 12.
    let module = parse(
        "decl data @name\ndata @rtti = align 8 { rtti_name @name, ptr @name, rtti_name @name }",
    )
    .unwrap();
    assert_eq!(
        errors(&module, DataLayout::WASM32),
        ["@rtti: ptr @name is at offset 12, which is not a multiple of 8"]
    );
    assert!(errors(&module, DataLayout::NATIVE64).is_empty());
}

#[test]
fn malformed_initializers_are_checked_on_the_actual_items() {
    let mut module = parse("data @bad = align 1 { i8 0 }").unwrap();
    let init = module.data[0].init.as_mut().unwrap();
    init.align = 3;
    init.items = vec![
        DataItem::Int {
            ty: Type::Ptr,
            value: 0,
        },
        DataItem::Int {
            ty: Type::F64,
            value: 0,
        },
        DataItem::Int {
            ty: Type::I128,
            value: 0,
        },
        DataItem::Int {
            ty: Type::I8,
            value: 256,
        },
        DataItem::Int {
            ty: Type::I16,
            value: -32769,
        },
    ];
    let expected = [
        "@bad: alignment must be a power of two up to 65536, got 3",
        "@bad: data integers must be i8 to i64, got ptr",
        "@bad: data integers must be i8 to i64, got f64",
        "@bad: data integers must be i8 to i64, got i128",
        "@bad: 256 does not fit in i8",
        "@bad: -32769 does not fit in i16",
    ];
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(errors(&module, layout), expected);
    }
    module.data[0].init.as_mut().unwrap().items = vec![DataItem::Zero(0)];
    assert!(
        errors(&module, DataLayout::WASM32)
            .iter()
            .any(|e| e == "@bad: data object is empty")
    );
}

#[test]
fn existing_verifier_corpus_preserves_all_checks_and_native_diagnostics() {
    let mut count = 0;
    let mut failures = 0;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "keel") {
            continue;
        }
        let Ok(module) = parse(&std::fs::read_to_string(&path).unwrap()) else {
            continue;
        };
        let native: Result<(), Vec<VerifierError>> = verify_module(&module);
        assert_eq!(
            verify_module_with_layout(&module, DataLayout::NATIVE64),
            native,
            "{path:?}"
        );
        // With no layout-dependent data, every existing check/diagnostic must
        // also remain identical under WASM32, including invalid actual modules.
        if module
            .data
            .iter()
            .filter_map(|d| d.init.as_ref())
            .all(|i| !i.has_relocations())
        {
            assert_eq!(
                verify_module_with_layout(&module, DataLayout::WASM32),
                native,
                "{path:?}"
            );
            count += 1;
            failures += usize::from(native.is_err());
        }
    }
    assert!(count >= 40, "only {count} parsed corpus controls");
    assert!(failures >= 20, "only {failures} malformed corpus controls");
}

#[test]
fn wasm_initializers_do_not_hide_function_debug_reference_or_metadata_errors() {
    let prefix = "decl data @layout_external\ndata @layout_records = align 4 { ptr @layout_external, ptr @layout_external, i32 7, b\"\\xa5\" }\n";
    for source in [
        include_str!("golden/err_structure.keel"),
        include_str!("golden/err_types.keel"),
        include_str!("golden/err_calls.keel"),
        include_str!("golden/err_debug_info.keel"),
        include_str!("golden/err_debug_scopes.keel"),
        include_str!("golden/err_block_args.keel"),
        include_str!("golden/err_not_dominated.keel"),
        include_str!("golden/err_structs.keel"),
    ] {
        let original = parse(source).unwrap();
        let expected = verify_module(&original).unwrap_err();
        let actual = parse(&format!("{prefix}{source}")).unwrap();
        assert_eq!(
            verify_module_with_layout(&actual, DataLayout::WASM32),
            Err(expected)
        );
    }
    let mut actual = parse(&format!("{prefix}func @location() {{\nblock0:\nreturn\n}}")).unwrap();
    let func = &mut actual.functions[0];
    let inst = func.layout_insts().next().unwrap().1;
    func.set_inst_loc(
        inst,
        Some(SourceLoc {
            file: SourceFile::new(0),
            line: 1,
            col: 1,
            scope: None,
        }),
    );
    assert_eq!(
        errors(&actual, DataLayout::WASM32),
        ["@location inst0: has location f0:1:1, but the module has no file f0"]
    );
    let mut actual = parse(prefix).unwrap();
    actual.data[1].thread_local = true;
    actual.data[1].init.as_mut().unwrap().constant = true;
    assert_eq!(
        errors(&actual, DataLayout::WASM32),
        ["@layout_records: thread-local data cannot be const"]
    );
}

#[test]
fn empty_definitions_are_not_external_declarations() {
    for items in ["", "zero 0", "b\"\", zero 0"] {
        let module = parse(&format!(
            "decl data @external\ndata @empty = align 1 {{ {items} }}\n\
             data @refs = const align 8 {{ ptr @empty, ptr @empty + 1, ptr @external + 4294967295 }}"
        ))
        .unwrap();
        for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
            assert_eq!(
                errors(&module, layout),
                [
                    "@empty: data object is empty",
                    "@refs: @empty + 1 is past the end of @empty (0 bytes)",
                ],
                "{items}"
            );
        }
    }
}

#[test]
fn large_zero_storage_does_not_narrow_target_sizes_or_cursors() {
    // Zero runs need no huge allocation. General IR verification checks storage
    // sizes in u64, independently of a backend's wasm32 address-space limits.
    let module = parse(
        "data @short = align 1 { zero 4294967294 }\n\
         data @exact = align 1 { zero 4294967295 }\n\
         data @large = align 8 { zero 4294967295, zero 1, ptr @exact }\n\
         data @refs = align 8 { ptr @short + 4294967295, ptr @exact + 4294967295, ptr @large + 4294967295 }",
    )
    .unwrap();
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(
            errors(&module, layout),
            ["@refs: @short + 4294967295 is past the end of @short (4294967294 bytes)"]
        );
    }
}

#[test]
fn mixed_pointer_rtti_alignment_reports_each_requirement_once() {
    for (items, wasm_requirements) in [
        (
            "ptr @name, zero 4, rtti_name @name, ptr @name, zero 4, rtti_name @name",
            vec![
                "@mixed: holds pointers, so it needs align 4 or more, got 1",
                "@mixed: holds pointers, so it needs align 8 or more, got 1",
            ],
        ),
        (
            "rtti_name @name, ptr @name, zero 4, rtti_name @name, ptr @name",
            vec!["@mixed: holds pointers, so it needs align 8 or more, got 1"],
        ),
    ] {
        let module = parse(&format!(
            "decl data @name\ndata @mixed = align 1 {{ {items} }}"
        ))
        .unwrap();
        assert_eq!(errors(&module, DataLayout::WASM32), wasm_requirements);
        let native = errors(&module, DataLayout::NATIVE64);
        assert_eq!(
            native
                .iter()
                .filter(|e| e.contains("holds pointers"))
                .collect::<Vec<_>>(),
            ["@mixed: holds pointers, so it needs align 8 or more, got 1"]
        );
        assert!(native.iter().any(|e| e.contains("not a multiple of 8")));
    }
}

#[test]
fn const_pointer_initializers_keep_storage_metadata_validation() {
    let module = parse(
        "decl data @external\n\
         data @table = hidden const align 4 { ptr @external, ptr @external }",
    )
    .unwrap();
    let before = module.clone();
    assert_eq!(
        verify_module_with_layout(&module, DataLayout::WASM32),
        Ok(())
    );
    assert_eq!(module.data, before.data);
    // Const pointer storage is legal; incompatible linkage/placement metadata
    // must still diagnose on the actual object carrying wasm32 initializers.
    for (field, expected) in [
        (0, "@table: thread-local data cannot be const"),
        (1, "@table: hidden requires a nonlocal definition"),
        (2, "@table: section/retain requires a non-COMDAT definition"),
    ] {
        let mut invalid = module.clone();
        let table = &mut invalid.data[1];
        match field {
            0 => table.thread_local = true,
            1 => table.local = true,
            2 => {
                table.weak_odr = true;
                table.retain = true;
            }
            _ => unreachable!(),
        }
        assert_eq!(errors(&invalid, DataLayout::WASM32), [expected]);
    }
}
