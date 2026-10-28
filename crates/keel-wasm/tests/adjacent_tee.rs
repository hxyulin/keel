#[path = "support/combined_corpus.rs"]
mod combined_corpus;
#[path = "support/adjacent_tee.rs"]
mod oracle;

use keel_ir::{Module, parse};
use keel_wasm::{
    Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_module,
    emit_wasm32_memory_module, emit_wasm32_runtime_module,
};
use std::io::Write;
use std::process::{Command, Stdio};
use wasmparser::{Operator, Parser, Payload};

fn emit(module: &Module, profile: u8) -> Vec<u8> {
    let scalar = Options {
        exports: module
            .functions
            .iter()
            .map(|f| Export {
                name: f.name.clone(),
                function: f.name.clone(),
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
    }
    .unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    for body in bodies(&bytes) {
        assert_eq!(
            oracle::fold_body(body.as_bytes()).1,
            0,
            "no eligible pair remains"
        );
    }
    bytes
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
fn execute(bytes: &[u8], assertions: &str) {
    let js = format!(
        r#"
const assert=require('assert/strict'), bytes=require('fs').readFileSync(0);
assert(WebAssembly.validate(bytes));
const m=new WebAssembly.Module(bytes); assert.deepEqual(WebAssembly.Module.imports(m),[]);
const e=new WebAssembly.Instance(m).exports;
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
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn exact_raw_local_tee_stack_types_calls_aliases_and_phi_cycles() {
    let mut module = parse(
        r#"
func @identity(i64)->i64 {
block0(v0:i64):
 return v0
}
func @wide(i64)->i64 {
block0(v0:i64):
 v1=iconst.i64 3
 v2=imul v1,v0
 v3=call @identity(v2)
 v4=iadd v3,v0
 return v4
}
func @alias(i32)->i32 {
block0(v0:i32):
 v1=iconst.i32 99
 v2=iadd v1,v0
 return v2
}
func @cycle(i32,i32,i32)->i32 {
block0(v0:i32,v1:i32,v2:i32):
 v3=iconst.i32 1
 jump block1(v0,v1,v2)
block1(v4:i32,v5:i32,v6:i32):
 brif v4, block2, block3
block2:
 v7=isub v4,v3
 jump block1(v7,v6,v5)
block3:
 v8=isub v5,v6
 return v8
}
"#,
    )
    .unwrap();
    let f = &mut module.functions[2];
    let b = f.entry_block().unwrap();
    let producer = f.block_insts(b)[0];
    f.replace_all_uses(f.first_result(producer), f.block_params(b)[0]);
    for profile in 0..3 {
        let bytes = emit(&module, profile);
        let bs = bodies(&bytes);
        let ops = bs[1]
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            ops.iter()
                .any(|op| matches!(op, Operator::LocalTee { local_index: 1 }))
        );
        let locals: u32 = bs[1]
            .get_locals_reader()
            .unwrap()
            .into_iter()
            .map(|l| l.unwrap().0)
            .sum();
        assert_eq!(locals, 4 + if profile == 2 { 2 } else { 0 });
        // Retained aliased producer must still write raw local 1, never v0.
        let alias_ops = bs[2]
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(alias_ops.windows(2).any(|p| matches!(
            p,
            [
                Operator::I32Const { value: 99 },
                Operator::LocalSet { local_index: 1 }
            ]
        )));
        execute(
            &bytes,
            r#"
for(const n of [0n,1n,-1n,-(1n<<63n),(1n<<63n)-1n]) assert.equal(e.wide(n),BigInt.asIntN(64,4n*n));
for(const n of [0,1,-1,-2147483648,2147483647]) assert.equal(e.alias(n),(2*n)|0);
for(let n=0;n<20;n++) assert.equal(e.cycle(n,17,5),n%2?-12:12);
"#,
        );
    }
}

#[test]
fn static_frozen_corpus_combined_exact_body_operator_and_local_delta() {
    // Optional external evidence input; normal tests do not depend on /tmp.
    let Ok(root) = std::env::var("KEEL_TEE_FROZEN_CORPUS") else {
        return;
    };
    for profile in ["plain", "basic"] {
        let root = std::path::Path::new(&root);
        let module =
            parse(&std::fs::read_to_string(root.join(format!("{profile}.ir"))).unwrap()).unwrap();
        let current = emit(&module, 2);
        if let Ok(destination) = std::env::var("KEEL_TEE_CORPUS_OUTPUT") {
            let destination = std::path::Path::new(&destination);
            assert!(destination.starts_with("/tmp"));
            std::fs::create_dir_all(destination).unwrap();
            std::fs::write(destination.join(format!("{profile}.wasm")), &current).unwrap();
        }
        let original = std::fs::read(root.join(format!("{profile}.wasm"))).unwrap();
        let before = bodies(&original);
        let after = bodies(&current);
        assert_eq!(before.len(), after.len());
        let mut pairs = 0;
        let mut bytes_saved = 0;
        let mut operators_saved = 0;
        let mut locals_removed = 0;
        for (index, (old, new)) in before.iter().zip(&after).enumerate() {
            let sibling = combined_corpus::pre_tee_body(profile, index);
            let pre_tee = sibling.as_deref().unwrap_or(old.as_bytes());
            let (expected, count) = oracle::fold_body(pre_tee);
            assert_eq!(new.as_bytes(), expected);
            pairs += count;
            bytes_saved += old.as_bytes().len() - new.as_bytes().len();
            let declaration_count = |body: &wasmparser::FunctionBody<'_>| -> u32 {
                body.get_locals_reader()
                    .unwrap()
                    .into_iter()
                    .map(|l| l.unwrap().0)
                    .sum()
            };
            let removed = u32::from(profile == "basic" && index == 10);
            assert_eq!(declaration_count(old) - declaration_count(new), removed);
            locals_removed += removed;
            operators_saved += old.get_operators_reader().unwrap().into_iter().count()
                - new.get_operators_reader().unwrap().into_iter().count();
        }
        // Low-bit casts remove 28 bytes/18 operators before tee folding;
        // terminal-if removes another 42 bytes/24 operators and one PC local.
        let terminal = usize::from(profile == "basic");
        assert_eq!(pairs, 68);
        assert_eq!(bytes_saved, 28 + 42 * terminal + 2 * pairs);
        assert_eq!(operators_saved, 18 + 24 * terminal + pairs);
        assert_eq!(locals_removed, terminal as u32);
        eprintln!(
            "corpus {profile}: pairs={pairs} body_bytes_saved={bytes_saved} operators_saved={operators_saved} locals_removed={locals_removed}"
        );
    }
}
