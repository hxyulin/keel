use super::systems::keel_build_convert;
use super::*;
use super::{abi::*, atomics::*, numeric::*, pipeline::*};
use std::ptr;

fn module() -> Box<KeelModule> {
    // SAFETY: Transfer a fresh unique API allocation into one Box.
    unsafe { Box::from_raw(keel_module_create()) }
}
fn builder() -> (Box<KeelModule>, KeelBlock) {
    let mut m = module();
    let mut f = Function::new("f", Signature::new(&[], &[]));
    let block = f.create_block();
    m.ir.functions.push(f);
    let b = KeelBlock {
        owner: m.owner,
        function: 0,
        index: block.index() as u32,
    };
    (m, b)
}
fn empty_value() -> KeelValue {
    KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    }
}
const IR: &str = "func @increment(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 1\n v2 = iadd v0, v1\n return v2\n}\n";

#[test]
fn complete_ir_load_merge_and_lookup_preserve_transaction_contracts() {
    let mut m = module();
    // SAFETY: Text and output storage are live and disjoint from the module.
    unsafe {
        assert_eq!(keel_module_load_ir(&mut *m, IR.as_ptr(), IR.len()), OK);
        let initial = m.owner;
        let mut f = KeelFunction { owner: 0, index: 0 };
        assert_eq!(
            keel_module_find_function(&mut *m, b"increment".as_ptr(), 9, &mut f),
            OK
        );
        let mut b = KeelBlock {
            owner: 0,
            function: 0,
            index: 0,
        };
        assert_eq!(keel_function_block_at(&mut *m, f, 0, &mut b), OK);
        let mut v = empty_value();
        assert_eq!(keel_block_parameter_at(&mut *m, b, 0, &mut v), OK);
        let mut kind = 0;
        assert_eq!(keel_value_type(&mut *m, v, &mut kind), OK);
        assert_eq!(kind, 4);
        assert_eq!(
            keel_module_load_ir(&mut *m, b"invalid".as_ptr(), 7),
            INVALID
        );
        assert_eq!(m.owner, initial);
        assert_eq!(m.ir.functions.len(), 1);
        let malformed = "func @bad() -> i64 {\nblock0:\n return\n}\n";
        assert_eq!(
            keel_module_load_ir(&mut *m, malformed.as_ptr(), malformed.len()),
            VERIFY
        );
        assert_eq!(m.owner, initial);
        assert_eq!(keel_module_optimize_with_tier(&mut *m, 999), INVALID);
        assert_eq!(
            keel_module_run_passes(&mut *m, b"unknown".as_ptr(), 7),
            INVALID
        );
        assert_eq!(m.owner, initial);
        assert_eq!(keel_module_optimize_with_tier(&mut *m, 2), OK);
        assert_eq!(keel_function_block_at(&mut *m, f, 0, &mut b), INVALID);
        assert_eq!(
            keel_module_find_function(&mut *m, b"increment".as_ptr(), 9, &mut f),
            OK
        );
        let other = IR.replace("increment", "other");
        assert_eq!(
            keel_module_merge_ir(&mut *m, other.as_ptr(), other.len()),
            OK
        );
        assert_eq!(m.ir.functions.len(), 2);
        let current = m.owner;
        let conflict = IR.replace("iconst.i64 1", "iconst.i64 2");
        assert_eq!(
            keel_module_merge_ir(&mut *m, conflict.as_ptr(), conflict.len()),
            INVALID
        );
        assert_eq!(m.owner, current);
        assert_eq!(m.ir.functions.len(), 2);
        let mut symbol = KeelSymbol { owner: 0, index: 0 };
        assert_eq!(
            keel_module_find_symbol(&mut *m, b"other".as_ptr(), 5, &mut symbol),
            OK
        );
    }
}

#[test]
fn numeric_tags_and_invalid_operations_do_not_mutate_ir() {
    let (mut m, b) = builder();
    let mut int = empty_value();
    let mut float = int;
    let mut out = int;
    // SAFETY: All input/output pointers refer to live, disjoint storage.
    unsafe {
        assert_eq!(keel_build_iconst(&mut *m, b, 4, 7, &mut int), OK);
        assert_eq!(
            keel_build_fconst(&mut *m, b, 8, 1.5f64.to_bits(), &mut float),
            OK
        );
        let before = m.ir.functions[0].num_insts();
        assert_eq!(
            keel_build_icmp(&mut *m, b, 1, float, float, &mut out),
            INVALID
        );
        assert_eq!(keel_build_fcmp(&mut *m, b, 1, int, int, &mut out), INVALID);
        assert_eq!(
            keel_build_binary(&mut *m, b, 1, float, float, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_binary(&mut *m, b, 20, int, int, &mut out),
            INVALID
        );
        assert_eq!(keel_build_unary(&mut *m, b, 9, int, &mut out), INVALID);
        assert_eq!(keel_build_fconst(&mut *m, b, 4, 0, &mut out), INVALID);
        assert_eq!(
            keel_build_overflow(&mut *m, b, 2, int, int, &mut out, 1),
            INVALID
        );
        assert_eq!(
            keel_build_select(&mut *m, b, float, int, int, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_convert(&mut *m, b, 8, 1, float, &mut out),
            INVALID
        );
        assert_eq!(m.ir.functions[0].num_insts(), before);
        let mut kind = 0;
        assert_eq!(keel_type_vector(&mut *m, 3, 3, &mut kind), INVALID);
        assert_eq!(keel_type_vector(&mut *m, 3, 4, &mut kind), OK);
        assert_eq!(keel_build_vsplat(&mut *m, b, kind, int, &mut out), INVALID);
        let mut i32 = empty_value();
        assert_eq!(keel_build_iconst(&mut *m, b, 3, 1, &mut i32), OK);
        assert_eq!(keel_build_vsplat(&mut *m, b, kind, i32, &mut out), OK);
        assert_eq!(keel_build_vextract(&mut *m, b, out, 4, &mut int), INVALID);
        assert_eq!(
            keel_build_vshuffle(&mut *m, b, out, out, [0u8, 1, 2, 9].as_ptr(), 4, &mut int),
            INVALID
        );
    }
}

#[test]
fn layouts_and_aggregate_signatures_validate_before_allocation() {
    let mut m = module();
    let mut other = module();
    let mut i8 = KeelLayout { owner: 0, index: 0 };
    let mut i64 = i8;
    let mut record = i8;
    // SAFETY: Inputs/outputs are live, aligned and disjoint from module storage.
    unsafe {
        assert_eq!(keel_layout_scalar(&mut *m, 1, &mut i8), OK);
        assert_eq!(keel_layout_scalar(&mut *m, 4, &mut i64), OK);
        assert_eq!(
            keel_layout_struct(&mut *m, [i8, i64].as_ptr(), 2, &mut record),
            OK
        );
        let mut size = 0;
        assert_eq!(keel_layout_size(&mut *m, record, &mut size), OK);
        assert_eq!(size, 16);
        assert_eq!(keel_layout_field_offset(&mut *m, record, 1, &mut size), OK);
        assert_eq!(size, 8);
        assert_eq!(
            keel_layout_field_offset(&mut *m, record, 2, &mut size),
            INVALID
        );
        assert_eq!(keel_layout_size(&mut *other, record, &mut size), INVALID);
        let mut huge = i64;
        assert_eq!(keel_layout_array(&mut *m, i64, u32::MAX, &mut huge), OK);
        let old = huge;
        let before = m.layouts.len();
        assert_eq!(
            keel_layout_array(&mut *m, huge, u32::MAX, &mut huge),
            INVALID
        );
        assert_eq!(huge.index, old.index);
        assert_eq!(m.layouts.len(), before);
        let normal = KeelAbiValue {
            kind: 4,
            extension: 0,
            purpose: 0,
            layout: record,
        };
        let sret = KeelAbiValue {
            kind: 5,
            extension: 0,
            purpose: 2,
            layout: record,
        };
        let options = KeelSignatureOptions {
            convention: 0,
            flags: 0,
            x86_vector_abi: 0,
        };
        let mut signature = KeelSignature { owner: 0, index: 0 };
        assert_eq!(
            keel_signature_create_full(&mut *m, &sret, 1, &normal, 1, options, &mut signature),
            INVALID
        );
        assert_eq!(
            keel_signature_create_full(
                &mut *m,
                [normal, sret].as_ptr(),
                2,
                ptr::null(),
                0,
                options,
                &mut signature
            ),
            INVALID
        );
        assert_eq!(
            keel_signature_create_full(&mut *m, &sret, 1, ptr::null(), 0, options, &mut signature),
            OK
        );
    }
}

#[test]
fn invalid_atomic_orders_and_intrinsics_leave_instruction_stream_unchanged() {
    let (mut m, b) = builder();
    let mut zero = empty_value();
    let mut pointer = zero;
    let mut out = zero;
    // SAFETY: Raw pointers refer to live, nonoverlapping inputs and outputs.
    unsafe {
        assert_eq!(keel_build_iconst(&mut *m, b, 4, 0, &mut zero), OK);
        assert_eq!(keel_build_convert(&mut *m, b, 5, 5, zero, &mut pointer), OK);
        let before = m.ir.functions[0].num_insts();
        assert_eq!(
            keel_build_atomic_load(&mut *m, b, 4, 2, pointer, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_atomic_store(&mut *m, b, 1, zero, pointer),
            INVALID
        );
        assert_eq!(
            keel_build_atomic_rmw(&mut *m, b, 999, 0, pointer, zero, &mut out),
            INVALID
        );
        assert_eq!(keel_build_fence(&mut *m, b, 0), INVALID);
        let mut results = [out, out];
        assert_eq!(
            keel_build_atomic_cas(
                &mut *m,
                b,
                4,
                2,
                0,
                pointer,
                zero,
                zero,
                results.as_mut_ptr(),
                2
            ),
            INVALID
        );
        assert_eq!(
            keel_build_atomic_cas(
                &mut *m,
                b,
                4,
                1,
                9,
                pointer,
                zero,
                zero,
                results.as_mut_ptr(),
                2
            ),
            INVALID
        );
        assert_eq!(
            super::advanced::keel_build_intrinsic(
                &mut *m,
                b,
                b"memcpy".as_ptr(),
                6,
                &zero,
                1,
                ptr::null_mut(),
                0
            ),
            INVALID
        );
        assert_eq!(
            super::advanced::keel_build_intrinsic(
                &mut *m,
                b,
                b"unknown".as_ptr(),
                7,
                ptr::null(),
                0,
                ptr::null_mut(),
                0
            ),
            INVALID
        );
        assert_eq!(m.ir.functions[0].num_insts(), before);
    }
}

#[test]
fn native_policies_source_locations_and_core_backend_limits_are_visible() {
    let mut m = module();
    // SAFETY: Borrowed text and all outputs are live and disjoint from module storage.
    unsafe {
        assert_eq!(keel_module_load_ir(&mut *m, IR.as_ptr(), IR.len()), OK);
        let mut f = KeelFunction { owner: 0, index: 0 };
        let mut b = KeelBlock {
            owner: 0,
            function: 0,
            index: 0,
        };
        assert_eq!(
            keel_module_find_function(&mut *m, b"increment".as_ptr(), 9, &mut f),
            OK
        );
        assert_eq!(keel_function_block_at(&mut *m, f, 0, &mut b), OK);
        let mut file = KeelSourceFile { owner: 0, index: 0 };
        assert_eq!(
            keel_source_file_add(&mut *m, b"test.tern".as_ptr(), 9, &mut file),
            OK
        );
        assert_eq!(keel_instruction_set_source(&mut *m, b, 0, file, 10, 2), OK);
        let mut output = ptr::null_mut();
        let bad = KeelCodegenOptions {
            selection: 99,
            ..Default::default()
        };
        assert_eq!(
            keel_module_emit_object_with_options(&mut *m, 1, bad, &mut output),
            INVALID
        );
        assert!(output.is_null());
        let options = KeelCodegenOptions {
            selection: 1,
            allocation: 1,
            unwind: 1,
            flags: 3,
            ..Default::default()
        };
        assert_eq!(
            keel_module_emit_assembly(&mut *m, 1, options, &mut output),
            OK
        );
        assert!(!(*output).0.is_empty());
        keel_buffer_destroy(output);
        output = ptr::null_mut();
        assert_eq!(
            keel_module_emit_object_with_options(&mut *m, 1, options, &mut output),
            OK
        );
        keel_buffer_destroy(output);
        output = ptr::null_mut();
        m.ir.functions[0].attrs.interrupt = true;
        assert_eq!(keel_module_emit_object(&mut *m, 1, &mut output), COMPILE);
        assert!(m.error.to_string_lossy().contains("interrupt"));
        assert!(output.is_null());
    }
}

#[test]
fn function_attributes_stay_consistent_before_and_after_call_import() {
    let mut m = module();
    // SAFETY: All inputs/outputs are live and disjoint; IDs are API-created.
    unsafe {
        assert_eq!(keel_module_load_ir(&mut *m, IR.as_ptr(), IR.len()), OK);
        let mut symbol = KeelSymbol { owner: 0, index: 0 };
        assert_eq!(
            keel_module_find_symbol(&mut *m, b"increment".as_ptr(), 9, &mut symbol),
            OK
        );
        assert_eq!(
            super::advanced::keel_symbol_set_function_attributes(&mut *m, symbol, 1 << 9, 16),
            OK
        );
        let mut f = KeelFunction { owner: 0, index: 0 };
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"caller".as_ptr(),
                6,
                [4u32].as_ptr(),
                1,
                [4u32].as_ptr(),
                1,
                &mut f
            ),
            OK
        );
        let mut b = KeelBlock {
            owner: 0,
            function: 0,
            index: 0,
        };
        assert_eq!(keel_block_create(&mut *m, f, &mut b), OK);
        let mut argument = empty_value();
        let mut result = argument;
        assert_eq!(keel_block_add_parameter(&mut *m, b, 4, &mut argument), OK);
        assert_eq!(
            super::systems::keel_build_call(&mut *m, b, symbol, &argument, 1, &mut result, 1),
            OK
        );
        assert_eq!(keel_build_return(&mut *m, b, &result, 1), OK);
        assert_eq!(keel_module_verify(&mut *m), OK);
        assert_eq!(
            super::advanced::keel_symbol_set_function_attributes(&mut *m, symbol, 1 << 12, 0),
            OK
        );
        assert_eq!(keel_module_verify(&mut *m), OK);
        let mut target = KeelFunction { owner: 0, index: 0 };
        assert_eq!(
            keel_module_find_function(&mut *m, b"increment".as_ptr(), 9, &mut target),
            OK
        );
        assert_eq!(
            keel_function_set_target_features(&mut *m, target, b"avx2".as_ptr(), 4),
            OK
        );
        assert_eq!(keel_module_verify(&mut *m), OK);
        assert_eq!(
            super::advanced::keel_symbol_set_function_attributes(&mut *m, symbol, 1 << 60, 0),
            INVALID
        );
        assert_eq!(keel_module_verify(&mut *m), OK);
    }
}

#[test]
fn x86_feature_permissions_reach_the_object_assembler() {
    let mut m = module();
    let mut f = Function::new("vector", Signature::new(&[], &[Type::I32]));
    let block = f.create_block();
    m.ir.functions.push(f);
    let f = KeelFunction {
        owner: m.owner,
        index: 0,
    };
    let b = KeelBlock {
        owner: m.owner,
        function: 0,
        index: block.index() as u32,
    };
    // SAFETY: API module and every output are live, aligned and disjoint.
    unsafe {
        let mut scalar = empty_value();
        let mut vector = scalar;
        let mut result = scalar;
        assert_eq!(keel_build_iconst(&mut *m, b, 3, 2, &mut scalar), OK);
        let mut kind = 0;
        assert_eq!(keel_type_vector(&mut *m, 3, 8, &mut kind), OK);
        assert_eq!(keel_build_vsplat(&mut *m, b, kind, scalar, &mut vector), OK);
        assert_eq!(
            keel_build_vector_binary(
                &mut *m,
                b,
                b"viadd".as_ptr(),
                5,
                vector,
                vector,
                &mut result
            ),
            OK
        );
        assert_eq!(
            keel_build_vector_reduce(&mut *m, b, 0, result, &mut scalar),
            OK
        );
        assert_eq!(keel_build_return(&mut *m, b, &scalar, 1), OK);
        let mut output = ptr::null_mut();
        assert_eq!(keel_module_emit_object(&mut *m, 3, &mut output), COMPILE);
        assert!(output.is_null());
        assert_eq!(
            keel_function_set_target_features(&mut *m, f, b"avx2".as_ptr(), 4),
            OK
        );
        assert_eq!(
            keel_module_emit_object(&mut *m, 3, &mut output),
            OK,
            "{:?}",
            m.error
        );
        assert!((*output).0.starts_with(b"\x7fELF"));
        keel_buffer_destroy(output);
    }
}

#[test]
fn default_optimization_selects_same_destination_arguments() {
    let src = "func @max_add(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v3 = icmp.sgt v0, v1
 brif v3, block1(v0), block1(v1)
block1(v2: i64):
 v4 = iconst.i64 7
 v5 = iadd v2, v4
 return v5
}";
    let mut m = module();
    // SAFETY: The module, text, and disjoint output slot remain live throughout.
    unsafe {
        assert_eq!(keel_module_load_ir(&mut *m, src.as_ptr(), src.len()), OK);
        assert_eq!(keel_module_optimize(&mut *m), OK);
        let mut buffer = ptr::null_mut();
        assert_eq!(keel_module_emit_ir(&mut *m, &mut buffer), OK);
        let ir = String::from_utf8_lossy(&(*buffer).0);
        assert!(!ir.contains("brif"), "{ir}");
        assert!(ir.contains("select"), "{ir}");
        keel_buffer_destroy(buffer);
        for unwind in [0, 1] {
            let options = KeelCodegenOptions {
                unwind,
                ..Default::default()
            };
            assert_eq!(
                keel_module_emit_assembly(&mut *m, 1, options, &mut buffer),
                OK
            );
            let assembly = String::from_utf8_lossy(&(*buffer).0);
            assert!(assembly.contains("csel"), "{assembly}");
            assert!(!assembly.contains("b.gt"), "{assembly}");
            if unwind == 1 {
                assert!(!assembly.contains("stp x29"), "{assembly}");
            }
            keel_buffer_destroy(buffer);
        }
    }
}
