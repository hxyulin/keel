#[path = "support/adjacent_tee.rs"]
mod adjacent_tee;

use keel_ir::{Module, parse};
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
fn structured(body: &wasmparser::FunctionBody<'_>, profile: u8) {
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
        if profile == 2 { 3 } else { 1 }
    );
    assert_eq!(
        ops.iter().filter(|o| matches!(o, Operator::Else)).count(),
        1
    );
    assert_eq!(
        ops.iter().filter(|o| matches!(o, Operator::Return)).count(),
        2
    );
    assert_eq!(
        ops.iter()
            .filter(|o| matches!(o, Operator::Unreachable))
            .count(),
        if profile == 2 { 3 } else { 1 }
    );
    let locals: u32 = body
        .get_locals_reader()
        .unwrap()
        .into_iter()
        .map(|l| l.unwrap().0)
        .sum();
    eprintln!(
        "terminal profile={profile} bytes={} ops={} locals={locals}",
        body.as_bytes().len(),
        ops.len()
    );
}

#[test]
fn void_integer_widths_reversed_layout_shared_aliases_and_selected_traps() {
    let mut module = parse(
        r#"
func @void(i32) {
block0(v0:i32):
 brif v0,block2,block1
block1:
 return
block2:
 return
}
func @word(i32,i32)->i32 {
block0(v0:i32,v1:i32):
 v2=iconst.i32 11
 brif v0,block2,block1
block1:
 v3=iconst.i32 99
 v4=iadd v2,v3
 return v4
block2:
 v5=iconst.i32 33
 v6=isub v2,v5
 return v6
}
func @wide(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 brif v0,block1,block2
block1:
 v2=sdiv v0,v1
 return v2
block2:
 return v1
}
"#,
    )
    .unwrap();
    let f = &mut module.functions[1];
    let entry = f.entry_block().unwrap();
    let arm = f.blocks().nth(1).unwrap();
    let raw = f.first_result(f.block_insts(arm)[0]);
    f.replace_all_uses(raw, f.block_params(entry)[1]);
    let mut divide = module.clone();
    let f = &mut divide.functions[2];
    let entry = f.entry_block().unwrap();
    let arm = f.blocks().nth(1).unwrap();
    let raw = f.first_result(f.block_insts(arm)[0]);
    // Retain the faulting producer, but read the shared entry replacement.
    f.replace_all_uses(raw, f.block_params(entry)[0]);
    for profile in 0..3 {
        for (m, aliased) in [(&module, false), (&divide, true)] {
            let bytes = emit(m, &["void", "word", "wide"], profile).unwrap();
            for b in bodies(&bytes) {
                structured(&b, profile);
            }
            execute(
                &bytes,
                &format!(
                    r#"
for(const n of [0,1,-1,-2147483648,2147483647]) {{
 assert.equal(e.void(n),undefined);
 assert.equal(e.word(n,19),n?-22:30);
}}
assert.equal(e.wide(0n,0n),0n); // Unselected division by zero must not execute.
assert.equal(e.wide(17n,3n),{}n);
assert.equal(e.wide(1n<<32n,2n),{}n);
assert.throws(()=>e.wide(17n,0n),WebAssembly.RuntimeError);
"#,
                    if aliased { 17 } else { 5 },
                    if aliased {
                        4294967296i64
                    } else {
                        2147483648i64
                    }
                ),
            );
            execute(
                &bytes,
                "assert.throws(()=>e.wide(-(1n<<63n),-1n),WebAssembly.RuntimeError);",
            );
        }
    }
}

const MEMORY: &str = r#"
func @ordered(i32,i64)->i64 {
block0(v0:i32,v1:i64):
 v2=iconst.i64 7
 v3=inttoptr.ptr v2
 v4=iconst.i8 85
 store volatile v4,v3
 brif v0,block2,block1
block1:
 v5=iconst.i8 102
 store volatile v5,v3
 v6=iconst.i64 9
 return v6
block2:
 v7=inttoptr.ptr v1
 v8=load.i64 volatile v7
 v9=load.i16 v7
 v10=iconst.i8 119
 store volatile v10,v3
 return v8
}
"#;
#[test]
fn memory_effect_order_unused_faults_exact_widths_and_canaries() {
    let module = parse(MEMORY).unwrap();
    for profile in 1..3 {
        let bytes = emit(&module, &["ordered"], profile).unwrap();
        structured(&bodies(&bytes)[0], profile);
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.equal(e.ordered(0,65536n),9n);
assert(u.every((x,i)=>x===(i===7?102:0xab)));
assert.equal(e.ordered(1,0x10000fff8n),BigInt.asIntN(64,0xababababababababn));
assert(u.every((x,i)=>x===(i===7?119:0xab)));
assert.throws(()=>e.ordered(1,65529n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?85:0xab)));
"#,
        );
        // Isolate the unused ordinary i16 load from the wider volatile load.
        let ordinary = parse(&MEMORY.replace("load.i64 volatile v7", "iconst.i64 123")).unwrap();
        let bytes = emit(&ordinary, &["ordered"], profile).unwrap();
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.equal(e.ordered(0,65536n),9n);
assert.equal(e.ordered(1,65534n),123n);
assert.throws(()=>e.ordered(1,65535n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?85:0xab)));
"#,
        );
        // The retained aliased volatile producer still traps before later stores.
        let mut unused = module.clone();
        let f = &mut unused.functions[0];
        let arm = f.blocks().nth(2).unwrap();
        let raw = f.first_result(f.block_insts(arm)[1]);
        let replacement = f.block_params(f.entry_block().unwrap())[1];
        f.replace_all_uses(raw, replacement);
        let bytes = emit(&unused, &["ordered"], profile).unwrap();
        execute(
            &bytes,
            r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.equal(e.ordered(1,32n),32n);
assert.throws(()=>e.ordered(1,65529n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===7?85:0xab)));
"#,
        );
    }
}

const RECURSIVE: &str = r#"
func @recur(i64)->i64 {
 ss0=stack_slot 16, align 16
block0(v0:i64):
 v1=stack_addr ss0
 store volatile v0,v1
 brif v0,block2,block1
block1:
 v2=load.i64 volatile v1
 return v2
block2:
 v3=iconst.i64 1
 v4=isub v0,v3
 v5=call @recur(v4)
 v6=load.i64 volatile v1
 v7=iadd v5,v6
 return v7
}
func @address()->i64 {
 ss0=stack_slot 16, align 16
block0:
 v0=stack_addr ss0
 v1=ptrtoint.i64 v0
 return v1
}
"#;
#[test]
fn recursive_frames_normal_returns_and_allocation_failure_before_effects() {
    let module = parse(RECURSIVE).unwrap();
    let bytes = emit(&module, &["recur", "address"], 2).unwrap();
    structured(&bodies(&bytes)[0], 2);
    execute(
        &bytes,
        r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
for(let repeat=0;repeat<3;repeat++) for(let n=0n;n<50n;n++) {
 assert.equal(e.recur(n),n*(n+1n)/2n);
 assert.equal(e.address(),65520n);
}
assert(u.slice(0,64736).every(x=>x===0xab));
for(let i=64736;i<65536;i++) if(i%16>=8) assert.equal(u[i],0xab);
"#,
    );
    // Give every terminal-return width a real frame, then verify repeated
    // calls through both arms leave the next frame at its original address.
    let mut widths = parse(
        r#"
func @nothing(i32) {
block0(v0:i32):
 brif v0,block1,block2
block1:
 return
block2:
 return
}
func @word(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block2
block1:
 return v0
block2:
 return v0
}
func @wide(i64)->i64 {
block0(v0:i64):
 brif v0,block1,block2
block1:
 return v0
block2:
 return v0
}
func @address()->i64 {
 ss0=stack_slot 16, align 16
block0:
 v0=stack_addr ss0
 v1=ptrtoint.i64 v0
 return v1
}
"#,
    )
    .unwrap();
    for f in &mut widths.functions[..3] {
        f.create_stack_slot(16, 16);
    }
    let bytes = emit(&widths, &["nothing", "word", "wide", "address"], 2).unwrap();
    for b in &bodies(&bytes)[..3] {
        structured(b, 2);
    }
    execute(
        &bytes,
        r#"
for(let repeat=0;repeat<100;repeat++) for(const n of [0,1,-1]) {
 assert.equal(e.nothing(n),undefined);
 assert.equal(e.address(),65520n);
 assert.equal(e.word(n),n);
 assert.equal(e.address(),65520n);
 assert.equal(e.wide(BigInt(n)),BigInt(n));
 assert.equal(e.address(),65520n);
}
"#,
    );
    let options = Wasm32RuntimeOptions {
        memory: Wasm32MemoryOptions {
            scalar: Options {
                exports: vec![Export {
                    name: "recur".into(),
                    function: "recur".into(),
                }],
            },
            pages: 1,
            export_name: "memory".into(),
        },
        data_floor: 1024,
        stack_bottom: 65520,
        stack_top: 65536,
    };
    let bytes = emit_wasm32_runtime_module(&module, &options).unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    execute(
        &bytes,
        r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.throws(()=>e.recur(1n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===65520?1:i>65520&&i<65528?0:0xab)));
"#,
    );
    let mut options = options;
    options.stack_bottom = 65536;
    let bytes = emit_wasm32_runtime_module(&module, &options).unwrap();
    execute(
        &bytes,
        r#"
const u=new Uint8Array(e.memory.buffer);u.fill(0xab);
assert.throws(()=>e.recur(0n),WebAssembly.RuntimeError);
assert(u.every(x=>x===0xab));
"#,
    );
}

// Unmatched shapes: phi arms, duplicate edges, nonterminal arm, extra layout
// block (the plain helper's shape), switch, and a loop with parallel phi copies.
const UNMATCHED: &str = r#"
func @phi(i32,i32)->i32 {
block0(v0:i32,v1:i32):
 brif v0,block1(v1),block2(v1)
block1(v2:i32):
 return v2
block2(v3:i32):
 return v3
}
func @duplicate(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block1
block1:
 return v0
block2:
 return v0
}
func @jump(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block2
block1:
 jump block2
block2:
 return v0
}
func @four(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block2
block1:
 return v0
block2:
 return v0
block3:
 return v0
}
func @switch(i32)->i32 {
block0(v0:i32):
 switch v0,block1,[1:block2]
block1:
 return v0
block2:
 return v0
}
func @swap(i32,i32,i32)->i32 {
block0(v0:i32,v1:i32,v2:i32):
 jump block1(v0,v1,v2)
block1(v3:i32,v4:i32,v5:i32):
 brif v5,block2,block3
block2:
 v6=iconst.i32 1
 v7=isub v5,v6
 jump block1(v4,v3,v7)
block3:
 return v3
}
"#;
// Frozen from a45e1e96 before the terminal-if production change.
const UNMATCHED_BASE: [[&str; 6]; 3] = [
    [
        "03017f017f017f0340200441004604402000044020012102410121040c020520012103410221040c020b0b2004410146044020020f0b2004410246044020030f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410121010c020b0b2001410146044020000f0b2001410246044020000f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410221010c020b0b20014101460440410221010c010b2001410246044020000f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410221010c020b0b2001410146044020000f0b2001410246044020000f0b2001410346044020000f0b000b000b",
        "01017f03402001410046044020004101460440410221010c020b410121010c010b2001410146044020000f0b2001410246044020000f0b000b000b",
        "06017f017f017f017f017f017f034020084100460440200020012002210521042103410121080c010b2008410146044020050440410221080c0205410321080c020b0b2008410246044041012106200520066b2107200420032007210521042103410121080c010b2008410346044020030f0b000b000b",
    ],
    [
        "03017f017f017f0340200441004604402000044020012102410121040c020520012103410221040c020b0b2004410146044020020f0b2004410246044020030f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410121010c020b0b2001410146044020000f0b2001410246044020000f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410221010c020b0b20014101460440410221010c010b2001410246044020000f0b000b000b",
        "01017f03402001410046044020000440410121010c0205410221010c020b0b2001410146044020000f0b2001410246044020000f0b2001410346044020000f0b000b000b",
        "01017f03402001410046044020004101460440410221010c020b410121010c010b2001410146044020000f0b2001410246044020000f0b000b000b",
        "06017f017f017f017f017f017f034020084100460440200020012002210521042103410121080c010b2008410146044020050440410221080c0205410321080c020b0b2008410246044041012106200520066b2107200420032007210521042103410121080c010b2008410346044020030f0b000b000b",
    ],
    [
        "04017f017f017f027f2300210520054100490440000b200541006b220641808002490440000b200624000340200441004604402000044020012102410121040c020520012103410221040c020b0b200441014604402002200524000f0b200441024604402003200524000f0b000b000b",
        "02017f027f2300210220024100490440000b200241006b220341808002490440000b2003240003402001410046044020000440410121010c0205410121010c020b0b200141014604402000200224000f0b200141024604402000200224000f0b000b000b",
        "02017f027f2300210220024100490440000b200241006b220341808002490440000b2003240003402001410046044020000440410121010c0205410221010c020b0b20014101460440410221010c010b200141024604402000200224000f0b000b000b",
        "02017f027f2300210220024100490440000b200241006b220341808002490440000b2003240003402001410046044020000440410121010c0205410221010c020b0b200141014604402000200224000f0b200141024604402000200224000f0b200141034604402000200224000f0b000b000b",
        "02017f027f2300210220024100490440000b200241006b220341808002490440000b2003240003402001410046044020004101460440410221010c020b410121010c010b200141014604402000200224000f0b200141024604402000200224000f0b000b000b",
        "07017f017f017f017f017f017f027f2300210920094100490440000b200941006b220a41808002490440000b200a2400034020084100460440200020012002210521042103410121080c010b2008410146044020050440410221080c0205410321080c020b0b2008410246044041012106200520066b2107200420032007210521042103410121080c010b200841034604402003200924000f0b000b000b",
    ],
];

#[test]
fn unmatched_cfg_snapshots_and_phi_cycles() {
    let module = parse(UNMATCHED).unwrap();
    for profile in 0..3 {
        let bytes = emit(
            &module,
            &["phi", "duplicate", "jump", "four", "switch", "swap"],
            profile,
        )
        .unwrap();
        for (i, b) in bodies(&bytes).iter().enumerate() {
            let ops = b
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(ops.iter().any(|o| matches!(o, Operator::Loop { .. })));
            let hex = b
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let frozen = UNMATCHED_BASE[profile as usize][i];
            let original: Vec<u8> = (0..frozen.len())
                .step_by(2)
                .map(|n| u8::from_str_radix(&frozen[n..n + 2], 16).unwrap())
                .collect();
            let (expected, _) = adjacent_tee::fold_body(&original);
            let expected_hex = expected
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert_eq!(hex, expected_hex, "profile={profile} body={i}");
        }
        execute(
            &bytes,
            r#"
for(let n=0;n<12;n++) {
 assert.equal(e.phi(n,123),123);
 for(const name of ['duplicate','jump','four','switch']) assert.equal(e[name](n),n);
 assert.equal(e.swap(11,22,n),n%2?22:11);
}
"#,
        );
    }
}

#[test]
fn removed_blocks_and_original_input_admission_guards() {
    let mut module = parse(UNMATCHED).unwrap();
    let f = &mut module.functions[3];
    let dead = f.blocks().nth(3).unwrap();
    f.remove_block(dead);
    for profile in 0..3 {
        let bytes = emit(&module, &["four"], profile).unwrap();
        structured(&bodies(&bytes)[3], profile);
        execute(
            &bytes,
            "for(const n of [0,1,-1]) assert.equal(e.four(n),n);",
        );
    }
    let cross_arm = parse(
        r#"
func @bad(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block2
block1:
 v1=iconst.i32 7
 return v1
block2:
 return v1
}
"#,
    )
    .unwrap();
    let mut cross_alias = parse(
        r#"
func @bad(i32)->i32 {
block0(v0:i32):
 brif v0,block1,block2
block1:
 v1=iconst.i32 7
 return v1
block2:
 v2=iconst.i32 9
 return v2
}
"#,
    )
    .unwrap();
    let f = &mut cross_alias.functions[0];
    let first = f.blocks().nth(1).unwrap();
    let second = f.blocks().nth(2).unwrap();
    f.replace_all_uses(
        f.first_result(f.block_insts(second)[0]),
        f.first_result(f.block_insts(first)[0]),
    );
    let unused = parse(
        r#"
func @ok(i32) {
block0(v0:i32):
 brif v0,block1,block2
block1:
 return
block2:
 return
}
func @unsupported() {
block0:
 v0=iconst.i32 1
 v1=clz v0
 return
}
"#,
    )
    .unwrap();
    for profile in 0..3 {
        for bad in [&cross_arm, &cross_alias, &unused] {
            assert!(emit(bad, &[], profile).is_err());
        }
    }
    // Unused over-aligned slots still reject before the shape can be chosen.
    let oversized =
        parse(&RECURSIVE.replace("stack_slot 16, align 16", "stack_slot 16, align 32")).unwrap();
    assert!(emit(&oversized, &[], 2).is_err());
}
