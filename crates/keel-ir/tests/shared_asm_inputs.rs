//! Matching inputs require a proof about SSA values, independent of allocation.
fn check(definitions: &str, constraints: &str, arguments: &str) -> Result<(), String> {
    let module = keel_ir::parse(&format!(
        "decl data @anchor\ndecl data @other\nfunc @f(i64,i64,ptr) -> i64 {{\nblock0(v0:i64,v1:i64,v2:ptr):\n{definitions}\n v20 = asm \"\", \"{constraints}\"({arguments}) -> i64\n return v20\n}}"
    )).unwrap();
    keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))
}

#[test]
fn common_low_bits_prove_matching_initializers() {
    for (definitions, args) in [
        ("", "v0,v0"),
        (" v3 = trunc.i8 v0", "v0,v3"),
        (" v3 = trunc.i16 v0\n v4 = trunc.i8 v3", "v4,v0"),
        (" v3 = trunc.i8 v0\n v4 = sext.i64 v3", "v3,v4"),
        (" v3 = ptrtoint.i64 v2", "v2,v3"),
        (" v3 = symbol_addr @anchor\n v4 = symbol_addr @anchor", "v3,v4"),
        (" v3 = iconst.i64 511\n v4 = iconst.i8 -1", "v3,v4"),
    ] {
        check(definitions, "=r,0,0", args).unwrap();
        check(definitions, "=&r,0,0", args).unwrap();
    }
}

#[test]
fn unrelated_values_and_required_high_bits_cannot_share() {
    for (definitions, args) in [
        ("", "v0,v1"),
        (" v3 = symbol_addr @anchor\n v4 = symbol_addr @other", "v3,v4"),
        (" v3 = iconst.i64 254\n v4 = iconst.i8 -1", "v3,v4"),
        (" v3 = trunc.i8 v0\n v4 = uext.i64 v3", "v0,v4"),
        (" v3 = load.i64 v2\n v4 = load.i64 v2", "v3,v4"),
        (" v3 = iconst.i64 1\n v4 = iadd v0,v3", "v0,v4"),
    ] {
        let error = check(definitions, "=r,0,0", args).unwrap_err();
        assert!(
            error.contains("conflicting or unproven"),
            "{definitions}: {error}"
        );
    }
    // Pairwise checking must not use the narrow first input as the sole
    // representative and thereby miss conflicting high bits in wider inputs.
    let error = check(" v3 = trunc.i8 v0", "=r,0,0,0", "v3,v0,v1").unwrap_err();
    assert!(error.contains("conflicting or unproven"), "{error}");
}
