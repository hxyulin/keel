//! Scope, composition and invalidation of producer contracts.
#[path = "contracts/lifetime.rs"]
mod lifetime;
#[path = "contracts/value_facts.rs"]
mod value_facts;

use keel_ir::{
    CallContract, FunctionAttrs, InstData, MemoryAccess, MemoryEffects, MemorySummary,
    PointerContract,
};

fn parsed(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).expect("parse contract fixture");
    keel_ir::verify_module(&module).expect("verify contract fixture");
    module
}
const SOURCE: &str = "decl @callee(ptr) -> ptr contract(memory(argmem: read, other: none, inaccessible: readwrite), nofree, param(0, align 8, nonnull, readable 16, nocapture))\nfunc @f(ptr) -> ptr contract(nosync, param(0, align 4)) {\nblock0(v0: ptr):\n v1 = call @callee(v0) contract(memory(argmem: readwrite, other: none, inaccessible: none), nosync, nocallback, result(0, align 16, nonnull))\n return v1\n}\n";

#[test]
fn richer_pointer_and_integer_contracts_roundtrip_at_their_scopes() {
    let module = parsed(
        "decl @callee(ptr, i32) -> ptr contract(param(0, captures(other(address_is_null), ret(address, read_provenance)), readable_or_null 32), range_param(1, 0xfffffff8, 0x8), result(0, noalias))\nfunc @f(ptr, i32) -> ptr {\nblock0(v0: ptr, v1: i32):\n v2 = call @callee(v0, v1) contract(param(0, nocapture), range_param(1, 0x0, 0x4))\n return v2\n}\n",
    );
    let output = module.to_string();
    assert_eq!(parsed(&output).to_string(), output);
    let mut function = module.functions[0].clone();
    let call = function.layout_insts().next().unwrap().1;
    let effective = function.effective_call_contract(call).unwrap();
    assert_eq!(
        effective.parameters[0].1.capture,
        keel_ir::CaptureContract::NONE
    );
    assert_eq!(effective.parameters[0].1.readable_or_null_bytes, 32);
    assert!(effective.returns[0].1.return_noalias);
    assert_eq!(
        effective.parameter_ranges.len(),
        2,
        "both simultaneous range constraints remain"
    );
    let cloned = function.clone();
    assert_eq!(cloned.call_contract(call), function.call_contract(call));
    let data = function.inst_data(call).clone();
    *function.inst_data_mut(call) = data;
    assert!(
        function.call_contract(call).is_none(),
        "mutation invalidates every site fact"
    );
    let mut removable = parsed(
        "func @drop(i32, i32) contract(range_param(1, 0, 8)) {\nblock0(v0: i32, v1: i32):\n return\n}\n",
    );
    let function = &mut removable.functions[0];
    function.remove_block_param(function.entry_block().unwrap(), 1);
    function.sig.params.pop();
    assert!(function.attrs.contract.parameter_ranges.is_empty());
    keel_ir::verify_module(&removable).unwrap();
}

#[test]
fn richer_contracts_reject_wrong_types_scopes_and_malformed_components() {
    for contract in [
        "param(0, noalias)",
        "result(0, captures(other(none), ret(address)))",
        "param(0, nocapture, captures(other(none), ret(none)))",
        "param(0, captures(other(none, address), ret(none)))",
        "param(0, captures(other(address, address_is_null), ret(none)))",
        "range_param(0, 0, 8)",
        "range_result(0, 0, 8)",
        "range_param(1, 8, 8)",
        "range_param(1, 0, 0x100000000)",
        "range_param(2, 0, 8)",
        "param(0, readable_or_null -1)",
    ] {
        let source = format!("decl @f(ptr, i32) -> ptr contract({contract})\n");
        let invalid = match keel_ir::parse(&source) {
            Err(_) => true,
            Ok(module) => keel_ir::verify_module(&module).is_err(),
        };
        assert!(invalid, "{source}");
    }
}

#[test]
fn separate_definition_declaration_and_call_scopes_roundtrip() {
    let module = parsed(SOURCE);
    let function = &module.functions[0];
    let call = function
        .layout_insts()
        .find(|(_, i)| matches!(function.inst_data(*i), InstData::Call { .. }))
        .unwrap()
        .1;
    let site = function.call_contract(call).unwrap();
    assert!(site.nosync && site.nocallback && !site.nofree);
    assert!(site.parameters.is_empty());
    let effective = function.effective_call_contract(call).unwrap();
    assert!(effective.nofree && effective.nosync && effective.nocallback);
    assert_eq!(effective.memory.argmem, MemoryAccess::Read);
    assert_eq!(effective.memory.other, MemoryAccess::None);
    assert_eq!(effective.memory.inaccessible, MemoryAccess::None);
    assert_eq!(effective.parameters[0].1.readable_bytes, 16);
    assert_eq!(effective.returns[0].1.align, 16);
    assert!(!function.attrs.contract.nofree);
    assert_eq!(
        function.inst_data(call).memory_effects(),
        MemoryEffects::Barrier
    );
    let output = module.to_string();
    assert_eq!(parsed(&output).to_string(), output);
}

#[test]
fn indirect_call_contract_uses_only_its_own_scope() {
    let module = parsed(
        "func @f(ptr,ptr) -> ptr {\nblock0(v0: ptr,v1: ptr):\n v2 = call_indirect v0(v1) : (ptr) -> ptr cc(c) contract(nofree, param(0, readable 8), result(0, nonnull))\n return v2\n}\n",
    );
    let f = &module.functions[0];
    let call = f.layout_insts().next().unwrap().1;
    assert_eq!(
        f.effective_call_contract(call),
        f.call_contract(call).cloned()
    );
    assert_eq!(parsed(&module.to_string()).to_string(), module.to_string());
}

#[test]
fn unknown_defaults_do_not_change_abi_or_text() {
    let module = parsed(
        "decl @callee(ptr) -> ptr\nfunc @f(ptr) -> ptr {\nblock0(v0: ptr):\n v1 = call @callee(v0)\n return v1\n}\n",
    );
    let f = &module.functions[0];
    let call = f.layout_insts().next().unwrap().1;
    assert_eq!(
        f.effective_call_contract(call),
        Some(CallContract::default())
    );
    assert_eq!(f.call_contract(call), None);
    assert!(!module.to_string().contains("contract("));
    assert_eq!(module.decls[0].sig, f.sig);
    assert_eq!(
        MemorySummary::default(),
        MemorySummary {
            argmem: MemoryAccess::ReadWrite,
            other: MemoryAccess::ReadWrite,
            inaccessible: MemoryAccess::ReadWrite
        }
    );
}

#[test]
fn cloning_instruction_drops_site_facts_but_function_clone_retains_them() {
    let mut module = parsed(SOURCE);
    let f = &mut module.functions[0];
    let (block, call) = f.layout_insts().next().unwrap();
    let clone = f.clone();
    assert_eq!(clone.call_contract(call), f.call_contract(call));
    let data = f.inst_data(call).clone();
    let inserted = f.append_inst(block, data);
    assert!(f.call_contract(inserted).is_none());
    f.remove_inst(inserted);
    let _ = f.inst_data_mut(call);
    assert!(f.call_contract(call).is_none());
    f.set_call_contract(
        call,
        CallContract {
            nofree: true,
            ..Default::default()
        },
    )
    .unwrap();
    f.remove_inst(call);
    assert!(f.call_contract(call).is_none());
}

#[test]
fn removing_entry_parameter_drops_scoped_parameter_facts() {
    let mut module = parsed(
        "func @f(ptr) -> i64 contract(param(0, nonnull)) {\nblock0(v0: ptr):\n v1 = iconst.i64 7\n return v1\n}\n",
    );
    let f = &mut module.functions[0];
    f.remove_block_param(f.entry_block().unwrap(), 0);
    f.sig.params.remove(0);
    assert!(f.attrs.contract.parameters.is_empty());
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn merges_simultaneous_contracts_without_weakening_or_changing_signature() {
    let mut attrs = FunctionAttrs {
        contract: CallContract {
            memory: MemorySummary {
                argmem: MemoryAccess::Read,
                ..Default::default()
            },
            parameters: vec![(
                0,
                PointerContract {
                    align: 4,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
        ..Default::default()
    };
    attrs.merge(FunctionAttrs {
        contract: CallContract {
            memory: MemorySummary {
                argmem: MemoryAccess::Write,
                ..Default::default()
            },
            nofree: true,
            parameters: vec![(
                0,
                PointerContract {
                    align: 16,
                    nonnull: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(attrs.contract.memory.argmem, MemoryAccess::None);
    assert!(attrs.contract.nofree && attrs.contract.parameters[0].1.nonnull);
    assert_eq!(attrs.contract.parameters[0].1.align, 16);
}

#[test]
fn malformed_contracts_and_incompatible_positions_are_rejected() {
    for header in [
        "contract(param(0, align 3))",
        "contract(param(1, nonnull))",
        "contract(param(0, nonnull), param(0, align 8))",
        "contract(result(0, nocapture))",
        "contract(memory(unknown: read))",
        "contract(nofree, nofree)",
        "contract(param(0, readable -1))",
    ] {
        let source = format!("func @f(ptr) -> ptr {header} {{\nblock0(v0: ptr):\n return v0\n}}\n");
        assert!(
            keel_ir::parse(&source)
                .map(|m| keel_ir::verify_module(&m).is_err())
                .unwrap_or(true),
            "{source}"
        );
    }
    let bad_type = "func @f(i64) contract(param(0, nonnull)) {\nblock0(v0: i64):\n return\n}";
    assert!(keel_ir::verify_module(&keel_ir::parse(bad_type).unwrap()).is_err());
    assert!(
        keel_ir::parse(
            "func @f() -> i64 {\nblock0:\n v0 = iconst.i64 1 contract(nofree)\n return v0\n}"
        )
        .is_err()
    );
}

#[test]
fn mutable_declaration_invalidates_site_facts_but_attribute_sync_does_not() {
    let mut module = parsed(SOURCE);
    let call = module.functions[0].layout_insts().next().unwrap().1;
    module.sync_function_attrs();
    assert!(module.functions[0].call_contract(call).is_some());
    let func = match module.functions[0].inst_data(call) {
        InstData::Call { func, .. } => *func,
        _ => unreachable!(),
    };
    let declaration = module.functions[0].callee_mut(func);
    assert!(declaration.attrs.contract.is_unknown());
    assert!(module.functions[0].call_contract(call).is_none());
}

#[test]
fn removed_block_discards_its_call_site_contracts() {
    let mut module = parsed(
        "decl @callee()\nfunc @f(i8) {\nblock0(v0:i8):\n brif v0, block1, block2\nblock1:\n call @callee() contract(nofree)\n jump block2\nblock2:\n return\n}\n",
    );
    let f = &mut module.functions[0];
    let (block, call) = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
        .unwrap();
    f.remove_block(block);
    assert!(f.call_contract(call).is_none());
}

#[test]
fn accessibility_preservation_is_explicit_and_not_implied_by_nofree() {
    let m = parsed(
        "decl @safe() contract(nofree, nosync, nocallback, preserves_access)\ndecl @unknown_access() contract(nofree, nosync, nocallback)\n",
    );
    assert!(m.decls[0].attrs.contract.preserves_access);
    assert!(!m.decls[1].attrs.contract.preserves_access);
    assert_eq!(parsed(&m.to_string()).to_string(), m.to_string());
}
