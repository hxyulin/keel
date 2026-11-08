use keel_ir::{ArgExt, CallConv, DataItem, InstData, Symbol, parse, verify_module};

const SOURCE: &str = "func @helper(i8 sext) -> i8 sext cc(c) local {
block0(v0: i8):
 v1 = iconst.i8 17
 v2 = iadd v0, v1
 return v2
}
func @entry(i64) -> i64 {
block0(v0: i64):
 v1 = trunc.i8 v0
 v2 = call @helper(v1)
 v3 = sext.i64 v2
 return v3
}";

#[test]
fn updates_every_direct_contract_and_is_idempotent() {
    let mut m = parse(SOURCE).unwrap();
    verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    verify_module(&m).unwrap();
    let f = &m.functions[0];
    assert_eq!(f.sig.cc, CallConv::Fast);
    assert!(
        f.sig
            .params
            .iter()
            .chain(&f.sig.returns)
            .all(|p| p.ext == ArgExt::None)
    );
    assert_eq!(m.functions[1].callees().next().unwrap().1.sig, f.sig);
    let text = m.to_string();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    assert_eq!(text, m.to_string());
}

#[test]
fn refuses_addresses_exports_and_tail_contracts() {
    // `@helper` returns `i8 sext`, so a C thunk could not be one tail call.
    for source in [
        SOURCE.replace(" local", ""),
        SOURCE.replace(" return v3", " asm \"nop\", \"\"()\n return v3"),
        "func @aggregate(ptr byval {i8}, i8 sext) -> i8 sext local {\nblock0(v0: ptr, v1: i8):\n return v1\n}".into(),
        "func @var(i8 sext, ...) -> i8 sext local {\nblock0(v0: i8):\n return v0\n}".into(),
        format!("{SOURCE}\ndata @address = align 8 {{ ptr @helper }}"),
        format!(
            "{SOURCE}\nfunc @address() -> ptr {{\nblock0:\n v0 = symbol_addr @helper\n return v0\n}}"
        ),
        format!(
            "{SOURCE}\nfunc @tail(i8 sext) -> i8 sext local {{\nblock0(v0: i8):\n return_call @helper(v0)\n}}"
        ),
    ] {
        let mut m = parse(&source).unwrap();
        verify_module(&m).unwrap();
        let before = m.to_string();
        keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
        assert_eq!(m.to_string(), before);
    }
}

#[test]
fn promotes_word_helpers_without_extensions_too() {
    let mut m = parse("func @word(i64) -> i64 local {\nblock0(v0: i64):\n return v0\n}").unwrap();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    verify_module(&m).unwrap();
    assert_eq!(m.functions[0].sig.cc, CallConv::Fast);
}

#[test]
fn full_finalizes_private_transport_after_cleanup_and_fast_retains_its_schedule() {
    use keel_opt::{OptimizationTier, optimize_with_tier};
    let pipeline = OptimizationTier::Full.pipeline();
    assert_eq!(
        &pipeline[pipeline.len() - 2..],
        &["branch-facts", "outline-cold"]
    );
    let pipeline = &pipeline[..pipeline.len() - 2];
    assert_eq!(pipeline.last(), Some(&"tail-calls"));
    assert_eq!(pipeline[pipeline.len() - 2], "internal-fast-calls");
    assert_eq!(
        pipeline
            .iter()
            .filter(|&&p| p == "internal-fast-calls")
            .count(),
        1
    );
    assert_eq!(pipeline[pipeline.len() - 3], "private-aggregate-cleanup");
    assert_eq!(pipeline[pipeline.len() - 4], "dce");
    let source = SOURCE.replace("cc(c) local", "cc(c) noinline local");
    for tier in [OptimizationTier::Fast, OptimizationTier::Full] {
        let mut m = parse(&source).unwrap();
        optimize_with_tier(&mut m, tier).unwrap();
        verify_module(&m).unwrap();
        let expected = if tier == OptimizationTier::Full {
            CallConv::Fast
        } else {
            CallConv::C
        };
        assert_eq!(m.functions[0].sig.cc, expected);
        assert_eq!(m.functions[1].sig.cc, CallConv::C);
        assert_eq!(
            m.functions[1].callees().next().unwrap().1.sig,
            m.functions[0].sig
        );
        assert!(m.functions[1].to_string().contains("call @helper"));
    }
}

/// `@helper` is called directly and its address escapes through
/// `symbol_addr` and data.
const ADDRESSED: &str = "func @helper(i8 sext, i64) -> i64 local {
block0(v0: i8, v1: i64):
 v2 = sext.i64 v0
 v3 = iadd v2, v1
 return v3
}
func @entry(i64) -> i64 {
block0(v0: i64):
 v1 = trunc.i8 v0
 v2 = call @helper(v1, v0)
 return v2
}
func @address() -> ptr {
block0:
 v0 = symbol_addr @helper
 return v0
}
data @table = align 8 { ptr @helper }";

fn addresses(m: &keel_ir::Module) -> Vec<String> {
    let mut names = Vec::new();
    for f in &m.functions {
        for (_, i) in f.layout_insts() {
            if let InstData::SymbolAddr {
                symbol: Symbol::Func(callee),
            } = f.inst_data(i)
            {
                names.push(f.callee(*callee).name.clone());
            }
        }
    }
    for d in &m.data {
        for item in &d.init.as_ref().unwrap().items {
            if let DataItem::Ptr { symbol, .. } = item {
                names.push(symbol.clone());
            }
        }
    }
    names
}

#[test]
fn address_taken_helpers_become_fast_behind_a_c_thunk() {
    let mut m = parse(ADDRESSED).unwrap();
    verify_module(&m).unwrap();
    let c_sig = m.functions[0].sig.clone();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    verify_module(&m).unwrap();
    let helper = &m.functions[0];
    assert_eq!(helper.sig.cc, CallConv::Fast);
    let thunk = m.functions.iter().find(|f| f.name == "helper.c").unwrap();
    assert!(thunk.local);
    assert_eq!(thunk.sig, c_sig);
    let body: Vec<_> = thunk
        .layout_insts()
        .map(|(_, i)| thunk.inst_data(i))
        .collect();
    let [InstData::ReturnCall { func, args }] = body.as_slice() else {
        panic!("{thunk}");
    };
    assert_eq!(thunk.callee(*func).name, "helper");
    assert_eq!(thunk.callee(*func).sig, helper.sig);
    assert_eq!(**args, *thunk.block_params(thunk.entry_block().unwrap()));
    assert_eq!(addresses(&m), ["helper.c", "helper.c"]);
    assert!(m.functions[1].to_string().contains("call @helper("));
    let text = m.to_string();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    assert_eq!(text, m.to_string());
    // The printed module parses back as the same module.
    assert_eq!(parse(&text).unwrap().to_string(), text);
}

#[test]
fn thunk_names_avoid_existing_symbols() {
    let source = format!(
        "{ADDRESSED}\nfunc @helper.c() local {{\nblock0:\n return\n}}\ndata @helper.c1 = align 1 {{ i8 0 }}"
    );
    let mut m = parse(&source).unwrap();
    keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
    verify_module(&m).unwrap();
    assert_eq!(addresses(&m)[..2], ["helper.c2", "helper.c2"]);
}

#[test]
fn address_taken_helpers_stay_c_without_a_one_jump_thunk_or_a_direct_call() {
    let many = (0..7).map(|_| "i64").collect::<Vec<_>>().join(", ");
    for source in [
        // Nothing calls it directly, so a thunk would gain nothing.
        ADDRESSED.replace(" v2 = call @helper(v1, v0)\n return v2", " return v0"),
        // A nonzero addend points inside the body, not at an entry.
        ADDRESSED.replace("ptr @helper }", "ptr @helper + 4 }"),
        // A seventh argument may be passed on the stack.
        ADDRESSED
            .replace(
                "(i8 sext, i64) -> i64 local",
                &format!("(i8 sext, {many}) -> i64 local"),
            )
            .replace(
                "block0(v0: i8, v1: i64):",
                "block0(v0: i8, v1: i64, v4: i64, v5: i64, v6: i64, v7: i64, v8: i64, v9: i64):",
            )
            .replace(
                "call @helper(v1, v0)",
                "call @helper(v1, v0, v0, v0, v0, v0, v0, v0)",
            ),
        // Module assembly may name the function and bypass its thunk.
        format!("module_asm \"nop\"\n{ADDRESSED}"),
    ] {
        let mut m = parse(&source).unwrap();
        verify_module(&m).unwrap();
        keel_opt::run_passes(&mut m, &["internal-fast-calls"]).unwrap();
        verify_module(&m).unwrap();
        assert_eq!(m.functions[0].sig.cc, CallConv::C, "{source}");
        assert!(!m.functions.iter().any(|f| f.name.starts_with("helper.c")));
    }
}
