use super::*;
use crate::{
    INVALID, OK, keel_block_create, keel_build_iconst, keel_module_create, keel_module_optimize,
    keel_module_verify,
};
use std::ptr;

fn module() -> Box<KeelModule> {
    // SAFETY: A fresh allocation is immediately transferred into one Box.
    unsafe { Box::from_raw(keel_module_create()) }
}

fn signature(m: &mut KeelModule, params: &[(u32, u32)], returns: &[(u32, u32)]) -> KeelSignature {
    let params = params
        .iter()
        .map(|&(kind, extension)| KeelAbiParam { kind, extension })
        .collect::<Vec<_>>();
    let returns = returns
        .iter()
        .map(|&(kind, extension)| KeelAbiParam { kind, extension })
        .collect::<Vec<_>>();
    let mut sig = KeelSignature { owner: 0, index: 0 };
    assert_eq!(
        // SAFETY: All pointers are live, aligned, and disjoint from module storage.
        unsafe {
            keel_signature_create(
                m,
                params.as_ptr(),
                params.len(),
                returns.as_ptr(),
                returns.len(),
                &mut sig,
            )
        },
        OK
    );
    sig
}

fn function(m: &mut KeelModule, name: &str, sig: KeelSignature) -> (KeelFunction, KeelBlock) {
    let mut f = KeelFunction { owner: 0, index: 0 };
    let mut b = KeelBlock {
        owner: 0,
        function: 0,
        index: 0,
    };
    // SAFETY: All pointers are live, aligned, and disjoint from module storage.
    unsafe {
        assert_eq!(
            keel_module_add_function_with_signature(m, name.as_ptr(), name.len(), sig, &mut f),
            OK
        );
        assert_eq!(keel_block_create(m, f, &mut b), OK);
    }
    (f, b)
}

#[test]
fn rejects_invalid_abi_extensions_and_preserves_output() {
    let mut m = module();
    let mut out = KeelSignature {
        owner: 99,
        index: 88,
    };
    // SAFETY: Inputs/outputs are live, aligned, and do not alias the module.
    unsafe {
        for (kind, extension) in [(1, 0), (2, 0), (3, 1), (4, 2), (5, 1), (99, 0), (1, 99)] {
            let param = KeelAbiParam { kind, extension };
            assert_eq!(
                keel_signature_create(&mut *m, &param, 1, ptr::null(), 0, &mut out),
                INVALID
            );
        }
        assert_eq!(
            keel_signature_create(&mut *m, ptr::null(), 1, ptr::null(), 0, &mut out),
            INVALID
        );
    }
    assert_eq!((out.owner, out.index), (99, 88));
    assert!(m.signatures.is_empty());
    signature(&mut m, &[(1, 1), (2, 2), (5, 0)], &[(1, 2)]);
}

#[test]
fn rejects_invalid_storage_casts_and_foreign_slots_without_mutation() {
    let mut m = module();
    let sig = signature(&mut m, &[], &[]);
    let (f, b) = function(&mut m, "memory", sig);
    let (other, other_b) = function(&mut m, "other", sig);
    let mut slot = KeelStackSlot {
        owner: 0,
        function: 0,
        index: 0,
    };
    let mut address = KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    };
    let mut integer = address;
    // SAFETY: All raw pointer arguments point to live, disjoint allocations.
    unsafe {
        assert_eq!(keel_stack_slot_create(&mut *m, f, 0, 8, &mut slot), INVALID);
        assert_eq!(keel_stack_slot_create(&mut *m, f, 8, 3, &mut slot), INVALID);
        assert_eq!(
            keel_stack_slot_create(&mut *m, f, 8, 128, &mut slot),
            INVALID
        );
        assert_eq!(keel_stack_slot_create(&mut *m, f, 8, 8, &mut slot), OK);
        assert_eq!(keel_build_stack_address(&mut *m, b, slot, &mut address), OK);
        assert_eq!(keel_build_iconst(&mut *m, b, 4, 17, &mut integer), OK);
        let before = m.ir.functions[0].num_insts();
        let mut out = integer;
        assert_eq!(
            keel_build_stack_address(&mut *m, other_b, slot, &mut out),
            INVALID
        );
        let bad_slot = KeelStackSlot {
            index: u32::MAX,
            ..slot
        };
        assert_eq!(
            keel_build_stack_address(&mut *m, b, bad_slot, &mut out),
            INVALID
        );
        for flags in [
            KeelMemory { align: 0, flags: 0 },
            KeelMemory { align: 3, flags: 0 },
            KeelMemory { align: 8, flags: 8 },
            KeelMemory { align: 8, flags: 5 },
        ] {
            assert_eq!(
                keel_build_load(&mut *m, b, 4, address, flags, &mut out),
                INVALID
            );
            assert_eq!(
                keel_build_store(&mut *m, b, integer, address, flags),
                INVALID
            );
        }
        assert_eq!(
            keel_build_load(
                &mut *m,
                b,
                4,
                integer,
                KeelMemory { align: 1, flags: 0 },
                &mut out
            ),
            INVALID
        );
        assert_eq!(
            keel_build_ptradd(&mut *m, b, integer, integer, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_convert(&mut *m, b, 1, 4, integer, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_convert(&mut *m, b, 4, 3, address, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_convert(&mut *m, b, 5, 5, address, &mut out),
            INVALID
        );
        assert_eq!(keel_build_iconst(&mut *m, b, 5, 0, &mut out), INVALID);
        assert_eq!(m.ir.functions[0].num_insts(), before);
        assert_eq!(
            out.index, integer.index,
            "failed calls must not write output"
        );
        assert_eq!(keel_function_set_linkage(&mut *m, other, 99), INVALID);
    }
}

#[test]
fn forward_declarations_and_globals_share_a_checked_namespace() {
    let mut m = module();
    let sig = signature(&mut m, &[], &[]);
    let mut symbol = KeelSymbol { owner: 0, index: 0 };
    let mut f = KeelFunction { owner: 0, index: 0 };
    // SAFETY: All pointers are live and disjoint. Handles are copied by value.
    unsafe {
        assert_eq!(
            keel_module_declare_function(&mut *m, b"forward".as_ptr(), 7, sig, &mut symbol),
            OK
        );
        let original = symbol;
        assert_eq!(
            keel_module_add_zero_global(&mut *m, b"forward".as_ptr(), 7, 8, 8, 0, &mut symbol),
            INVALID
        );
        assert_eq!(keel_module_define_function(&mut *m, original, &mut f), OK);
        assert!(m.ir.decls.is_empty());
        assert_eq!(
            keel_module_define_function(&mut *m, original, &mut f),
            INVALID
        );
        assert_eq!(keel_function_symbol(&mut *m, f, &mut symbol), OK);
        assert_eq!(symbol.index, original.index);
        assert_eq!(
            keel_module_declare_global(&mut *m, b"data".as_ptr(), 4, &mut symbol),
            OK
        );
        let external = symbol;
        assert_eq!(
            keel_module_add_zero_global(&mut *m, b"data".as_ptr(), 4, 8, 8, 0, &mut symbol),
            OK
        );
        assert_eq!(symbol.index, external.index);
        assert_eq!(
            keel_module_define_function(&mut *m, symbol, &mut f),
            INVALID
        );
        assert_eq!(
            keel_module_add_zero_global(&mut *m, b"data".as_ptr(), 4, 8, 8, 0, &mut symbol),
            INVALID
        );
        for (size, align, flags) in [(0, 8, 0), (8, 3, 0), (8, 131072, 0), (8, 8, 8)] {
            assert_eq!(
                keel_module_add_zero_global(
                    &mut *m,
                    b"invalid".as_ptr(),
                    7,
                    size,
                    align,
                    flags,
                    &mut symbol
                ),
                INVALID
            );
        }
        assert_eq!(m.ir.data.len(), 1);
    }
}

#[test]
fn call_validation_precedes_imports_and_instruction_insertion() {
    let mut m = module();
    let sig = signature(&mut m, &[(4, 0)], &[(4, 0)]);
    let void = signature(&mut m, &[], &[]);
    let (_, b) = function(&mut m, "caller", void);
    let mut symbol = KeelSymbol { owner: 0, index: 0 };
    let mut value = KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    };
    // SAFETY: All pointer arguments are valid, aligned and disjoint.
    unsafe {
        assert_eq!(
            keel_module_declare_function(&mut *m, b"callee".as_ptr(), 6, sig, &mut symbol),
            OK
        );
        assert_eq!(keel_build_iconst(&mut *m, b, 4, 5, &mut value), OK);
        let before = m.ir.functions[0].num_insts();
        let mut out = value;
        assert_eq!(
            keel_build_call(&mut *m, b, symbol, ptr::null(), 0, &mut out, 1),
            INVALID
        );
        assert_eq!(
            keel_build_call(&mut *m, b, symbol, &value, 1, ptr::null_mut(), 0),
            INVALID
        );
        assert_eq!(
            keel_build_call(&mut *m, b, symbol, &value, 1, ptr::null_mut(), 1),
            INVALID
        );
        assert_eq!(
            keel_build_call_indirect(&mut *m, b, sig, value, &value, 1, &mut out, 1),
            INVALID
        );
        assert_eq!(m.ir.functions[0].num_insts(), before);
        assert_eq!(m.ir.functions[0].callees().count(), 0);
        assert_eq!(m.ir.functions[0].signatures().count(), 0);
        assert_eq!(
            keel_build_call(&mut *m, b, symbol, &value, 1, &mut out, 1),
            OK
        );
        assert_eq!(m.ir.functions[0].callees().count(), 1);
    }
}

#[test]
fn signature_symbol_and_slot_ids_reject_foreign_and_stale_owners() {
    let mut m = module();
    let sig = signature(&mut m, &[], &[]);
    let (f, b) = function(&mut m, "owner", sig);
    let mut other = module();
    let other_sig = signature(&mut other, &[], &[]);
    let (other_f, other_b) = function(&mut other, "other", other_sig);
    let mut symbol = KeelSymbol { owner: 0, index: 0 };
    let mut slot = KeelStackSlot {
        owner: 0,
        function: 0,
        index: 0,
    };
    let mut out = KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    };
    let mut result = f;
    // SAFETY: All module, name and output pointers are live and disjoint.
    unsafe {
        assert_eq!(keel_function_symbol(&mut *m, f, &mut symbol), OK);
        assert_eq!(keel_stack_slot_create(&mut *m, f, 8, 8, &mut slot), OK);
        assert_eq!(
            keel_module_add_function_with_signature(
                &mut *other,
                b"foreign".as_ptr(),
                7,
                sig,
                &mut result
            ),
            INVALID
        );
        assert_eq!(
            keel_build_symbol_address(&mut *other, other_b, symbol, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_stack_address(&mut *other, other_b, slot, &mut out),
            INVALID
        );
        let bad = KeelSignature {
            index: u32::MAX,
            ..sig
        };
        assert_eq!(
            keel_module_add_function_with_signature(&mut *m, b"bad".as_ptr(), 3, bad, &mut result),
            INVALID
        );
        let bad = KeelSymbol {
            index: u32::MAX,
            ..symbol
        };
        assert_eq!(
            keel_build_symbol_address(&mut *m, b, bad, &mut out),
            INVALID
        );
        assert_eq!(crate::keel_build_return(&mut *m, b, ptr::null(), 0), OK);
        assert_eq!(keel_module_verify(&mut *m), OK);
        assert_eq!(keel_module_optimize(&mut *m), OK);
        assert_eq!(
            keel_module_add_function_with_signature(
                &mut *m,
                b"stale".as_ptr(),
                5,
                sig,
                &mut result
            ),
            INVALID
        );
        let new_sig = signature(&mut m, &[], &[]);
        let (_, new_b) = function(&mut m, "new_function", new_sig);
        assert_eq!(
            keel_build_symbol_address(&mut *m, new_b, symbol, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_stack_address(&mut *m, new_b, slot, &mut out),
            INVALID
        );
        assert_eq!(keel_function_symbol(&mut *other, other_f, &mut symbol), OK);
    }
}
