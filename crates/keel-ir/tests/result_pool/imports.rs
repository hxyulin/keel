use keel_ir::{EntityRef, Function, InsertPoint, Signature, Symbol, Type, verify};

/// A function referring to tens of thousands of distinct symbols, as a large
/// C initializer can. Lookups must not scan the tables once per reference,
/// and references keep their first-seen numbering.
#[test]
fn many_distinct_symbols_keep_first_seen_numbering() {
    const N: usize = 20_000;
    let mut f = Function::new("init", Signature::new(&[], &[]));
    let block = f.create_block();
    for i in 0..N {
        let name = format!("global{i}");
        let data = f.import_data(&name);
        assert_eq!(data.index(), i);
        // Every earlier name still resolves to its first reference.
        assert_eq!(f.import_data(&format!("global{}", i / 2)).index(), i / 2);
        let addr = f
            .ins(InsertPoint::End(block))
            .symbol_addr(Symbol::Data(data));
        let callee = f.import_function(&format!("func{i}"), Signature::new(&[Type::Ptr], &[]));
        assert_eq!(callee.index(), i);
        assert_eq!(
            f.import_function(&format!("func{}", i / 3), Signature::new(&[], &[]))
                .index(),
            i / 3
        );
        f.ins(InsertPoint::End(block)).call(callee, &[addr]);
        // Distinct signatures, then a repeat of an earlier one.
        let params = vec![Type::I32; i % 300];
        let returns = vec![Type::I64; i / 300 % 4];
        let sig = f.import_signature(Signature::new(&params, &returns));
        assert_eq!(sig.index(), i % 1200);
        assert_eq!(
            f.import_signature(Signature::new(&vec![Type::I32; i / 2 % 300], &[])),
            f.import_signature(Signature::new(&vec![Type::I32; i / 2 % 300], &[]))
        );
    }
    f.ins(InsertPoint::End(block)).return_(&[]);
    assert_eq!(f.data_symbols().count(), N);
    assert_eq!(f.callees().count(), N);
    assert_eq!(f.signatures().count(), 1200);
    verify(&f).unwrap();
}

/// Renaming a callee in place moves its lookup with it, as a scan would.
#[test]
fn renamed_callees_are_found_by_their_new_name() {
    let mut f = Function::new("caller", Signature::new(&[], &[]));
    let refs: Vec<_> = (0..100)
        .map(|i| f.import_function(&format!("f{i}"), Signature::new(&[], &[])))
        .collect();
    f.callee_mut(refs[40]).name = "renamed".into();
    assert_eq!(
        f.import_function("renamed", Signature::new(&[], &[])),
        refs[40]
    );
    let fresh = f.import_function("f40", Signature::new(&[], &[]));
    assert_eq!(fresh.index(), 100);
    // Touching callees without renaming them keeps every lookup.
    for &r in &refs {
        f.callee_mut(r).attrs.noinline = true;
    }
    for (i, &r) in refs.iter().enumerate() {
        let name = if i == 40 {
            "renamed".to_string()
        } else {
            format!("f{i}")
        };
        assert_eq!(f.import_function(&name, Signature::new(&[], &[])), r);
    }
}
