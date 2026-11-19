use keel_ir_interp::{Interpreter, Outcome, Val};

const SOURCE: &str = "func @copy(ptr,ptr,ptr) -> ptr {
entry(v0:ptr,v1:ptr,v2:ptr):
  jump loop(v0,v1)
loop(v3:ptr,v4:ptr):
  v5 = load.i64 trap_ub v4
  store trap_ub v5,v3
  v6 = iconst.i64 8
  v7 = ptradd v3,v6
  v8 = ptradd v4,v6
  v9 = icmp.ult v7,v2
  brif v9,loop(v7,v8),exit
exit:
  return v7
}";

fn optimized(source: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["loop-copy"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn copy_widens_only_with_room_and_preserves_forward_overlap_and_exit_values() {
    let module = optimized(SOURCE);
    assert_eq!(module.to_string().matches("load.i8x16").count(), 2);
    let mut rerun = module.clone();
    assert!(
        !keel_opt::find_pass("loop-copy")
            .unwrap()
            .run(&mut rerun.functions[0], &Default::default())
    );
    for (offset, length) in [
        (32u64, 0u64),
        (32, 32),
        (32, 33),
        (32, 64),
        (63, 65),
        (40, 127),
        (16, 129),
        (8, 129),
    ] {
        let mut bytes = (0..256u64).map(|i| (197 * i + 7) as u8).collect::<Vec<_>>();
        let mut interp = Interpreter::new(&module);
        let base = interp.memory_mut().alloc_bytes(&bytes, 16);
        let completed = length.max(1).div_ceil(8) * 8;
        for i in 0..completed as usize {
            bytes[offset as usize + i] = bytes[i];
        }
        let dst = base + offset;
        let result = interp.call(
            "copy",
            &[Val::Ptr(dst), Val::Ptr(base), Val::Ptr(dst + length)],
        );
        assert_eq!(result, Outcome::Return(vec![Val::Ptr(dst + completed)]));
        assert_eq!(interp.memory().read(base, 256).unwrap(), bytes);
    }
}

#[test]
fn copies_with_observable_faults_or_volatile_accesses_stay_scalar() {
    for source in [
        SOURCE.replace(" trap_ub", ""),
        SOURCE.replace("load.i64 trap_ub", "load.i64 volatile"),
    ] {
        let actual = optimized(&source);
        assert_eq!(
            keel_ir::parse(&source).unwrap().to_string(),
            actual.to_string()
        );
    }
}
