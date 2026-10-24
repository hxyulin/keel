use keel_ir::{Module, parse};
use keel_wasm::{
    Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_module,
    emit_wasm32_memory_module, emit_wasm32_runtime_module,
};
use std::io::Write;
use std::process::{Command, Stdio};

fn emit(module: &Module, names: &[&str], profile: u8) -> Result<Vec<u8>, keel_wasm::Diagnostic> {
    let scalar = Options {
        exports: names
            .iter()
            .map(|name| Export {
                name: (*name).into(),
                function: (*name).into(),
            })
            .collect(),
    };
    let memory = Wasm32MemoryOptions {
        scalar: scalar.clone(),
        pages: 1,
        export_name: "memory".into(),
    };
    match profile {
        0 => emit_module(module, &scalar),
        1 => emit_wasm32_memory_module(module, &memory),
        2 => emit_wasm32_runtime_module(
            module,
            &Wasm32RuntimeOptions {
                memory,
                data_floor: 1024,
                stack_bottom: 32768,
                stack_top: 65536,
            },
        ),
        _ => unreachable!(),
    }
}

fn execute(module: &Module, names: &[&str], profile: u8, assertions: &str) {
    let bytes = emit(module, names, profile).unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    let js = format!(
        r#"
const assert=require('assert/strict'), bytes=require('fs').readFileSync(0);
assert(WebAssembly.validate(bytes));
const mod=new WebAssembly.Module(bytes);
assert.deepEqual(WebAssembly.Module.imports(mod),[]);
const e=new WebAssembly.Instance(mod).exports;
{assertions}
"#
    );
    let mut child = Command::new("node")
        .args(["-e", &js])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node required for independent Wasm execution");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "profile {profile}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn conversions(bytes: &[u8]) -> Vec<(usize, usize)> {
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(bytes).unwrap();
    let mut result = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut wrap = 0;
            let mut extend = 0;
            for op in body.get_operators_reader().unwrap() {
                match op.unwrap() {
                    wasmparser::Operator::I32WrapI64 => wrap += 1,
                    wasmparser::Operator::I64ExtendI32U => extend += 1,
                    _ => (),
                }
            }
            result.push((wrap, extend));
        }
    }
    result
}

const CHAINS: &str = r#"
func @id(i32)->i32 {
block0(v0:i32):
 return v0
}
func @matched(i32)->i64 {
block0(v0:i32):
 v1 = call @id(v0)
 v2 = uext.i64 v1
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = inttoptr.ptr v4
 v6 = ptrtoint.i64 v5
 return v6
}
func @getter(i64)->i32 {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = ptrtoint.i64 v1
 v3 = trunc.i32 v2
 return v3
}
func @shared(i32)->i64 {
block0(v0:i32):
 v1 = uext.i64 v0
 v2 = trunc.i32 v1
 v3 = uext.i64 v2
 v4 = inttoptr.ptr v3
 v5 = ptrtoint.i64 v4
 v6 = iadd v5,v1
 return v6
}
func @roundtrip(i64)->i64 {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = ptrtoint.i64 v1
 return v2
}
func @phi(i64,i32)->i64 {
block0(v0:i64,v1:i32):
 v2 = inttoptr.ptr v0
 v3 = ptrtoint.i64 v2
 brif v1,block1(v3),block1(v0)
block1(v4:i64):
 v5 = trunc.i32 v4
 v6 = uext.i64 v5
 return v6
}
func @shared_getter(i64)->i64 {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = ptrtoint.i64 v1
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = iadd v2,v4
 return v5
}
"#;

#[test]
fn exact_patterns_calls_shared_uses_phi_and_unsigned_roundtrips() {
    let module = parse(CHAINS).unwrap();
    let names = ["matched", "getter", "shared", "roundtrip", "phi", "shared_getter"];
    for profile in [1, 2] {
        // matched removes four conversions, getter removes exactly two.
        assert_eq!(conversions(&emit(&module, &names, profile).unwrap()),
            [(0,0), (0,1), (1,0), (2,3), (1,1), (2,2), (2,2)]);
        execute(&module, &names, profile, r#"
const low=x=>BigInt.asUintN(32,x), signed=x=>BigInt.asIntN(32,x);
for(const x of [0n,1n,-1n,-2147483648n,0x80000000n,0xffffffffn,0x100000000n,
 0x100000001n,-0x100000001n,0x7fffffffffffffffn,-0x8000000000000000n]) {
 const a=BigInt.asIntN(64,x), i=Number(signed(a));
 assert.equal(e.matched(i),low(a));
 assert.equal(e.getter(a),Number(signed(a)));
 assert.equal(e.shared(i),2n*low(a));
 assert.equal(e.roundtrip(a),low(a));
 assert.equal(e.shared_getter(a),2n*low(a));
 for(const c of [0,1]) assert.equal(e.phi(a,c),low(a));
}
"#);
    }
}

#[test]
fn raw_aliases_keep_conversions_and_destinations() {
    let mut module = parse(r#"
func @alias(i32,i64)->i64 {
block0(v0:i32,v1:i64):
 v2 = uext.i64 v0
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = inttoptr.ptr v4
 v6 = ptrtoint.i64 v5
 v7 = iadd v6,v2
 return v7
}
"#).unwrap();
    let f = &mut module.functions[0];
    let b = f.entry_block().unwrap();
    let first = f.block_insts(b)[0];
    f.replace_all_uses(f.first_result(first), f.block_params(b)[1]);
    for profile in [1, 2] {
        assert_eq!(conversions(&emit(&module, &["alias"], profile).unwrap()), [(2,3)]);
        execute(&module, &["alias"], profile, r#"
for(const x of [0n,-1n,0xffffffffn,0x100000000n,-0x8000000000000000n]) {
 assert.equal(e.alias(-37,x),BigInt.asIntN(64,BigInt.asUintN(32,x)+x));
}
"#);
    }
}

#[test]
fn exact_memory_wrap_canaries_oob_and_call_volatile_order() {
    let module = parse(r#"
func @mark(i32)->i32 {
block0(v0:i32):
 v1 = iconst.i64 32
 v2 = inttoptr.ptr v1
 v3 = iconst.i8 85
 store volatile v3,v2
 return v0
}
func @write(i32,i64,i32)->i64 {
block0(v0:i32,v1:i64,v2:i32):
 v3 = call @mark(v0)
 v4 = uext.i64 v3
 v5 = trunc.i32 v4
 v6 = uext.i64 v5
 v7 = inttoptr.ptr v6
 v8 = ptradd v7,v1
 store align 1 volatile v2,v8
 v9 = load.i32 align 1 volatile v8
 v10 = uext.i64 v9
 return v10
}
func @fault(i32,i32)->i32 {
block0(v0:i32,v1:i32):
 v2 = uext.i64 v0
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = inttoptr.ptr v4
 v6 = iconst.i8 51
 store volatile v6,v5
 v7 = iconst.i32 1
 v8 = sdiv v7,v1
 v9 = iconst.i8 68
 store volatile v9,v5
 return v8
}
"#).unwrap();
    for profile in [1, 2] {
        execute(&module, &["write", "fault"], profile, r#"
const u=new Uint8Array(e.memory.buffer);
for(const [base,offset] of [[64n,0n],[0xffffffffn,66n],[0x80000000n,-0x7fffffbfn],
 [65n,0x100000000n],[65n,-0x100000000n],[0n,65532n]]) {
 u.fill(0xa7); const want=u.slice(), addr=Number(BigInt.asUintN(32,base+offset));
 want[32]=85; new DataView(want.buffer).setUint32(addr,0xdeadbeef,true);
 assert.equal(e.write(Number(BigInt.asIntN(32,base)),offset,-559038737),0xdeadbeefn);
 assert.deepEqual(u,want,'all bytes including surrounding canaries');
}
// Each trapped instance is discarded after inspecting its exact memory.
for(const offset of [65533n,0xffffffffn,0x80000000n]) {
 const t=new WebAssembly.Instance(mod).exports, m=new Uint8Array(t.memory.buffer);
 m.fill(0xa7); const want=m.slice(); want[32]=85;
 assert.throws(()=>t.write(0,offset,123),WebAssembly.RuntimeError);
 assert.deepEqual(m,want,'call store precedes OOB; OOB writes no prefix');
}
for(const addr of [72,65536]) {
 const t=new WebAssembly.Instance(mod).exports, m=new Uint8Array(t.memory.buffer);
 m.fill(0xa7); const want=m.slice(); if(addr<65536)want[addr]=51;
 assert.throws(()=>t.fault(addr,0),WebAssembly.RuntimeError);
 assert.deepEqual(m,want,'first store, then division, then final store');
}
"#);
    }
}

#[test]
fn unsupported_original_input_and_actual_wasm32_layout_still_gate() {
    for bad in [
        "func @unused(i32)->i32 {\nblock0(v0:i32):\n v1 = popcnt v0\n return v1\n}",
        "func @unused()->ptr {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n return v1\n}",
    ] {
        let module = parse(&format!("{CHAINS}\n{bad}")).unwrap();
        for profile in [1,2] {
            assert!(emit(&module, &["getter"], profile).is_err());
        }
    }
    // Ptr initializer occupies four bytes, not native eight: accepted by the
    // actual runtime WASM32 verifier, and getter still activates afterward.
    let module = parse(r#"
data @target = align 4 { i32 7 }
data @record = align 4 { ptr @target, i32 9 }
func @getter()->i32 {
block0:
 v0 = symbol_addr @record
 v1 = ptrtoint.i64 v0
 v2 = trunc.i32 v1
 return v2
}
"#).unwrap();
    assert_eq!(conversions(&emit(&module, &["getter"], 2).unwrap()), [(0,0)]);
    execute(&module, &["getter"], 2, "assert.equal(e.getter(),1028);");
}

#[test]
fn unused_external_alias_and_cross_block_chain_stay_literal() {
    let mut module = parse(r#"
func @external(i32)->i64 {
block0(v0:i32):
 v1 = uext.i64 v0
 v2 = trunc.i32 v1
 v3 = uext.i64 v2
 v4 = inttoptr.ptr v3
 v5 = ptrtoint.i64 v4
 v6 = iconst.i64 99
 return v5
}
func @cross(i32)->i64 {
block0(v0:i32):
 v1 = uext.i64 v0
 jump block1(v1)
block1(v2:i64):
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = inttoptr.ptr v4
 v6 = ptrtoint.i64 v5
 return v6
}
"#).unwrap();
    let f = &mut module.functions[0];
    let b = f.entry_block().unwrap();
    let first = f.first_result(f.block_insts(b)[0]);
    let external = f.first_result(f.block_insts(b)[5]);
    f.replace_all_uses(external, first);
    for profile in [1,2] {
        assert_eq!(conversions(&emit(&module, &["external", "cross"], profile).unwrap()), [(2,3), (2,3)]);
        execute(&module, &["external", "cross"], profile, r#"
for(const x of [0n,-1n,0x80000000n,0xffffffffn]) {
 for(const name of ['external','cross'])
  assert.equal(e[name](Number(BigInt.asIntN(32,x))),BigInt.asUintN(32,x));
}
"#);
    }
}

#[test]
fn final_pointer_alias_and_shared_users_keep_the_raw_destination() {
    let mut module = parse(r#"
func @final_alias(i32,i64)->i64 {
block0(v0:i32,v1:i64):
 v2 = inttoptr.ptr v1
 v3 = uext.i64 v0
 v4 = trunc.i32 v3
 v5 = uext.i64 v4
 v6 = inttoptr.ptr v5
 v7 = ptrtoint.i64 v6
 v8 = ptrtoint.i64 v2
 v9 = iadd v7,v8
 return v9
}
"#).unwrap();
    let f = &mut module.functions[0];
    let b = f.entry_block().unwrap();
    let replacement = f.first_result(f.block_insts(b)[0]);
    let final_value = f.first_result(f.block_insts(b)[4]);
    f.replace_all_uses(final_value, replacement);
    for profile in [1,2] {
        assert_eq!(conversions(&emit(&module, &["final_alias"], profile).unwrap()), [(1,2)]);
        execute(&module, &["final_alias"], profile, r#"
for(const x of [0n,-1n,0x80000000n,0x100000000n,-0x8000000000000000n])
 assert.equal(e.final_alias(123,x),2n*BigInt.asUintN(32,x));
"#);
    }
}


#[test]
fn complete_chains_keep_intermediates_used_by_calls_edges_and_control() {
    // Each suffix uses an intermediate after an otherwise complete adjacent
    // chain. Count operand occurrences, including duplicate operands/edges.
    for suffix in [
        "v7 = call @sum(v2,v2)\n return v7",
        "brif v1,block1(v2,v2),block1(v2,v6)\nblock1(v7:i64,v8:i64):\n v9 = iadd v7,v8\n return v9",
        "switch v2,block1(v2,v6),[1: block1(v2,v2)]\nblock1(v7:i64,v8:i64):\n v9 = iadd v7,v8\n return v9",
        "v7 = select v1,v2,v2\n v8 = iadd v7,v6\n return v8",
    ] {
        let module = parse(&format!(r#"
func @sum(i64,i64)->i64 {{
block0(v0:i64,v1:i64):
 v2 = iadd v0,v1
 return v2
}}
func @probe(i32,i32)->i64 {{
block0(v0:i32,v1:i32):
 v2 = uext.i64 v0
 v3 = trunc.i32 v2
 v4 = uext.i64 v3
 v5 = inttoptr.ptr v4
 v6 = ptrtoint.i64 v5
 {suffix}
}}
"#)).unwrap();
        for profile in [1,2] {
            assert_eq!(conversions(&emit(&module, &["probe"], profile).unwrap()), [(0,0),(2,3)]);
            execute(&module, &["probe"], profile, r#"
for(const x of [0n,1n,-1n,-2147483648n,0x7fffffffn,0x80000000n,0xffffffffn])
 for(const c of [0,1])
  assert.equal(e.probe(Number(BigInt.asIntN(32,x)),c),2n*BigInt.asUintN(32,x));
"#);
        }
    }
}

#[test]
fn saturated_occurrence_counts_cannot_turn_shared_values_into_single_use() {
    // A wrapping u8 counter would see 257 uses as one. The chain's first
    // intermediate must remain available to all 256 subsequent additions.
    let mut source = String::from(r#"
func @many(i32)->i64 {
block0(v0:i32):
 v1 = uext.i64 v0
 v2 = trunc.i32 v1
 v3 = uext.i64 v2
 v4 = inttoptr.ptr v3
 v5 = ptrtoint.i64 v4
"#);
    for n in 6..262 {
        source.push_str(&format!(" v{n} = iadd v{},v1\n",n-1));
    }
    source.push_str(" return v261\n}\n");
    let module = parse(&source).unwrap();
    for profile in [1,2] {
        assert_eq!(conversions(&emit(&module, &["many"], profile).unwrap()), [(2,3)]);
        execute(&module, &["many"], profile, r#"
for(const x of [0,1,-1,-2147483648,2147483647])
 assert.equal(e.many(x),257n*BigInt.asUintN(32,BigInt(x)));
"#);
    }
}

#[test]
fn signed_and_narrow_near_matches_and_effect_barriers_remain_literal() {
    for (first, second, barrier, counts, expected) in [
        ("sext.i64", "uext.i64", "", (2,2), "low(x)"),
        ("uext.i64", "sext.i64", "", (2,2), "low(x)"),
        ("uext.i64", "uext.i64", "call @mark()", (2,3), "low(x)"),
        // Narrow truncation is semantically different at 0x10000 and -1.
        ("uext.i64", "uext.i64", "narrow", (2,3), "BigInt.asUintN(16,x)"),
    ] {
        let trunc = if barrier == "narrow" {"i16"} else {"i32"};
        let effect = if barrier == "narrow" {""} else {barrier};
        let module = parse(&format!(r#"
func @mark() {{
block0:
 return
}}
func @probe(i32)->i64 {{
block0(v0:i32):
 v1 = {first} v0
 v2 = trunc.{trunc} v1
 {effect}
 v3 = {second} v2
 v4 = inttoptr.ptr v3
 v5 = ptrtoint.i64 v4
 return v5
}}
"#)).unwrap();
        for profile in [1,2] {
            assert_eq!(conversions(&emit(&module, &["probe"], profile).unwrap()), [(0,0),counts]);
            execute(&module, &["probe"], profile, &format!(r#"
const low=x=>BigInt.asUintN(32,x);
for(const x of [0n,1n,-1n,65535n,65536n,0x7fffffffn,0x80000000n,0xffffffffn])
 assert.equal(e.probe(Number(BigInt.asIntN(32,x))),{expected});
"#));
        }
    }
}

#[test]
fn matched_chains_keep_retained_aliased_faulting_source_producers() {
    for producer in ["sdiv v0,v1", "load.i32 align 1 volatile v3"] {
        let mut module = parse(&format!(r#"
func @probe(i32,i32,i64)->i64 {{
block0(v0:i32,v1:i32,v2:i64):
 v3 = inttoptr.ptr v2
 v4 = iconst.i64 40
 v5 = inttoptr.ptr v4
 v6 = iconst.i8 85
 store volatile v6,v5
 v7 = {producer}
 v8 = uext.i64 v7
 v9 = trunc.i32 v8
 v10 = uext.i64 v9
 v11 = inttoptr.ptr v10
 v12 = ptrtoint.i64 v11
 v13 = iconst.i8 102
 store volatile v13,v5
 return v12
}}
"#)).unwrap();
        let f = &mut module.functions[0];
        let b = f.entry_block().unwrap();
        let value = f.first_result(f.block_insts(b)[5]);
        f.replace_all_uses(value, f.block_params(b)[0]);
        for profile in [1,2] {
            assert_eq!(conversions(&emit(&module, &["probe"], profile).unwrap()), [(2,1)]);
            let faults = if producer.starts_with("sdiv") {
                "[[17,0,64n],[-2147483648,-1,64n]]"
            } else {
                "[[17,3,65533n],[17,3,0xffffffffn]]"
            };
            execute(&module, &["probe"], profile, &format!(r#"
const u=new Uint8Array(e.memory.buffer);
for(const x of [0,1,-1,-2147483648,2147483647]) {{
 u.fill(0xa7); const want=u.slice(); want[40]=102;
 assert.equal(e.probe(x,3,64n),BigInt.asUintN(32,BigInt(x)));
 assert.deepEqual(u,want);
}}
for(const args of {faults}) {{
 const t=new WebAssembly.Instance(mod).exports, m=new Uint8Array(t.memory.buffer);
 m.fill(0xa7); const want=m.slice(); want[40]=85;
 assert.throws(()=>t.probe(...args),WebAssembly.RuntimeError);
 assert.deepEqual(m,want,'retained source faults before the final store');
}}
"#));
        }
    }
}
