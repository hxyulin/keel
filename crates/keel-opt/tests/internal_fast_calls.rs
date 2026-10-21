use keel_ir::{ArgExt, CallConv, parse, verify_module};

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
    assert_eq!(pipeline.last(), Some(&"internal-fast-calls"));
    assert_eq!(pipeline.iter().filter(|&&p| p == "internal-fast-calls").count(), 1);
    assert_eq!(pipeline[pipeline.len() - 2], "dce");
    let source = SOURCE.replace("cc(c) local", "cc(c) noinline local");
    for tier in [OptimizationTier::Fast, OptimizationTier::Full] {
        let mut m = parse(&source).unwrap();
        optimize_with_tier(&mut m, tier).unwrap();
        verify_module(&m).unwrap();
        let expected = if tier == OptimizationTier::Full { CallConv::Fast } else { CallConv::C };
        assert_eq!(m.functions[0].sig.cc, expected);
        assert_eq!(m.functions[1].sig.cc, CallConv::C);
        assert_eq!(m.functions[1].callees().next().unwrap().1.sig, m.functions[0].sig);
        assert!(m.functions[1].to_string().contains("call @helper"));
    }
}
