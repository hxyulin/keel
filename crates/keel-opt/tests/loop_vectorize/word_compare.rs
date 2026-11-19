use keel_ir_interp::{Interpreter, Outcome, Val};

const SOURCE: &str = "func @compare(ptr,ptr,ptr) -> i64 {
entry(v0:ptr,v1:ptr,v2:ptr):
  v3 = icmp.ult v0,v2
  brif v3,loop(v0,v1),end(v0)
loop(v4:ptr,v5:ptr):
  v6 = load.i64 v4
  v7 = load.i64 v5
  v8 = xor v6,v7
  brif v8,mismatch,latch
latch:
  v9 = iconst.i64 8
  v10 = ptradd v4,v9
  v11 = ptradd v5,v9
  v12 = icmp.ult v10,v2
  brif v12,loop(v10,v11),end(v10)
mismatch:
  v13 = ctz v8
  v14 = iconst.i64 3
  v15 = ushr v13,v14
  v16 = ptradd v4,v15
  v17 = ptrtoint.i64 v0
  v18 = ptrtoint.i64 v16
  v19 = isub v18,v17
  return v19
end(v20:ptr):
  v21 = ptrtoint.i64 v0
  v22 = ptrtoint.i64 v20
  v23 = isub v22,v21
  return v23
}";

fn optimized(source: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["loop-unroll-words"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn word_unrolling_preserves_early_exits_and_scalar_tail() {
    let module = optimized(SOURCE);
    assert_eq!(module.to_string().matches("load.i64").count(), 10);
    let mut rerun = module.clone();
    assert!(
        !keel_opt::find_pass("loop-unroll-words")
            .unwrap()
            .run(&mut rerun.functions[0], &Default::default())
    );
    for size in [0usize, 8, 24, 32, 40, 64, 72] {
        for mismatch in 0..=size {
            let mut interp = Interpreter::new(&module);
            let a = interp.memory_mut().alloc_bytes(&vec![0; size], 8);
            // The second buffer ends after the mismatching word. Reading later
            // words on the widened path would fault.
            let b_size = ((mismatch / 8) * 8 + 8).min(size);
            let mut b_bytes = vec![0; b_size];
            if mismatch < size {
                b_bytes[mismatch] = 1;
            }
            let b = interp.memory_mut().alloc_bytes(&b_bytes, 8);
            assert_eq!(
                interp.call(
                    "compare",
                    &[Val::Ptr(a), Val::Ptr(b), Val::Ptr(a + size as u64)]
                ),
                Outcome::Return(vec![Val::I64(mismatch as u64)]),
                "{size} bytes, first mismatch at {mismatch}"
            );
        }
    }
}

#[test]
fn word_unrolling_leaves_volatile_loads_in_the_scalar_loop() {
    let source = SOURCE.replace("load.i64 v5", "load.i64 volatile v5");
    assert_eq!(
        keel_ir::parse(&source).unwrap().to_string(),
        optimized(&source).to_string()
    );
}
