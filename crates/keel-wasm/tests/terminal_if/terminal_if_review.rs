use super::{bodies, emit, execute};
use keel_ir::parse;
use wasmparser::Operator;

// Independent review: the false arm executes a retained faulting producer,
// whose resolved replacement is an earlier arm-local definition via a
// detached alias. The true arm must never execute this division.
#[test]
fn false_arm_retained_division_and_detached_arm_local_alias() {
    let mut module = parse(
        r#"
func @choose(i32,i32)->i32 {
block0(v0:i32,v1:i32):
 brif v0,block2,block1
block1:
 v2=iconst.i32 -2147483648
 v3=iconst.i32 17
 v4=sdiv v2,v1
 return v4
block2:
 return v1
}
"#,
    )
    .unwrap();
    let f = &mut module.functions[0];
    let arm = f.blocks().nth(1).unwrap();
    let insts = f.block_insts(arm).to_vec();
    f.replace_all_uses(f.first_result(insts[1]), f.first_result(insts[0]));
    f.replace_all_uses(f.first_result(insts[2]), f.first_result(insts[1]));
    f.remove_inst(insts[1]);
    for profile in 0..3 {
        let bytes = emit(&module, &["choose"], profile).unwrap();
        let body = &bodies(&bytes)[0];
        let ops = body
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!ops.iter().any(|o| matches!(o, Operator::Loop { .. })));
        assert_eq!(
            ops.iter()
                .filter(|o| matches!(o, Operator::I32DivS))
                .count(),
            1
        );
        execute(
            &bytes,
            r#"
for (const d of [0,-1,1,2,2147483647,-2147483648]) {
 assert.equal(e.choose(1,d),d);
 assert.equal(e.choose(-1,d),d);
}
for (const d of [1,2,2147483647,-2147483648]) assert.equal(e.choose(0,d),-2147483648);
assert.throws(()=>e.choose(0,0),WebAssembly.RuntimeError);
"#,
        );
        execute(
            &bytes,
            "assert.throws(()=>e.choose(0,-1),WebAssembly.RuntimeError);",
        );
    }
}

#[test]
fn selected_arm_calls_fault_order_and_framed_caller_restoration() {
    let source = r#"
func @mark(i32)->i32 {
 ss0=stack_slot 16, align 16
block0(v0:i32):
 v1=iconst.i64 7
 v2=inttoptr.ptr v1
 store volatile v0,v2
 return v0
}
func @choose(i32,i32)->i32 {
 ss0=stack_slot 16, align 16
block0(v0:i32,v1:i32):
 v2=iconst.i32 1
 v3=call @mark(v2)
 brif v0,block2,block1
block1:
 v4=iconst.i32 3
 v5=call @mark(v4)
 v6=iconst.i32 -2147483648
 v7=sdiv v6,v1
 v8=iconst.i32 4
 v9=call @mark(v8)
 return v7
block2:
 v10=iconst.i32 2
 v11=call @mark(v10)
 return v11
}
func @address()->i64 {
 ss0=stack_slot 16, align 16
block0:
 v0=stack_addr ss0
 v1=ptrtoint.i64 v0
 return v1
}
"#;
    for profile in 1..3 {
        let fixed_source = source
            .split("func @address")
            .next()
            .unwrap()
            .replace(" ss0=stack_slot 16, align 16", "");
        let module = parse(if profile == 2 { source } else { &fixed_source }).unwrap();
        let names = if profile == 2 {
            vec!["choose", "address"]
        } else {
            vec!["choose"]
        };
        let bytes = emit(&module, &names, profile).unwrap();
        let bs = bodies(&bytes);
        assert!(
            !bs[1]
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .any(|o| matches!(o.unwrap(), Operator::Loop { .. }))
        );
        execute(
            &bytes,
            &format!(
                r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
const check=mark=>assert(u.every((x,i)=>x===(i===7?mark:i>7&&i<11?0:0xab)));
for(let n=0;n<100;n++) {{
 assert.equal(e.choose(1,0),2);check(2);
 {}
 assert.equal(e.choose(0,2),-1073741824);check(4);
 {}
}}
assert.throws(()=>e.choose(0,0),WebAssembly.RuntimeError);check(3);
"#,
                if profile == 2 {
                    "assert.equal(e.address(),65520n);"
                } else {
                    ""
                },
                if profile == 2 {
                    "assert.equal(e.address(),65520n);"
                } else {
                    ""
                }
            ),
        );
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.throws(()=>e.choose(0,-1),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?3:i>7&&i<11?0:0xab)));
"#,
        );
    }
}

#[test]
fn alias_from_entry_into_later_arm_is_rejected_before_emission() {
    let mut module = parse(
        r#"
func @bad(i32)->i32 {
block0(v0:i32):
 v1=iconst.i32 9
 v2=iadd v0,v1
 brif v0,block1,block2
block1:
 v3=iconst.i32 7
 return v3
block2:
 return v2
}
"#,
    )
    .unwrap();
    let f = &mut module.functions[0];
    let entry = f.entry_block().unwrap();
    let arm = f.blocks().nth(1).unwrap();
    f.replace_all_uses(
        f.first_result(f.block_insts(entry)[0]),
        f.first_result(f.block_insts(arm)[0]),
    );
    for profile in 0..3 {
        let error = emit(&module, &[], profile).unwrap_err().to_string();
        assert!(error.contains("not dominated"), "{error}");
    }
}
