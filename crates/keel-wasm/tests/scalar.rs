use keel_ir::{Module, parse};
use keel_wasm::{Export, Options, emit_module};
use std::io::Write;
use std::process::{Command, Stdio};

fn options(names: &[&str]) -> Options {
    Options {
        exports: names
            .iter()
            .map(|n| Export {
                name: n.to_string(),
                function: n.to_string(),
            })
            .collect(),
    }
}
fn execute(module: &Module, names: &[&str], assertions: &str) {
    execute_options(module, &options(names), assertions);
}
fn execute_options(module: &Module, opts: &Options, assertions: &str) {
    let bytes = emit_module(module, opts).unwrap();
    // An independent parser/validator checks the binary in addition to the
    // emitter's validation; V8 independently validates, compiles and executes.
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    let js = format!(
        r#"
const fs = require('fs');
const assert = require('assert/strict');
const bytes = fs.readFileSync(0);
assert(WebAssembly.validate(bytes));
const e = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
{assertions}
"#
    );
    let mut child = Command::new("node")
        .args(["-e", &js])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node is required for Wasm execution tests");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn run(source: &str, names: &[&str], assertions: &str) {
    execute(&parse(source).unwrap(), names, assertions);
}

#[test]
fn diamond_calls_multiple_returns_and_same_target_edges() {
    run(
        r#"
func @diamond(i32, i64, i64) -> i64 {
block0(v0: i32, v1: i64, v2: i64):
    v3 = iconst.i32 0
    v4 = icmp.ne v0, v3
    brif v4, block1, block2
block1:
    v5 = call @twice(v1)
    jump block3(v5)
block2:
    jump block3(v2)
block3(v6: i64):
    return v6
}
func @twice(i64) -> i64 {
block0(v0: i64):
    v1 = iadd v0, v0
    return v1
}
func @edges(i64, i32, i32) -> i32 {
block0(v0: i64, v1: i32, v2: i32):
    brif v0, block1(v1), block1(v2)
block1(v3: i32):
    return v3
}
func @returns(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2
block1:
    v1 = iconst.i32 99
    return v1
block2:
    v2 = iconst.i32 -77
    return v2
}
"#,
        &["diamond", "edges", "returns"],
        r#"
assert.equal(e.diamond(1, 7n, 3n), 14n);
assert.equal(e.diamond(0, 7n, 3n), 3n);
assert.equal(e.edges(0x100000000n, 11, 22), 11);
assert.equal(e.edges(0n, 11, 22), 22);
assert.equal(e.returns(1), 99); assert.equal(e.returns(0), -77);
"#,
    );
}

#[test]
fn nested_loop() {
    run(
        r#"
func @nested(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 0
    v2 = iconst.i32 1
    jump block1(v1, v1)
block1(v3: i32, v4: i32):
    v5 = icmp.slt v3, v0
    brif v5, block2(v1, v4), block5(v4)
block2(v6: i32, v7: i32):
    v8 = icmp.slt v6, v3
    brif v8, block3, block4
block3:
    v9 = iadd v6, v2
    v10 = iadd v7, v2
    jump block2(v9, v10)
block4:
    v11 = iadd v3, v2
    jump block1(v11, v7)
block5(v12: i32):
    return v12
}
"#,
        &["nested"],
        "for(let n=0;n<12;n++) assert.equal(e.nested(n),(n*(n-1)/2)|0);",
    );
}

#[test]
fn irreducible_two_entry_loop() {
    run(
        r#"
func @irreducible(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iconst.i32 1
    v3 = iconst.i32 0
    brif v0, block1(v1, v3), block2(v1, v3)
block1(v4: i32, v5: i32):
    v6 = icmp.sgt v4, v3
    brif v6, block3, block5(v5)
block3:
    v7 = isub v4, v2
    v8 = iadd v5, v2
    jump block2(v7, v8)
block2(v9: i32, v10: i32):
    v11 = icmp.sgt v9, v3
    brif v11, block4, block5(v10)
block4:
    v12 = isub v9, v2
    v13 = iadd v10, v2
    jump block1(v12, v13)
block5(v14: i32):
    return v14
}
"#,
        &["irreducible"],
        "for(let n=0;n<15;n++) for(let b=0;b<2;b++) assert.equal(e.irreducible(b,n),n);",
    );
}

#[test]
fn self_edge_parallel_cyclic_copies() {
    run(
        r#"
func @swap(i32, i64, i64) -> i64 {
block0(v0: i32, v1: i64, v2: i64):
    v3 = iconst.i32 0
    v4 = iconst.i32 1
    jump block1(v0, v1, v2)
block1(v5: i32, v6: i64, v7: i64):
    v8 = isub v5, v4
    v9 = icmp.sgt v5, v3
    brif v9, block1(v8, v7, v6), block2(v6)
block2(v10: i64):
    return v10
}
"#,
        &["swap"],
        "for(let n=0;n<10;n++) assert.equal(e.swap(n,11n,22n),n%2?22n:11n);",
    );
}

#[test]
fn switch_default_and_duplicate_destinations() {
    run(
        r#"
func @switcher(i64, i32, i32) -> i32 {
block0(v0: i64, v1: i32, v2: i32):
    switch v0, block1(v1), [0: block1(v2), -1: block2, 4294967296: block1(v2)]
block1(v3: i32):
    return v3
block2:
    v4 = iconst.i32 77
    return v4
}
"#,
        &["switcher"],
        r#"
assert.equal(e.switcher(0n,11,22),22); assert.equal(e.switcher(-1n,11,22),77);
assert.equal(e.switcher(4294967296n,11,22),22); assert.equal(e.switcher(7n,11,22),11);
"#,
    );
}

#[test]
fn aliases_and_removed_definitions() {
    let mut m = parse(
        r#"
func @alias(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 99
    v2 = iconst.i32 100
    v3 = iadd v1, v2
    return v3
}
"#,
    )
    .unwrap();
    let f = &mut m.functions[0];
    let b = f.entry_block().unwrap();
    let insts = f.block_insts(b).to_vec();
    let param = f.block_params(b)[0];
    f.replace_all_uses(f.first_result(insts[0]), param);
    f.replace_all_uses(f.first_result(insts[1]), param);
    f.remove_inst(insts[1]);
    execute(
        &m,
        &["alias"],
        "assert.equal(e.alias(7),14); assert.equal(e.alias(-9),-18);",
    );
}

#[test]
fn arithmetic_extrema_and_comparisons() {
    let mut source = String::new();
    let mut names = Vec::new();
    let mut js = String::new();
    for width in [32, 64] {
        for op in [
            "iadd", "isub", "imul", "udiv", "sdiv", "urem", "srem", "and", "or", "xor", "shl",
            "ushr", "sshr", "rotl", "rotr",
        ] {
            let name = format!("{op}{width}");
            source += &format!(
                "func @{name}(i{width}, i{width}) -> i{width} {{\nblock0(v0: i{width}, v1: i{width}):\n v2 = {op} v0, v1\n return v2\n}}\n"
            );
            names.push(name);
        }
        for cc in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            let name = format!("{cc}{width}");
            // Comparison i8 remains internal; select exposes a plain i32 result.
            source += &format!(
                "func @{name}(i{width}, i{width}) -> i32 {{\nblock0(v0: i{width}, v1: i{width}):\n v2 = icmp.{cc} v0, v1\n v3 = iconst.i32 1\n v4 = iconst.i32 0\n v5 = select v2, v3, v4\n return v5\n}}\n"
            );
            names.push(name);
        }
        js += &format!(
            r#"
{{
const w={width}, W=BigInt(w), mask=(1n<<W)-1n;
const s=x=>BigInt.asIntN(w,x), u=x=>x&mask;
const values=[0n,1n,-1n,1n<<(W-1n),(1n<<(W-1n))-1n,0x12345678n,-7n,W,W+1n];
// Deterministic full-width patterns supplement the semantic boundary pairs.
let seed=0xd1b54a32d192ed03n;
for(let i=0;i<24;i++) {{
 seed=BigInt.asUintN(64,seed*6364136223846793005n+1442695040888963407n);
 values.push(s(seed));
}}
const call=(op,a,b)=>BigInt(e[op+w](...(w===64?[s(a),s(b)]:[Number(s(a)),Number(s(b))])));
for(const a of values) for(const b of values) {{
 const A=s(a),B=s(b),U=u(a),V=u(b),k=V%W;
 const expected={{iadd:A+B,isub:A-B,imul:A*B,and:U&V,or:U|V,xor:U^V,shl:U<<k,ushr:U>>k,sshr:A>>k,
 rotl:(U<<k)|(U>>((W-k)%W)),rotr:(U>>k)|(U<<((W-k)%W))}};
 if(V) {{expected.udiv=U/V; expected.urem=U%V;}}
 if(B && !(A===-(1n<<(W-1n)) && B===-1n)) expected.sdiv=A/B;
 if(B) expected.srem=A%B;
 for(const [op,x] of Object.entries(expected)) assert.equal(call(op,a,b),s(x),`${{op}}${{w}} ${{a}},${{b}}`);
 const tests={{eq:A===B,ne:A!==B,slt:A<B,sle:A<=B,sgt:A>B,sge:A>=B,ult:U<V,ule:U<=V,ugt:U>V,uge:U>=V}};
 for(const [op,x] of Object.entries(tests)) assert.equal(call(op,a,b),x?1n:0n);
}}
for(const op of ['sdiv','udiv','srem','urem']) assert.throws(()=>call(op,1n,0n),WebAssembly.RuntimeError);
assert.throws(()=>call('sdiv',1n<<(W-1n),-1n),WebAssembly.RuntimeError);
assert.equal(call('srem',1n<<(W-1n),-1n),0n);
}}
"#
        );
    }
    run(
        &source,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        &js,
    );
}

#[test]
fn rejects_invalid_exports_and_unsupported_contracts() {
    let valid = parse("func @f() {\nblock0:\n return\n}").unwrap();
    assert!(
        emit_module(&valid, &options(&["missing"]))
            .unwrap_err()
            .0
            .contains("export")
    );
    assert!(
        emit_module(&valid, &options(&["f", "f"]))
            .unwrap_err()
            .0
            .contains("duplicate export")
    );
    for source in [
        "decl @missing(i32) -> i32",
        "decl @missing()\nfunc @f() {\nblock0:\n call @missing()\n return\n}",
        "decl @unused(ptr)",
        "func @f(i16) {\nblock0(v0: i16):\n return\n}",
        "func @f(f32) {\nblock0(v0: f32):\n return\n}",
        "func @f(i128) {\nblock0(v0: i128):\n return\n}",
        "func @f() {\nblock0:\n v0 = fconst.f64 1.0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i8 1\n v1 = iadd v0, v0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i32 1\n v1 = umulhi v0, v0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i32 1\n v1, v2 = uadd_overflow v0, v0\n return\n}",
        "func @f(i32) {\nblock0(v0: i32):\n v1 = clz v0\n return\n}",
        "func @f(i32) {\nblock0(v0: i32):\n v1 = sext.i64 v0\n return\n}",
    ] {
        let m =
            parse(source).unwrap_or_else(|e| panic!("test fixture parse failed: {e}\n{source}"));
        keel_ir::verify_module(&m).unwrap_or_else(|e| panic!("invalid fixture: {e:?}\n{source}"));
        assert!(
            emit_module(&m, &Options::default()).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn narrow_predicate_transport_and_mixed_width_counts() {
    run(
        r#"
func @predicate(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i8 -1
    v2 = iconst.i8 256
    brif v0, block1(v1), block1(v2)
block1(v3: i8):
    v4 = iconst.i32 19
    v5 = iconst.i32 23
    v6 = select v3, v4, v5
    return v6
}
func @shift64(i64, i32) -> i64 {
block0(v0: i64, v1: i32):
    v2 = shl v0, v1
    return v2
}
func @shift32(i32, i64) -> i32 {
block0(v0: i32, v1: i64):
    v2 = ushr v0, v1
    return v2
}
func @shift8(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i8 -1
    v2 = rotr v0, v1
    return v2
}
"#,
        &["predicate", "shift64", "shift32", "shift8"],
        r#"
assert.equal(e.predicate(1),19); assert.equal(e.predicate(0),23);
for(const k of [-1,0,31,32,63,64,65,255]) {
 assert.equal(e.shift64(1n,k),BigInt.asIntN(64,1n<<BigInt(k&63)));
 assert.equal(e.shift32(-1,BigInt(k)),(-1>>>(k&31))|0);
}
assert.equal(e.shift32(-1,0x100000001n),2147483647);
assert.equal(e.shift8(1n),2n);
"#,
    );
}

#[test]
fn recursive_and_void_calls_empty_module_explicit_exports() {
    run(
        r#"
func @count(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 0
    v2 = iconst.i32 1
    v3 = icmp.sgt v0, v1
    brif v3, block1, block2
block1:
    v4 = isub v0, v2
    call @nothing()
    v5 = call @count(v4)
    v6 = iadd v5, v2
    return v6
block2:
    return v1
}
func @nothing() {
block0:
    return
}
"#,
        &["count"],
        "assert.equal(e.count(20),20); assert.equal(e.nothing,undefined);",
    );
    let m = parse("func @f() {\nblock0:\n return\n}").unwrap();
    let opts = Options {
        exports: vec![Export {
            name: "renamed/λ".into(),
            function: "f".into(),
        }],
    };
    execute_options(
        &m,
        &opts,
        "assert.equal(e['renamed/λ'](),undefined); assert.equal(e.f,undefined);",
    );
    execute(
        &Module::default(),
        &[],
        "assert.deepEqual(Object.keys(e),[]);",
    );
    execute(&m, &[], "assert.deepEqual(Object.keys(e),[]);");
}

#[test]
fn rejects_families_even_in_unexported_functions() {
    for source in [
        include_str!("../../keel-ir/tests/golden/atomics.keel"),
        include_str!("../../keel-ir/tests/golden/inline_asm.keel"),
        include_str!("../../keel-ir/tests/golden/exceptions.keel"),
        include_str!("../../keel-ir/tests/golden/tail_calls.keel"),
        include_str!("../../keel-ir/tests/golden/stack_slots.keel"),
        include_str!("../../keel-ir/tests/golden/globals.keel"),
        include_str!("../../keel-ir/tests/golden/intrinsics.keel"),
        include_str!("../../keel-ir/tests/golden/variadic.keel"),
        "func @f(i32x4) {\nblock0(v0: i32x4):\n return\n}",
        "func @f(i32, ...) {\nblock0(v0: i32):\n return\n}",
        "func @f() returns_twice {\nblock0:\n return\n}",
        "func @f() constructor {\nblock0:\n return\n}",
        "func @f() weak {\nblock0:\n return\n}",
    ] {
        let m = parse(source).unwrap_or_else(|e| panic!("fixture parse failed: {e}\n{source}"));
        keel_ir::verify_module(&m).unwrap_or_else(|e| panic!("invalid fixture: {e:?}\n{source}"));
        assert!(
            emit_module(&m, &Options::default()).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn verifier_and_incompatible_callee_contracts() {
    let mut m = parse(
        r#"
func @f(i32) -> i32 {
block0(v0: i32):
    v1 = call @g(v0)
    return v1
}
func @g(i32) -> i32 {
block0(v0: i32):
    return v0
}
"#,
    )
    .unwrap();
    let callee = m.functions[0].callees().next().unwrap().0;
    m.functions[0].callee_mut(callee).sig =
        keel_ir::Signature::new(&[keel_ir::Type::I64], &[keel_ir::Type::I32]);
    assert!(emit_module(&m, &Options::default()).is_err());
    // Unused callee table contracts must also be checked.
    let mut m = parse("func @f() {\nblock0:\n return\n}").unwrap();
    m.functions[0].import_function("missing", keel_ir::Signature::new(&[], &[]));
    assert!(emit_module(&m, &Options::default()).is_err());
}

#[test]
fn constants_and_i32_switch_patterns() {
    run(
        r#"
func @constant64() -> i64 {
block0:
    v0 = iconst.i64 -9223372036854775808
    return v0
}
func @constant32() -> i32 {
block0:
    v0 = iconst.i32 4294967295
    return v0
}
func @switch32(i32) -> i32 {
block0(v0: i32):
    switch v0, block1, [4294967295: block2, -2147483648: block3]
block1:
    v1 = iconst.i32 7
    return v1
block2:
    v2 = iconst.i32 8
    return v2
block3:
    v3 = iconst.i32 9
    return v3
}
"#,
        &["constant64", "constant32", "switch32"],
        r#"
assert.equal(e.constant64(),-9223372036854775808n); assert.equal(e.constant32(),-1);
assert.equal(e.switch32(-1),8); assert.equal(e.switch32(-2147483648),9);
assert.equal(e.switch32(0),7); assert.equal(e.switch32(2147483647),7);
"#,
    );
}

#[test]
fn switch_parallel_cycles_with_removed_layout_gap() {
    let mut m = parse(
        r#"
func @cycle(i32, i64, i64, i64) -> i64 {
block0(v0: i32, v1: i64, v2: i64, v3: i64):
    v4 = iconst.i32 1
    jump block2(v0, v1, v2, v3)
block1:
    v5 = fconst.f64 1.0
    return v1
block2(v6: i32, v7: i64, v8: i64, v9: i64):
    v10 = isub v6, v4
    switch v6, block2(v10, v8, v9, v7), [0: block3(v7), 1: block2(v10, v9, v8, v7)]
block3(v11: i64):
    return v11
}
"#,
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    // Unsupported instructions in unreachable but laid-out blocks are rejected.
    assert!(emit_module(&m, &options(&["cycle"])).is_err());
    let removed = m.functions[0].blocks().nth(1).unwrap();
    m.functions[0].remove_block(removed);
    // Arena block IDs now differ from dispatcher PC indices. Only live layout
    // is emitted, including the mixed i32/i64 copies on the switch self-edge.
    execute(
        &m,
        &["cycle"],
        r#"
const seeds=[[11n,22n,33n],[-1n,-9223372036854775808n,0x123456789abcdefn]];
for(const initial of seeds) for(let n=0;n<30;n++) {
 let [a,b,c]=initial;
 for(let remaining=n;remaining>0;remaining--) {
  [a,b,c]=remaining===1?[c,b,a]:[b,c,a];
 }
 assert.equal(e.cycle(n,...initial),a);
}
"#,
    );
}

#[test]
fn rejects_explicit_scalar_call_abi_and_unused_indirect_table() {
    let mut m = parse(
        r#"
func @caller(i32) -> i32 {
block0(v0: i32):
    v1 = call @callee(v0)
    return v1
}
func @callee(i32) -> i32 {
block0(v0: i32):
    return v0
}
"#,
    )
    .unwrap();
    let f = &mut m.functions[0];
    let call = f.block_insts(f.entry_block().unwrap())[0];
    let mut data = f.inst_data(call).clone();
    if let keel_ir::InstData::Call { args, .. } = &mut data {
        *args = keel_ir::CallArgs::with_abi(
            args.iter()
                .map(|&v| (v, keel_ir::AbiParam::new(keel_ir::Type::I32))),
        );
    }
    *f.inst_data_mut(call) = data;
    keel_ir::verify_module(&m).unwrap();
    assert!(
        emit_module(&m, &Options::default())
            .unwrap_err()
            .0
            .contains("unsupported instruction")
    );

    let mut m = parse("func @f() {\nblock0:\n return\n}").unwrap();
    m.functions[0].import_signature(keel_ir::Signature::new(&[], &[]));
    keel_ir::verify_module(&m).unwrap();
    assert!(
        emit_module(&m, &Options::default())
            .unwrap_err()
            .0
            .contains("indirect-call contract")
    );
}
