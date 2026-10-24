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

#[test]
fn wrapping_negation_extrema_live_inputs_calls_phi_and_count_widths_all_profiles() {
    let mut source = String::new();
    let mut names = Vec::new();
    for w in [32, 64] {
        let other = if w == 32 { 64 } else { 32 };
        source += &format!(
            r#"
func @neg{w}(i{w})->i{w} {{
block0(v0:i{w}):
 v1 = ineg v0
 return v1
}}
func @live{w}(i{w})->i{w} {{
block0(v0:i{w}):
 v1 = ineg v0
 v2 = call @neg{w}(v0)
 v3 = iadd v1,v2
 v4 = iadd v3,v0
 return v4
}}
func @cycle{w}(i32,i{w})->i{w} {{
block0(v0:i32,v1:i{w}):
 v2 = iconst.i32 0
 v3 = iconst.i32 1
 jump block1(v0,v1)
block1(v4:i32,v5:i{w}):
 v6 = icmp.eq v4,v2
 brif v6,block2(v5),block3
block3:
 v7 = call @neg{w}(v5)
 v8 = isub v4,v3
 jump block1(v8,v7)
block2(v9:i{w}):
 return v9
}}
func @count{w}(i{other},i{w})->i{other} {{
block0(v0:i{other},v1:i{w}):
 v2 = ineg v1
 v3 = shl v0,v2
 return v3
}}
"#
        );
        for prefix in ["neg", "live", "cycle", "count"] {
            names.push(format!("{prefix}{w}"));
        }
    }
    let module = parse(&source).unwrap();
    let js = r#"
for(const w of [32,64]) {
 const W=BigInt(w), min=-(1n<<(W-1n)), max=-min-1n;
 const s=x=>BigInt.asIntN(w,x), arg=x=>w===32?Number(s(x)):s(x);
 const values=[0n,1n,-1n,min,max,0x123456789abcdefn,-37n];
 let seed=0xa35e981bc49f276dn;
 for(let i=0;i<32;i++) {
  seed=BigInt.asUintN(64,seed*6364136223846793005n+1442695040888963407n);
  values.push(s(seed));
 }
 for(const x of values) {
  const want=s(-s(x));
  assert.equal(BigInt(e['neg'+w](arg(x))),want);
  assert.equal(BigInt(e['live'+w](arg(x))),s(want+want+s(x)));
  for(let n=0;n<7;n++) assert.equal(BigInt(e['cycle'+w](n,arg(x))),n%2?want:s(x));
  const other=w===32?64:32, value=BigInt.asIntN(other,0x8123456789abcdefn);
  const count=BigInt.asUintN(other,want)%BigInt(other);
  assert.equal(BigInt(e['count'+w](other===32?Number(value):value,arg(x))),
   BigInt.asIntN(other,value<<count));
 }
 assert.equal(BigInt(e['neg'+w](arg(min))),min,'INT_MIN wraps without trapping');
}
assert.throws(()=>e.neg32(1n),TypeError);
assert.throws(()=>e.neg64(1),TypeError);
"#;
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    for profile in 0..3 {
        execute(&module, &names, profile, js);
    }
}

#[test]
fn retained_negation_producer_and_removed_operand_alias_all_profiles() {
    for w in [32, 64] {
        let mut module = parse(&format!(
            r#"
func @retained(i{w})->i{w} {{
block0(v0:i{w}):
 v1 = ineg v0
 v2 = ineg v1
 return v2
}}
func @removed(i{w})->i{w} {{
block0(v0:i{w}):
 v1 = iconst.i{w} 99
 v2 = ineg v1
 return v2
}}
"#
        ))
        .unwrap();
        for (index, f) in module.functions.iter_mut().enumerate() {
            let b = f.entry_block().unwrap();
            let producer = f.block_insts(b)[0];
            f.replace_all_uses(f.first_result(producer), f.block_params(b)[0]);
            if index == 1 {
                f.remove_inst(producer);
            }
        }
        keel_ir::verify_module(&module).unwrap();
        let js = format!(
            r#"
const w={w}, W=BigInt(w);
for(const x of [0n,1n,-1n,-(1n<<(W-1n)),(1n<<(W-1n))-1n,0x123456789abcdefn]) {{
 const input=BigInt.asIntN(w,x), arg=w===32?Number(input):input;
 for(const name of ['retained','removed']) assert.equal(BigInt(e[name](arg)),BigInt.asIntN(w,-input));
}}
"#
        );
        for profile in 0..3 {
            execute(&module, &["retained", "removed"], profile, &js);
        }
    }
}

#[test]
fn other_unary_and_negation_widths_stay_rejected_in_unexported_code() {
    let mut sources = Vec::new();
    for w in [32, 64] {
        for op in ["bnot", "clz", "ctz", "popcnt", "bswap"] {
            sources.push(format!(
                "func @f(i{w}) {{\nblock0(v0:i{w}):\n v1 = {op} v0\n return\n}}"
            ));
        }
    }
    for w in [8, 16, 128] {
        sources.push(format!(
            "func @f() {{\nblock0:\n v0 = iconst.i{w} 1\n v1 = ineg v0\n return\n}}"
        ));
    }
    for ty in ["f32", "f64"] {
        for op in [
            "fneg", "fabs", "fsqrt", "fceil", "ffloor", "ftrunc", "fnearest",
        ] {
            sources.push(format!(
                "func @f() {{\nblock0:\n v0 = fconst.{ty} 1.0\n v1 = {op} v0\n return\n}}"
            ));
        }
    }
    for source in sources {
        let module = parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for profile in 0..3 {
            let err = emit(&module, &[], profile).unwrap_err();
            assert!(err.0.contains("unsupported"), "{source}: {err}");
            // Accepted local widths must fail on the operation, not a signature.
            if source.contains("func @f(i")
                || source.contains("iconst.i8")
                || (profile != 0 && source.contains("iconst.i16"))
            {
                assert!(err.0.contains("unsupported instruction"), "{source}: {err}");
            }
        }
    }
}

#[test]
fn negated_i64_pointer_decrement_exact_canaries_memory_and_runtime() {
    let module = parse(
        r#"
func @decrement(i64,i64,i32)->i64 {
block0(v0:i64,v1:i64,v2:i32):
 v3 = inttoptr.ptr v0
 v4 = ineg v1
 v5 = ptradd v3,v4
 store volatile v2,v5
 v6 = ptrtoint.i64 v5
 return v6
}
"#,
    )
    .unwrap();
    let js = r#"
const u=new Uint8Array(e.memory.buffer);
const value=0x12345678;
for(const [base,amount] of [[21n,4n],[1n,1n],[65536n,4n],[0n,-17n],
 [0x100000021n,4n],[17n,-9223372036854775808n]]) {
 u.fill(0xa5);
 const address=Number(BigInt.asUintN(32,base-BigInt.asIntN(64,amount)));
 assert.equal(e.decrement(base,amount,value),BigInt(address));
 for(let i=0;i<u.length;i++) assert.equal(u[i],i>=address&&i<address+4
  ? Number((BigInt(value)>>BigInt((i-address)*8))&255n):0xa5,`canary ${i}`);
}
// Last operation on each fresh instance: runtime traps invalidate the instance.
u.fill(0xa5);
assert.throws(()=>e.decrement(65536n,1n,value),WebAssembly.RuntimeError);
assert(u.every(x=>x===0xa5),'trapping store writes no prefix');
"#;
    for profile in 1..3 {
        execute(&module, &["decrement"], profile, js);
    }
}
