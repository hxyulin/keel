use keel_ir::{Function, Module, Signature, parse};
use keel_wasm::{
    Diagnostic, Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_module,
    emit_wasm32_memory_module, emit_wasm32_runtime_module,
};
use std::io::Write;
use std::process::{Command, Stdio};
use wasmparser::{Operator, Parser, Payload};

fn emit(module: &Module, names: &[&str], profile: u8) -> Result<Vec<u8>, Diagnostic> {
    let scalar = Options {
        exports: names
            .iter()
            .map(|n| Export {
                name: (*n).into(),
                function: (*n).into(),
            })
            .collect(),
    };
    let memory = Wasm32MemoryOptions {
        scalar: scalar.clone(),
        pages: 1,
        export_name: "memory".into(),
    };
    let bytes = match profile {
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
    }?;
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    Ok(bytes)
}
fn execute(bytes: &[u8], assertions: &str) {
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
        .expect("Node required");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn bodies(bytes: &[u8]) -> Vec<wasmparser::FunctionBody<'_>> {
    Parser::new(0)
        .parse_all(bytes)
        .filter_map(|p| match p.unwrap() {
            Payload::CodeSectionEntry(b) => Some(b),
            _ => None,
        })
        .collect()
}
fn direct_shape(body: &wasmparser::FunctionBody<'_>, profile: u8) {
    let ops = body
        .get_operators_reader()
        .unwrap()
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!ops.iter().any(|o| matches!(
        o,
        Operator::Loop { .. } | Operator::Br { .. } | Operator::BrIf { .. }
    )));
    assert_eq!(
        ops.iter()
            .filter(|o| matches!(o, Operator::If { .. }))
            .count(),
        if profile == 2 { 2 } else { 0 }
    );
    assert_eq!(
        ops.iter()
            .filter(|o| matches!(o, Operator::Unreachable))
            .count(),
        if profile == 2 { 2 } else { 0 }
    );
    assert!(matches!(ops[ops.len() - 2], Operator::Return));
    eprintln!(
        "direct profile={profile} body_bytes={} operators={}",
        body.as_bytes().len(),
        ops.len()
    );
}

const FALLBACK: &str = r#"
func @selfloop() {
block0:
 jump block1
block1:
 jump block1
}
func @jump(i32)->i32 {
block0(v0:i32):
 jump block1(v0)
block1(v1:i32):
 return v1
}
func @branch(i32)->i32 {
block0(v0:i32):
 brif v0, block1, block2
block1:
 v1=iconst.i32 7
 return v1
block2:
 v2=iconst.i32 9
 return v2
}
func @switch(i32)->i32 {
block0(v0:i32):
 switch v0, block1, [1: block2]
block1:
 v1=iconst.i32 7
 return v1
block2:
 v2=iconst.i32 9
 return v2
}
func @unreachable_layout() {
block0:
 return
block1:
 return
}
"#;

// Captured from the unchanged dispatcher at 82d3244e (base d8aeff5d).
const FALLBACK_BASE: [&str; 5] = [
    "01017f034020004100460440410121000c010b20004101460440410121000c010b000b000b",
    "02017f017f03402002410046044020002101410121020c010b2002410146044020010f0b000b000b",
    "03017f017f017f03402003410046044020000440410121030c0205410221030c020b0b200341014604404107210120010f0b200341024604404109210220020f0b000b000b",
    "03017f017f017f03402003410046044020004101460440410221030c020b410121030c010b200341014604404107210120010f0b200341024604404109210220020f0b000b000b",
    "01017f0340200041004604400f0b200041014604400f0b000b000b",
];
const FALLBACK_RUNTIME: [&str; 5] = [
    "02017f027f2300210120014100490440000b200141006b220241808002490440000b20022400034020004100460440410121000c010b20004101460440410121000c010b000b000b",
    "03017f017f027f2300210320034100490440000b200341006b220441808002490440000b2004240003402002410046044020002101410121020c010b200241014604402001200324000f0b000b000b",
    "04017f017f017f027f2300210420044100490440000b200441006b220541808002490440000b2005240003402003410046044020000440410121030c0205410221030c020b0b20034101460440410721012001200424000f0b20034102460440410921022002200424000f0b000b000b",
    "04017f017f017f027f2300210420044100490440000b200441006b220541808002490440000b2005240003402003410046044020004101460440410221030c020b410121030c010b20034101460440410721012001200424000f0b20034102460440410921022002200424000f0b000b000b",
    "02017f027f2300210120014100490440000b200141006b220241808002490440000b20022400034020004100460440200124000f0b20004101460440200124000f0b000b000b",
];

#[test]
fn fallback_bodies_remain_byte_identical_including_one_block_selfedge() {
    let module = parse(FALLBACK).unwrap();
    for profile in 0..3 {
        let bytes = emit(
            &module,
            &["jump", "branch", "switch", "unreachable_layout"],
            profile,
        )
        .unwrap();
        for (i, body) in bodies(&bytes).iter().enumerate() {
            let ops = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(ops.iter().any(|o| matches!(o, Operator::Loop { .. })));
            let hex = body
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert_eq!(
                hex,
                if profile == 2 {
                    FALLBACK_RUNTIME[i]
                } else {
                    FALLBACK_BASE[i]
                },
                "profile {profile} function {i}"
            );
        }
        execute(
            &bytes,
            "for(let n=-3;n<4;n++){assert.equal(e.jump(n),n);assert.equal(e.branch(n),n?7:9);assert.equal(e.switch(n),n===1?9:7);} assert.equal(e.unreachable_layout(),undefined);",
        );
    }
}

#[test]
fn void_i32_i64_calls_and_mutual_recursion_all_profiles() {
    let module = parse(
        r#"
func @nothing() {
block0:
 return
}
func @wide(i64)->i64 {
block0(v0:i64):
 v1=iconst.i64 3
 v2=imul v0,v1
 return v2
}
func @entry(i32,i64)->i64 {
block0(v0:i32,v1:i64):
 call @nothing()
 v2=call @narrow(v0)
 v3=call @wide(v1)
 v4=iadd v3,v1
 return v4
}
func @narrow(i32)->i32 {
block0(v0:i32):
 v1=iconst.i32 5
 v2=iadd v0,v1
 return v2
}
func @step(i64)->i64 {
block0(v0:i64):
 v1=iconst.i64 1
 v2=isub v0,v1
 v3=call @recur(v2)
 v4=iadd v0,v3
 return v4
}
func @recur(i64)->i64 {
block0(v0:i64):
 v1=iconst.i64 0
 v2=icmp.eq v0,v1
 brif v2,block1,block2
block1:
 return v1
block2:
 v3=call @step(v0)
 return v3
}
"#,
    )
    .unwrap();
    for profile in 0..3 {
        let bytes = emit(
            &module,
            &["nothing", "narrow", "wide", "entry", "recur"],
            profile,
        )
        .unwrap();
        for body in &bodies(&bytes)[..5] {
            direct_shape(body, profile);
        }
        execute(
            &bytes,
            r#"
assert.equal(e.nothing(),undefined);
for(const x of [0n,1n,-1n,-2147483648n,2147483647n,0x123456789abcdefn,-(1n<<63n),(1n<<63n)-1n]) {
 const n=BigInt.asIntN(32,x), narrow=BigInt.asIntN(32,n+5n);
 assert.equal(BigInt(e.narrow(Number(n))),narrow);
 assert.equal(e.wide(x),BigInt.asIntN(64,3n*x));
 assert.equal(e.entry(Number(n),x),BigInt.asIntN(64,4n*x));
}
for(let n=0n;n<30n;n++) assert.equal(e.recur(n),n*(n+1n)/2n);
"#,
        );
    }
}

#[test]
fn raw_alias_destinations_and_removed_layout_blocks_all_profiles() {
    let mut module = parse(
        r#"
func @retained(i32)->i32 {
block0(v0:i32):
 v1=iconst.i32 99
 v2=iadd v1,v0
 return v2
block1:
 v3=iconst.i32 0
 return v3
}
func @removed(i32)->i32 {
block0(v0:i32):
 v1=iconst.i32 99
 v2=iadd v1,v0
 return v2
}
"#,
    )
    .unwrap();
    for (index, f) in module.functions.iter_mut().enumerate() {
        let entry = f.entry_block().unwrap();
        let producer = f.block_insts(entry)[0];
        f.replace_all_uses(f.first_result(producer), f.block_params(entry)[0]);
        if index == 0 {
            let dead = f.blocks().nth(1).unwrap();
            f.remove_block(dead);
        } else {
            f.remove_inst(producer);
        }
    }
    for profile in 0..3 {
        let bytes = emit(&module, &["retained", "removed"], profile).unwrap();
        for body in bodies(&bytes) {
            direct_shape(&body, profile);
        }
        execute(
            &bytes,
            "for(const x of [0,1,-1,-2147483648,2147483647]) for(const name of ['retained','removed']) assert.equal(e[name](x),(2*x)|0);",
        );
    }
}

#[test]
fn runtime_direct_frames_survive_recursion_and_restore_all_return_widths() {
    let module = parse(
        r#"
func @address()->i64 {
 ss0=stack_slot 16, align 16
block0:
 v0=stack_addr ss0
 v1=ptrtoint.i64 v0
 return v1
}
func @touch() {
 ss0=stack_slot 16, align 16
block0:
 v0=stack_addr ss0
 v1=iconst.i8 17
 store volatile v1,v0
 return
}
func @small()->i32 {
 ss0=stack_slot 16, align 16
block0:
 v0=call @address()
 v1=trunc.i32 v0
 return v1
}
func @step(i64)->i64 {
 ss0=stack_slot 16, align 16
block0(v0:i64):
 v1=stack_addr ss0
 store volatile v0,v1
 v2=iconst.i64 1
 v3=isub v0,v2
 v4=call @recur(v3)
 v5=load.i64 volatile v1
 v6=iadd v4,v5
 return v6
}
func @recur(i64)->i64 {
 ss0=stack_slot 16, align 16
block0(v0:i64):
 v1=iconst.i64 0
 v2=icmp.eq v0,v1
 brif v2,block1,block2
block1:
 return v1
block2:
 v3=call @step(v0)
 return v3
}
"#,
    )
    .unwrap();
    let bytes = emit(&module, &["address", "touch", "small", "recur"], 2).unwrap();
    for body in &bodies(&bytes)[..4] {
        direct_shape(body, 2);
    }
    execute(
        &bytes,
        r#"
const u=new Uint8Array(e.memory.buffer); u.fill(0xab);
for(let n=0n;n<50n;n++) {
 assert.equal(e.address(),65520n);
 assert.equal(e.small(),65504);
 assert.equal(e.recur(n),n*(n+1n)/2n);
 e.touch(); assert.equal(e.address(),65520n);
}
assert(u.slice(0,63000).every(x=>x===0xab));
assert.equal(u[65520],17); assert.equal(u[65535],0xab);
"#,
    );
}

#[test]
fn memory_width_volatile_call_order_pointer_normalization_and_trap_order() {
    let module = parse(
        r#"
func @set(i64) {
block0(v0:i64):
 v1=inttoptr.ptr v0
 v2=iconst.i8 34
 store volatile v2,v1
 return
}
func @ordered(i64)->i64 {
block0(v0:i64):
 v1=inttoptr.ptr v0
 v2=iconst.i8 17
 store volatile v2,v1
 v3=load.i8 volatile v1
 call @set(v0)
 v4=load.i8 volatile v1
 v5=uext.i64 v3
 v6=uext.i64 v4
 v7=iconst.i64 256
 v8=imul v5,v7
 v9=iadd v8,v6
 return v9
}
func @divide(i64)->i64 {
block0(v0:i64):
 v1=iconst.i64 7
 v2=inttoptr.ptr v1
 v3=iconst.i8 85
 store volatile v3,v2
 v4=iconst.i64 123
 v5=udiv v4,v0
 v6=iconst.i8 102
 store volatile v6,v2
 return v5
}
func @trap(i64) {
block0(v0:i64):
 v1=iconst.i64 7
 v2=inttoptr.ptr v1
 v3=iconst.i8 85
 store volatile v3,v2
 v4=inttoptr.ptr v0
 v5=iconst.i64 123
 store volatile v5,v4
 v6=iconst.i8 102
 store volatile v6,v2
 return
}
"#,
    )
    .unwrap();
    for profile in 1..3 {
        let bytes = emit(&module, &["ordered", "trap", "divide"], profile).unwrap();
        for body in bodies(&bytes) {
            direct_shape(&body, profile);
        }
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.equal(e.ordered(0x100000003n),17n*256n+34n);
assert(u.every((x,i)=>x===(i===3?34:0xab)));
assert.throws(()=>e.trap(65532n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===3?34:i===7?85:0xab)));
// Discard the instance after the trap.
"#,
        );
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.throws(()=>e.divide(0n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?85:0xab)));
// Discard the instance after the trap.
"#,
        );
    }
}

#[test]
fn unsupported_unused_instructions_and_invalid_empty_functions_still_reject() {
    let mut no_blocks = Module::default();
    no_blocks
        .functions
        .push(Function::new("invalid", Signature::new(&[], &[])));
    let mut no_terminal = parse("func @invalid() {\nblock0:\n return\n}").unwrap();
    let f = &mut no_terminal.functions[0];
    let ret = f.block_insts(f.entry_block().unwrap())[0];
    f.remove_inst(ret);
    let selfloop = parse("func @invalid() {\nblock0:\n jump block0\n}").unwrap();
    let unsupported = parse("func @valid() {\nblock0:\n return\n}\nfunc @unused() {\nblock0:\n v0=iconst.i32 1\n v1=clz v0\n return\n}").unwrap();
    for profile in 0..3 {
        emit(&Module::default(), &[], profile).unwrap();
        for module in [&no_blocks, &no_terminal, &unsupported, &selfloop] {
            assert!(emit(module, &[], profile).is_err());
        }
    }
}

#[test]
fn minimal_return_shapes_and_code_size_evidence_all_profiles() {
    let module = parse("func @void() {\nblock0:\n return\n}\nfunc @i32(i32)->i32 {\nblock0(v0:i32):\n return v0\n}\nfunc @i64(i64)->i64 {\nblock0(v0:i64):\n return v0\n}").unwrap();
    let mut emitted = Vec::new();
    for profile in 0..3 {
        let bytes = emit(&module, &[], profile).unwrap();
        for (index, body) in bodies(&bytes).iter().enumerate() {
            let operators = body.get_operators_reader().unwrap().into_iter().count();
            let locals: u32 = body
                .get_locals_reader()
                .unwrap()
                .into_iter()
                .map(|l| l.unwrap().0)
                .sum();
            eprintln!(
                "minimal profile={profile} function={index} body_bytes={} operators={operators} locals={locals}",
                body.as_bytes().len()
            );
        }
        emitted.push(bytes);
    }
    for (profile, bytes) in emitted.iter().enumerate() {
        for body in bodies(bytes) {
            direct_shape(&body, profile as u8);
            let locals: u32 = body
                .get_locals_reader()
                .unwrap()
                .into_iter()
                .map(|l| l.unwrap().0)
                .sum();
            assert_eq!(locals, if profile == 2 { 2 } else { 0 });
        }
    }
}

#[test]
fn unused_loads_and_aliased_division_producers_keep_their_traps() {
    let mut scalar = parse(
        r#"
func @divide(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 v2=sdiv v0,v1
 return v2
}
"#,
    )
    .unwrap();
    let f = &mut scalar.functions[0];
    let b = f.entry_block().unwrap();
    let div = f.block_insts(b)[0];
    // The raw producer stays in layout although its result resolves to v0.
    f.replace_all_uses(f.first_result(div), f.block_params(b)[0]);
    for profile in 0..3 {
        let bytes = emit(&scalar, &["divide"], profile).unwrap();
        direct_shape(&bodies(&bytes)[0], profile);
        execute(
            &bytes,
            "assert.equal(e.divide(17n,3n),17n); assert.throws(()=>e.divide(17n,0n),WebAssembly.RuntimeError);",
        );
        execute(
            &bytes,
            "assert.throws(()=>e.divide(-(1n<<63n),-1n),WebAssembly.RuntimeError);",
        );
    }

    let memory = parse(
        r#"
func @read(i64) {
block0(v0:i64):
 v1=iconst.i64 7
 v2=inttoptr.ptr v1
 v3=iconst.i8 85
 store volatile v3,v2
 v4=inttoptr.ptr v0
 v5=load.i64 volatile v4
 v6=load.i16 v4
 v7=iconst.i8 102
 store volatile v7,v2
 return
}
"#,
    )
    .unwrap();
    for profile in 1..3 {
        let bytes = emit(&memory, &["read"], profile).unwrap();
        direct_shape(&bodies(&bytes)[0], profile);
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
e.read(65528n);
assert(u.every((x,i)=>x===(i===7?102:0xab)));
assert.throws(()=>e.read(65529n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?85:0xab)));
"#,
        );
    }
}
