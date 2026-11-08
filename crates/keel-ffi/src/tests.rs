use super::*;
use std::ptr;

fn module() -> Box<KeelModule> {
    // SAFETY: A fresh API allocation is transferred immediately into one Box.
    unsafe { Box::from_raw(keel_module_create()) }
}

fn function(m: &mut KeelModule, name: &str, params: &[u32], returns: &[u32]) -> KeelFunction {
    let mut f = KeelFunction { owner: 0, index: 0 };
    // SAFETY: All pointers refer to live, nonoverlapping Rust allocations.
    let status = unsafe {
        keel_module_add_function(
            m,
            name.as_ptr(),
            name.len(),
            params.as_ptr(),
            params.len(),
            returns.as_ptr(),
            returns.len(),
            &mut f,
        )
    };
    assert_eq!(status, OK, "add function: {:?}", m.error);
    f
}

fn block(m: &mut KeelModule, f: KeelFunction) -> KeelBlock {
    let mut b = KeelBlock {
        owner: 0,
        function: 0,
        index: 0,
    };
    // SAFETY: Module and output are live and disjoint.
    assert_eq!(unsafe { keel_block_create(m, f, &mut b) }, OK);
    b
}

fn constant(m: &mut KeelModule, b: KeelBlock, kind: u32, n: i64) -> KeelValue {
    let mut v = KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    };
    // SAFETY: Module and output are live and disjoint.
    assert_eq!(unsafe { keel_build_iconst(m, b, kind, n, &mut v) }, OK);
    v
}

fn constant_module() -> (Box<KeelModule>, KeelFunction, KeelBlock, KeelValue) {
    let mut m = module();
    let f = function(&mut m, "answer", &[], &[4]);
    let b = block(&mut m, f);
    let v = constant(&mut m, b, 4, 42);
    // SAFETY: The one-element value input is live and disjoint from the module.
    assert_eq!(unsafe { keel_build_return(&mut *m, b, &v, 1) }, OK);
    (m, f, b, v)
}

#[test]
fn bad_inputs_leave_outputs_and_ir_unchanged() {
    let mut m = module();
    let mut f = KeelFunction {
        owner: 123,
        index: 456,
    };
    // SAFETY: Null inputs have zero lengths except the deliberately rejected
    // null/nonzero input. Output is a separate live allocation.
    unsafe {
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                ptr::null(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut f
            ),
            INVALID
        );
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"bad name".as_ptr(),
                8,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut f
            ),
            INVALID
        );
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"bad".as_ptr(),
                3,
                [99].as_ptr(),
                1,
                ptr::null(),
                0,
                &mut f
            ),
            INVALID
        );
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"bad".as_ptr(),
                3,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null_mut()
            ),
            INVALID
        );
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"narrow".as_ptr(),
                6,
                [1].as_ptr(),
                1,
                ptr::null(),
                0,
                &mut f
            ),
            INVALID
        );
        assert_eq!(
            keel_module_add_function(
                &mut *m,
                b"narrow".as_ptr(),
                6,
                ptr::null(),
                0,
                [2].as_ptr(),
                1,
                &mut f
            ),
            INVALID
        );
        assert_eq!(keel_module_verify(ptr::null_mut()), INVALID);
    }
    assert_eq!(
        (f.owner, f.index),
        (123, 456),
        "failed calls must not write outputs"
    );
    assert!(
        m.ir.functions.is_empty(),
        "failed calls must not add functions"
    );
    assert!(
        !m.error.is_empty(),
        "invalid arguments must have a diagnostic"
    );
    function(&mut m, "good", &[], &[]);
    assert!(m.error.is_empty(), "success must clear the diagnostic");
}

#[test]
fn rejects_foreign_stale_and_out_of_range_ids() {
    let (mut m, f, b, v) = constant_module();
    let mut other = module();
    let other_f = function(&mut other, "other", &[], &[4]);
    let other_b = block(&mut other, other_f);
    let mut result = v;
    let mut destination = b;
    // SAFETY: All pointers are live and disjoint. IDs are deliberately invalid,
    // which the API must reject before indexing any Rust arena.
    unsafe {
        assert_eq!(keel_block_create(&mut *other, f, &mut destination), INVALID);
        assert_eq!(
            keel_build_binary(&mut *other, other_b, 1, v, v, &mut result),
            INVALID
        );
        let second_f = function(&mut m, "second", &[], &[4]);
        let second_b = block(&mut m, second_f);
        assert_eq!(
            keel_build_binary(&mut *m, second_b, 1, v, v, &mut result),
            INVALID
        );
        let invalid = KeelValue {
            owner: m.owner,
            function: second_f.index,
            index: u32::MAX,
        };
        assert_eq!(
            keel_build_binary(&mut *m, second_b, 1, invalid, invalid, &mut result),
            INVALID
        );
        assert_eq!(
            keel_build_jump(&mut *m, second_b, b, ptr::null(), 0),
            INVALID
        );
        let invalid = KeelBlock {
            index: u32::MAX,
            ..second_b
        };
        assert_eq!(
            keel_build_iconst(&mut *m, invalid, 4, 1, &mut result),
            INVALID
        );
    }
    let (mut m, f, b, _) = constant_module();
    // SAFETY: The module and outputs are live. Optimization makes old IDs stale.
    unsafe {
        assert_eq!(keel_module_optimize(&mut *m), OK);
        assert_eq!(keel_block_create(&mut *m, f, &mut destination), INVALID);
        assert_eq!(keel_build_iconst(&mut *m, b, 4, 1, &mut result), INVALID);
        assert_eq!(keel_module_verify(&mut *m), OK);
    }
}

#[test]
fn type_and_control_flow_errors_are_recoverable() {
    let mut m = module();
    let f = function(&mut m, "typed", &[4], &[4]);
    let entry = block(&mut m, f);
    let dest = block(&mut m, f);
    let mut param = KeelValue {
        owner: 0,
        function: 0,
        index: 0,
    };
    // SAFETY: Live, separate module/input/output allocations; invalid tags,
    // widths and arities are part of the checked API rather than pointer misuse.
    unsafe {
        assert_eq!(
            keel_block_add_parameter(&mut *m, entry, 3, &mut param),
            INVALID
        );
        assert_eq!(keel_block_add_parameter(&mut *m, entry, 4, &mut param), OK);
        let a = constant(&mut m, entry, 4, 7);
        let narrow = constant(&mut m, entry, 1, 1);
        let mut out = param;
        assert_eq!(
            keel_build_binary(&mut *m, entry, 1, a, narrow, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_binary(&mut *m, entry, 999, a, a, &mut out),
            INVALID
        );
        assert_eq!(
            keel_build_icmp(&mut *m, entry, 999, a, a, &mut out),
            INVALID
        );
        assert_eq!(keel_build_return(&mut *m, entry, &narrow, 1), INVALID);
        assert_eq!(keel_build_return(&mut *m, entry, ptr::null(), 0), INVALID);
        assert_eq!(
            keel_block_add_parameter(&mut *m, entry, 4, &mut out),
            INVALID
        );
        assert_eq!(keel_block_add_parameter(&mut *m, dest, 4, &mut out), OK);
        assert_eq!(
            keel_build_jump(&mut *m, entry, dest, ptr::null(), 0),
            INVALID
        );
        assert_eq!(
            keel_build_brif(&mut *m, entry, a, dest, &a, 1, dest, &a, 1),
            INVALID
        );
        assert_eq!(keel_build_jump(&mut *m, entry, dest, &a, 1), OK);
        assert_eq!(keel_build_return(&mut *m, entry, &a, 1), INVALID);
        assert_eq!(keel_build_return(&mut *m, dest, &out, 1), OK);
        assert_eq!(keel_module_verify(&mut *m), OK);
    }
}

#[test]
fn verification_checks_dominance_and_preserves_ids_on_failure() {
    let mut m = module();
    let f = function(&mut m, "dominance", &[], &[4]);
    let entry = block(&mut m, f);
    let left = block(&mut m, f);
    let right = block(&mut m, f);
    let condition = constant(&mut m, entry, 1, 1);
    let left_value = constant(&mut m, left, 4, 2);
    // SAFETY: All allocations are live. The IR violates dominance, which must
    // be reported by verification without a panic or an output allocation.
    unsafe {
        assert_eq!(
            keel_build_brif(
                &mut *m,
                entry,
                condition,
                left,
                ptr::null(),
                0,
                right,
                ptr::null(),
                0
            ),
            OK
        );
        assert_eq!(keel_build_return(&mut *m, left, &left_value, 1), OK);
        assert_eq!(keel_build_return(&mut *m, right, &left_value, 1), OK);
        assert_eq!(keel_module_verify(&mut *m), VERIFY);
        assert!(!m.error.is_empty());
        let owner = m.owner;
        assert_eq!(keel_module_optimize(&mut *m), VERIFY);
        assert_eq!(m.owner, owner, "verification failure must preserve IDs");
        let mut output = ptr::null_mut();
        assert_eq!(keel_module_emit_object(&mut *m, 3, &mut output), VERIFY);
        assert!(output.is_null());
    }
}

#[test]
fn emits_native_objects_and_buffers_outlive_modules() {
    use object::{Object, ObjectSymbol};
    let (mut m, _, _, _) = constant_module();
    let mut ir = ptr::null_mut();
    // SAFETY: The module and output allocations are live and disjoint. Each
    // returned buffer remains live throughout inspection and is freed once.
    unsafe {
        assert_eq!(keel_module_emit_ir(&mut *m, &mut ir), OK);
        assert_eq!(
            keel_module_emit_object(&mut *m, 999, &mut ptr::null_mut()),
            INVALID
        );
        for (target, format, arch) in [
            (
                1,
                object::BinaryFormat::MachO,
                object::Architecture::Aarch64,
            ),
            (2, object::BinaryFormat::Elf, object::Architecture::Aarch64),
            (3, object::BinaryFormat::Elf, object::Architecture::X86_64),
        ] {
            let mut output = ptr::null_mut();
            assert_eq!(
                keel_module_emit_object(&mut *m, target, &mut output),
                OK,
                "{:?}",
                m.error
            );
            let bytes =
                std::slice::from_raw_parts(keel_buffer_data(output), keel_buffer_size(output));
            let file = object::File::parse(bytes).expect("valid native object");
            assert_eq!(file.format(), format);
            assert_eq!(file.architecture(), arch);
            assert!(
                file.symbols()
                    .any(|s| s.is_definition() && matches!(s.name(), Ok("answer" | "_answer")))
            );
            keel_buffer_destroy(output);
        }
        drop(m);
        let text = std::slice::from_raw_parts(keel_buffer_data(ir), keel_buffer_size(ir));
        assert!(
            std::str::from_utf8(text)
                .expect("UTF-8 IR")
                .contains("answer")
        );
        keel_buffer_destroy(ir);
        keel_buffer_destroy(ptr::null_mut());
        keel_module_destroy(ptr::null_mut());
        assert!(keel_buffer_data(ptr::null()).is_null());
        assert_eq!(keel_buffer_size(ptr::null()), 0);
        assert!(keel_module_error(ptr::null()).is_null());
    }
}

#[test]
fn unexpected_panic_poisons_module_without_unwinding_to_c() {
    let mut m = module();
    // SAFETY: The live module is exclusively accessed in every operation.
    unsafe {
        assert_eq!(run(&mut *m, |_| panic!("injected boundary failure")), PANIC);
        let message = m.error.clone();
        assert_eq!(keel_module_verify(&mut *m), POISONED);
        assert_eq!(
            m.error, message,
            "poison must preserve the original diagnostic"
        );
        assert!(!keel_module_error(&*m).is_null());
    }
}
