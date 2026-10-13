//! SSA construction tests: build small programs the way a frontend would,
//! with mutable variables, and check the IR that comes out.

use keel_frontend::FunctionBuilder;
use keel_ir::{CallConv, Function, IntCC, Signature, Type, verify};

fn sig(params: &[Type], returns: &[Type]) -> Signature {
    Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        cc: CallConv::C,
        variadic: false,
    }
}

/// ```c
/// i64 sum(i64 n) { i64 i = 0, acc = 0; while (i < n) { acc += i; i += 1; } return acc; }
/// ```
#[test]
fn while_loop() {
    let mut func = Function::new("sum", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let (entry, header, body, exit) = (
        b.create_block(),
        b.create_block(),
        b.create_block(),
        b.create_block(),
    );
    let (n, i, acc) = (
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
    );

    b.switch_to_block(entry);
    b.seal_block(entry);
    let arg = b.func.append_block_param(entry, Type::I64);
    b.def_var(n, arg);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(i, zero);
    b.def_var(acc, zero);
    b.ins().jump(header, &[]);

    // The back edge is not built yet, so the header stays unsealed.
    b.switch_to_block(header);
    let (iv, nv) = (b.use_var(i), b.use_var(n));
    let cond = b.ins().icmp(IntCC::Slt, iv, nv);
    b.ins().brif(cond, body, &[], exit, &[]);

    b.switch_to_block(body);
    b.seal_block(body);
    let (accv, iv) = (b.use_var(acc), b.use_var(i));
    let acc2 = b.ins().iadd(accv, iv);
    b.def_var(acc, acc2);
    let one = b.ins().iconst(Type::I64, 1);
    let i2 = b.ins().iadd(iv, one);
    b.def_var(i, i2);
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);
    let result = b.use_var(acc);
    b.ins().return_(&[result]);
    b.finalize();

    verify(&func).unwrap();
    // `n` got a header parameter while the header was unsealed; both
    // edges pass the same `v0`, so it was trivial and removed. `acc`'s
    // header parameter (v5) is created when the body reads it.
    assert_eq!(
        func.to_string(),
        "\
func @sum(i64) -> i64 cc(c) {
block0(v0: i64):
    v1 = iconst.i64 0
    jump block1(v1, v1)
block1(v2: i64, v5: i64):
    v4 = icmp.slt v2, v0
    brif v4, block2, block3
block2:
    v6 = iadd v5, v2
    v7 = iconst.i64 1
    v8 = iadd v2, v7
    jump block1(v8, v6)
block3:
    return v5
}
"
    );
}

/// ```c
/// i64 f(i64 x) { i64 y = 1; if (x) y = 2; else x = x + 1; return x + y; }
/// ```
#[test]
fn if_else_merges_both_variables() {
    let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let (entry, then_b, else_b, merge) = (
        b.create_block(),
        b.create_block(),
        b.create_block(),
        b.create_block(),
    );
    let (x, y) = (b.declare_var(Type::I64), b.declare_var(Type::I64));

    b.switch_to_block(entry);
    b.seal_block(entry);
    let arg = b.func.append_block_param(entry, Type::I64);
    b.def_var(x, arg);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(y, one);
    let xv = b.use_var(x);
    b.ins().brif(xv, then_b, &[], else_b, &[]);

    b.switch_to_block(then_b);
    b.seal_block(then_b);
    let two = b.ins().iconst(Type::I64, 2);
    b.def_var(y, two);
    b.ins().jump(merge, &[]);

    b.switch_to_block(else_b);
    b.seal_block(else_b);
    let xv = b.use_var(x);
    let x1 = b.ins().iadd(xv, one);
    b.def_var(x, x1);
    b.ins().jump(merge, &[]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    let (xv, yv) = (b.use_var(x), b.use_var(y));
    let sum = b.ins().iadd(xv, yv);
    b.ins().return_(&[sum]);
    b.finalize();

    verify(&func).unwrap();
    assert_eq!(
        func.to_string(),
        "\
func @f(i64) -> i64 cc(c) {
block0(v0: i64):
    v1 = iconst.i64 1
    brif v0, block1, block2
block1:
    v2 = iconst.i64 2
    jump block3(v0, v2)
block2:
    v3 = iadd v0, v1
    jump block3(v3, v1)
block3(v4: i64, v5: i64):
    v6 = iadd v4, v5
    return v6
}
"
    );
}

/// Nested loops that only read `n`. Sealing each block as soon as its
/// predecessors are known removes every trivial parameter on the spot.
#[test]
fn nested_loops_sealed_early() {
    nested_loops(false);
}

/// The same, sealing everything at the end. Sealing the outer header gives
/// `n` a parameter on the still-unsealed latch, so the outer header's
/// parameter (args `v0` and the latch's) is not trivial yet. The latch's
/// becomes trivial later, which makes the outer header's trivial too; only
/// `finalize` sees that.
#[test]
fn nested_loops_sealed_at_end() {
    nested_loops(true);
}

fn nested_loops(seal_at_end: bool) {
    let seal = |b: &mut FunctionBuilder, block| {
        if !seal_at_end {
            b.seal_block(block);
        }
    };
    let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let entry = b.create_block();
    let outer = b.create_block();
    let inner = b.create_block();
    let inner_body = b.create_block();
    let outer_latch = b.create_block();
    let exit = b.create_block();
    let (n, i, j) = (
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
    );

    b.switch_to_block(entry);
    seal(&mut b, entry);
    let arg = b.func.append_block_param(entry, Type::I64);
    b.def_var(n, arg);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(i, zero);
    b.ins().jump(outer, &[]);

    // for (i = 0; i < n; i++) for (j = 0; j < n; j++) {}
    b.switch_to_block(outer);
    let (iv, nv) = (b.use_var(i), b.use_var(n));
    let c = b.ins().icmp(IntCC::Slt, iv, nv);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(j, zero);
    b.ins().brif(c, inner, &[], exit, &[]);

    b.switch_to_block(inner);
    let (jv, nv) = (b.use_var(j), b.use_var(n));
    let c = b.ins().icmp(IntCC::Slt, jv, nv);
    b.ins().brif(c, inner_body, &[], outer_latch, &[]);

    b.switch_to_block(inner_body);
    seal(&mut b, inner_body);
    let jv = b.use_var(j);
    let one = b.ins().iconst(Type::I64, 1);
    let j2 = b.ins().iadd(jv, one);
    b.def_var(j, j2);
    b.ins().jump(inner, &[]);
    seal(&mut b, inner);

    b.switch_to_block(outer_latch);
    seal(&mut b, outer_latch);
    let iv = b.use_var(i);
    let one = b.ins().iconst(Type::I64, 1);
    let i2 = b.ins().iadd(iv, one);
    b.def_var(i, i2);
    b.ins().jump(outer, &[]);
    seal(&mut b, outer);

    b.switch_to_block(exit);
    seal(&mut b, exit);
    let result = b.use_var(i);
    b.ins().return_(&[result]);
    if seal_at_end {
        b.seal_all_blocks();
    }
    b.finalize();

    verify(&func).unwrap();
    let text = func.to_string();
    // Only i (outer) and j (inner) are loop-carried; n is v0 everywhere.
    assert!(text.contains("block1(v2: i64):"), "{text}");
    assert!(text.contains("block2(v6: i64):"), "{text}");
    assert!(text.contains("icmp.slt v2, v0"), "{text}");
    assert!(text.contains("icmp.slt v6, v0"), "{text}");
}

#[test]
#[should_panic(expected = "used in block0 before it is defined")]
fn use_before_definition_panics() {
    let mut func = Function::new("f", sig(&[], &[]));
    let mut b = FunctionBuilder::new(&mut func);
    let entry = b.create_block();
    b.switch_to_block(entry);
    b.seal_block(entry);
    let x = b.declare_var(Type::I64);
    b.use_var(x);
}
