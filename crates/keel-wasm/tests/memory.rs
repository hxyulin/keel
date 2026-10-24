use keel_ir::{Module, parse};
use keel_wasm::{Export, Options, Wasm32MemoryOptions, emit_module, emit_wasm32_memory_module};
use std::io::Write;
use std::process::{Command, Stdio};

fn options(names: &[&str]) -> Wasm32MemoryOptions {
    Wasm32MemoryOptions {
        scalar: Options {
            exports: names
                .iter()
                .map(|n| Export {
                    name: (*n).into(),
                    function: (*n).into(),
                })
                .collect(),
        },
        pages: 1,
        export_name: "memory".into(),
    }
}
fn checked(source: &str) -> Module {
    let m = parse(source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    keel_ir::verify_module(&m).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    m
}
fn validate(bytes: &[u8]) {
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(bytes)
        .unwrap();
}
fn execute(source: &str, names: &[&str], assertions: &str) {
    execute_pages(source, names, 1, assertions);
}
fn execute_pages(source: &str, names: &[&str], pages: u32, assertions: &str) {
    let m = checked(source);
    let mut opts = options(names);
    opts.pages = pages;
    let bytes = emit_wasm32_memory_module(&m, &opts).unwrap();
    validate(&bytes);
    let js = format!(
        r#"
const fs=require('fs'), assert=require('assert/strict');
const bytes=fs.readFileSync(0);
assert(WebAssembly.validate(bytes));
const mod=new WebAssembly.Module(bytes);
assert.deepEqual(WebAssembly.Module.imports(mod),[]);
const e=new WebAssembly.Instance(mod).exports;
const u=new Uint8Array(e.memory.buffer), dv=new DataView(e.memory.buffer);
assert.equal(u.length,{pages}*65536);
assert.throws(()=>e.memory.grow(1),RangeError);
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
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn explicit_memory_configuration_and_export_namespace() {
    let empty = Module::default();
    for pages in [0, 1, 65536] {
        let mut opts = options(&[]);
        opts.pages = pages;
        opts.export_name = "mémoire\0raw".into(); // Exact UTF-8; no C string convention.
        let bytes = emit_wasm32_memory_module(&empty, &opts).unwrap();
        validate(&bytes); // Validate 4 GiB encoding without allocating 4 GiB.
        let mut count = 0;
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            if let wasmparser::Payload::MemorySection(section) = payload.unwrap() {
                for memory in section {
                    let memory = memory.unwrap();
                    assert_eq!(memory.initial, u64::from(pages));
                    assert_eq!(memory.maximum, Some(u64::from(pages)));
                    assert!(!memory.memory64 && !memory.shared);
                    count += 1;
                }
            }
        }
        assert_eq!(count, 1);
    }
    for pages in [65537, u32::MAX] {
        let mut opts = options(&[]);
        opts.pages = pages;
        assert!(
            emit_wasm32_memory_module(&empty, &opts)
                .unwrap_err()
                .0
                .contains("pages")
        );
    }
    let mut opts = options(&[]);
    opts.export_name.clear();
    assert!(
        emit_wasm32_memory_module(&empty, &opts)
            .unwrap_err()
            .0
            .contains("nonempty")
    );
    let m = checked("func @f() {\nblock0:\n return\n}");
    let mut opts = options(&["f"]);
    opts.export_name = "f".into();
    assert!(
        emit_wasm32_memory_module(&m, &opts)
            .unwrap_err()
            .0
            .contains("collides")
    );
    let mut opts = options(&["f"]);
    opts.scalar.exports.push(opts.scalar.exports[0].clone());
    assert!(
        emit_wasm32_memory_module(&m, &opts)
            .unwrap_err()
            .0
            .contains("duplicate")
    );
    assert!(emit_wasm32_memory_module(&m, &options(&["missing"])).is_err());
    // Existing struct literals and empty scalar binary remain memory-free.
    let bytes = emit_module(&empty, &Options { exports: vec![] }).unwrap();
    assert!(
        !wasmparser::Parser::new(0)
            .parse_all(&bytes)
            .any(|p| matches!(p.unwrap(), wasmparser::Payload::MemorySection(_)))
    );
    execute(
        "",
        &[],
        "assert.deepEqual(Object.keys(e),['memory']); assert(u.every(x=>x===0));",
    );
}

#[test]
fn exact_integer_and_pointer_footprints_unaligned_edges_and_oob() {
    let mut source = String::new();
    let mut names = Vec::new();
    for ty in ["i8", "i16", "i32", "i64", "ptr"] {
        let cv = match ty {
            "ptr" => "v3 = inttoptr.ptr v1".into(),
            "i64" => String::new(),
            _ => format!("v3 = trunc.{ty} v1"),
        };
        let value = if ty == "i64" { "v1" } else { "v3" };
        source += &format!(
            "func @store_{ty}(i64,i64) {{\nblock0(v0:i64,v1:i64):\n v2 = inttoptr.ptr v0\n {cv}\n store {value},v2\n return\n}}\n"
        );
        let cv = match ty {
            "ptr" => "v3 = ptrtoint.i64 v2".into(),
            "i64" => String::new(),
            _ => "v3 = uext.i64 v2".into(),
        };
        let value = if ty == "i64" { "v2" } else { "v3" };
        source += &format!(
            "func @load_{ty}(i64)->i64 {{\nblock0(v0:i64):\n v1 = inttoptr.ptr v0\n v2 = load.{ty} v1\n {cv}\n return {value}\n}}\n"
        );
        names.push(format!("store_{ty}"));
        names.push(format!("load_{ty}"));
        if ty != "ptr" && ty != "i64" {
            source += &format!(
                "func @signed_{ty}(i64)->i64 {{\nblock0(v0:i64):\n v1 = inttoptr.ptr v0\n v2 = load.{ty} v1\n v3 = sext.i64 v2\n return v3\n}}\n"
            );
            names.push(format!("signed_{ty}"));
        }
    }
    execute(
        &source,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        r#"
const values=[0n,1n,-1n,0x80n,0xffn,0x8000n,0xffffn,0x80000000n,0xffffffffn,
 0x123456789abcdefn,-9223372036854775808n,9223372036854775807n];
for(const [ty,width] of [['i8',1],['i16',2],['i32',4],['i64',8],['ptr',4]]) {
 const store=e['store_'+ty], load=e['load_'+ty];
 for(const addr of [0,1,3,17,65536-width]) for(const x of values) {
  u.fill(0xa5); store(BigInt(addr),x);
  const expected=BigInt.asUintN(width*8,x);
  for(let i=0;i<u.length;i++) {
   const want=i>=addr && i<addr+width ? Number((expected>>BigInt(8*(i-addr)))&255n):0xa5;
   assert.equal(u[i],want,`${ty} footprint byte ${i}`);
  }
  assert.equal(load(BigInt(addr)),ty==='i64'?BigInt.asIntN(64,x):expected);
  if(e['signed_'+ty]) assert.equal(e['signed_'+ty](BigInt(addr)),BigInt.asIntN(width*8,x));
  // High bits are discarded before the access, for all storage types.
  assert.equal(load(0x100000000n+BigInt(addr)),load(BigInt(addr)));
 }
 for(const addr of [65537-width,65536,0x80000000,0xffffffff]) {
  u.fill(0x6b);
  assert.throws(()=>load(BigInt(addr)),WebAssembly.RuntimeError);
  assert.throws(()=>store(BigInt(addr),-1n),WebAssembly.RuntimeError);
  assert(u.every(x=>x===0x6b),'OOB store must not write a prefix');
 }
 // Independent host bytes exercise read width and little-endian semantics.
 for(let i=0;i<8;i++) u[101+i]=0x80+i;
 let want=0n; for(let i=0;i<width;i++) want|=BigInt(u[101+i])<<BigInt(i*8);
 assert.equal(load(101n),ty==='i64'?BigInt.asIntN(64,want):want);
}
"#,
    );
}

#[test]
fn full_width_pointer_normalization_wrap_and_unsigned_relations() {
    let mut source = String::from(
        r#"
func @address(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 v2 = inttoptr.ptr v0
 v3 = ptradd v2,v1
 v4 = ptrtoint.i64 v3
 return v4
}
func @read_offset(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 v2 = inttoptr.ptr v0
 v3 = ptradd v2,v1
 v4 = load.i8 v3
 v5 = uext.i64 v4
 return v5
}
func @write_offset(i64,i64,i64) {
block0(v0:i64,v1:i64,v2:i64):
 v3 = inttoptr.ptr v0
 v4 = ptradd v3,v1
 v5 = trunc.i8 v2
 store v5,v4
 return
}
func @offset32(i32)->i64 {
block0(v0:i32):
 v1 = uext.i64 v0
 v2 = inttoptr.ptr v1
 v3 = ptrtoint.i64 v2
 return v3
}
"#,
    );
    let mut names = vec!["address", "read_offset", "write_offset", "offset32"];
    for cc in ["eq", "ne", "ult", "ule", "ugt", "uge"] {
        source += &format!(
            "func @{cc}(i64,i64)->i32 {{\nblock0(v0:i64,v1:i64):\n v2 = inttoptr.ptr v0\n v3 = inttoptr.ptr v1\n v4 = icmp.{cc} v2,v3\n v5 = uext.i32 v4\n return v5\n}}\n"
        );
        names.push(cc);
    }
    execute(
        &source,
        &names,
        r#"
const xs=[0n,1n,-1n,0x7fffffffn,0x80000000n,0xffffffffn,0x100000000n,
 0x100000001n,0x123456789abcdefn,-9223372036854775808n,9223372036854775807n];
const low=x=>BigInt.asUintN(32,x);
for(const a of xs) for(const b of xs) {
 assert.equal(e.address(a,b),low(a+b));
 const x=low(a),y=low(b);
 for(const [cc,want] of [['eq',x===y],['ne',x!==y],['ult',x<y],['ule',x<=y],['ugt',x>y],['uge',x>=y]])
  assert.equal(e[cc](a,b),Number(want));
}
for(const x of [0,1,-1,-2147483648,2147483647]) assert.equal(e.offset32(x),low(BigInt(x)));
for(const [base,offset,addr] of [[0xffffffffn,1n,0],[0n,-4294967295n,1],
 [0x80000000n,-2147483647n,1],[2n,-1n,1],[0x100000001n,0x100000000n,1],
 [1n,-9223372036854775808n,1],[0n,65535n,65535]]) {
 u.fill(0x47); e.write_offset(base,offset,0x1a5n);
 assert.equal(u[addr],0xa5); assert.equal(e.read_offset(base,offset),0xa5n);
 assert(u.every((x,i)=>x===(i===addr?0xa5:0x47)));
}
for(const [a,b] of [[0n,-1n],[0x7fffffffn,1n],[65535n,1n]]) {
 assert.throws(()=>e.read_offset(a,b),WebAssembly.RuntimeError);
 assert.throws(()=>e.write_offset(a,b,1n),WebAssembly.RuntimeError);
}
"#,
    );
}

#[test]
fn narrow_conversion_matrix_and_canonical_transport() {
    let mut source = String::new();
    let mut names = Vec::new();
    for from in [8, 16, 32, 64] {
        for to in [8, 16, 32, 64] {
            if from == to {
                continue;
            }
            for op in if from > to {
                vec!["trunc"]
            } else {
                vec!["sext", "uext"]
            } {
                let name = format!("{op}_{from}_{to}");
                let input = if from == 64 {
                    String::new()
                } else {
                    format!("v1 = trunc.i{from} v0")
                };
                let arg = if from == 64 { "v0" } else { "v1" };
                let output = if to == 64 {
                    String::new()
                } else {
                    "v3 = uext.i64 v2".into()
                };
                let result = if to == 64 { "v2" } else { "v3" };
                source += &format!(
                    "func @{name}(i64)->i64 {{\nblock0(v0:i64):\n {input}\n v2 = {op}.i{to} {arg}\n {output}\n return {result}\n}}\n"
                );
                names.push(name);
            }
        }
    }
    source += r#"
func @transport(i32)->i64 {
block0(v0:i32):
 v1 = iconst.i16 -1
 v2 = iconst.i16 65537
 v3 = select v0,v1,v2
 v4 = iconst.i8 128
 v5 = sext.i16 v4
 jump block1(v3,v5)
block1(v6:i16,v7:i16):
 v8 = uext.i64 v6
 v9 = uext.i64 v7
 v10 = iadd v8,v9
 return v10
}
"#;
    names.push("transport".into());
    execute(
        &source,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        r#"
const xs=[0n,1n,-1n,127n,128n,255n,256n,32767n,32768n,65535n,65536n,
 0x80000000n,0xffffffffn,0x123456789abcdefn,-9223372036854775808n,9223372036854775807n];
for(let f of [8,16,32,64]) for(let t of [8,16,32,64]) if(f!==t) {
 for(const op of (f>t?['trunc']:['sext','uext'])) for(const x of xs) {
  let v=op==='sext'?BigInt.asIntN(f,x):BigInt.asUintN(f,x);
  const want=t===64?BigInt.asIntN(t,v):BigInt.asUintN(t,v);
  assert.equal(e[`${op}_${f}_${t}`](x),want,`${op} ${f} to ${t} at ${x}`);
 }
}
assert.equal(e.transport(1),65535n+65408n); assert.equal(e.transport(0),1n+65408n);
"#,
    );
}

#[test]
fn volatile_order_aliases_calls_alignment_and_notrap_hint() {
    execute(
        r#"
func @ordered(i64)->i64 {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = iconst.i8 17
 store volatile v2,v1
 v3 = load.i8 volatile v1
 call @overwrite(v0)
 v4 = load.i8 volatile v1
 v5 = uext.i64 v3
 v6 = uext.i64 v4
 v7 = iconst.i64 256
 v8 = imul v5,v7
 v9 = iadd v8,v6
 return v9
}
func @overwrite(i64) {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = iconst.i8 34
 store volatile v2,v1
 return
}
func @aligned(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 v2 = inttoptr.ptr v0
 store align 64 notrap v1,v2
 v3 = load.i64 align 64 notrap v2
 return v3
}
"#,
        &["ordered", "aligned"],
        r#"
u.fill(0xab); assert.equal(e.ordered(3n),17n*256n+34n);
assert(u.every((x,i)=>x===(i===3?34:0xab)));
assert.equal(e.aligned(64n,-1n),-1n);
// Wasm alignment immediates remain hints even when they exceed natural width.
assert.equal(e.aligned(3n,0x123456789abcdefn),0x123456789abcdefn);
// notrap is a producer guarantee, never an unchecked Wasm memory access.
assert.throws(()=>e.aligned(65536n,5n),WebAssembly.RuntimeError);
"#,
    );
}

#[test]
fn rejects_unused_contracts_and_unimplemented_operations() {
    for source in [
        "data @unused = align 1 { i8 0 }",
        "decl data @unused",
        "func @f(ptr) {\nblock0(v0:ptr):\n return\n}",
        "func @f(i8) {\nblock0(v0:i8):\n return\n}",
        "func @f()->ptr {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n return v1\n}",
        "func @f() {\n ss0 = stack_slot 4, align 4\nblock0:\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i16 1\n v1 = iadd v0,v0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i8 1\n v1 = icmp.eq v0,v0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = icmp.slt v1,v1\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = load.f32 v1\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = load.i128 v1\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = atomic_load.i32 acquire v1\n return\n}",
        "func @f() {\nblock0:\n v0 = intrinsic stack_save()\n return\n}",
        "func @f() {\nblock0:\n return\nblock1:\n v0 = fconst.f64 1.0\n return\n}",
        "decl @unused(i32)->i32",
    ] {
        let m = checked(source);
        assert!(
            emit_wasm32_memory_module(&m, &options(&[])).is_err(),
            "accepted {source}"
        );
    }
    let mut m = checked("func @f() {\nblock0:\n return\n}");
    m.functions[0].import_function("unused", keel_ir::Signature::new(&[], &[]));
    assert!(emit_wasm32_memory_module(&m, &options(&[])).is_err());
    let mut m = checked("func @f() {\nblock0:\n return\n}");
    m.functions[0].import_signature(keel_ir::Signature::new(&[], &[]));
    assert!(emit_wasm32_memory_module(&m, &options(&[])).is_err());
    let mut m = checked("func @f() {\nblock0:\n return\n}");
    m.functions[0].import_data("unused");
    assert!(emit_wasm32_memory_module(&m, &options(&[])).is_err());
}

#[test]
fn memory_subset_requires_opt_in_and_invalid_access_flags_are_rejected() {
    for source in [
        "func @f() {\nblock0:\n v0 = iconst.i16 1\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i64 1\n v1 = inttoptr.ptr v0\n return\n}",
        "func @f() {\nblock0:\n v0 = iconst.i32 1\n v1 = trunc.i8 v0\n return\n}",
    ] {
        let m = checked(source);
        assert!(emit_module(&m, &Options::default()).is_err());
        emit_wasm32_memory_module(&m, &options(&[])).unwrap();
    }
    let mut m = checked(
        "func @f() {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = load.i32 v1\n store v2,v1\n return\n}",
    );
    let b = m.functions[0].entry_block().unwrap();
    let insts = m.functions[0].block_insts(b).to_vec();
    for i in [insts[2], insts[3]] {
        for align in [0, 3] {
            let mut bad = m.clone();
            match bad.functions[0].inst_data_mut(i) {
                keel_ir::InstData::Load { flags, .. } | keel_ir::InstData::Store { flags, .. } => {
                    flags.align = align
                }
                _ => unreachable!(),
            }
            assert!(
                emit_wasm32_memory_module(&bad, &options(&[]))
                    .unwrap_err()
                    .0
                    .contains("alignment")
            );
        }
    }
    // Invalid addresses/types also fail verification before any bytes are encoded.
    *m.functions[0].inst_data_mut(insts[2]) = keel_ir::InstData::Load {
        flags: keel_ir::MemoryFlags::default(),
        ty: keel_ir::Type::I32,
        addr: m.functions[0].inst_results(insts[0])[0],
    };
    assert!(emit_wasm32_memory_module(&m, &options(&[])).is_err());
}

#[test]
fn pointer_and_narrow_cfg_transport_loaded_addresses_and_direct_recursion() {
    execute(
        r#"
func @cycle(i32,i64,i64)->i64 {
block0(v0:i32,v1:i64,v2:i64):
 v3 = inttoptr.ptr v1
 v4 = inttoptr.ptr v2
 v5 = iconst.i16 65535
 v6 = iconst.i16 256
 v7 = iconst.i32 1
 v8 = iconst.i32 0
 jump block1(v0,v3,v4,v5,v6)
block1(v9:i32,v10:ptr,v11:ptr,v12:i16,v13:i16):
 v14 = icmp.eq v9,v8
 brif v14,block2(v10,v12),block3
block3:
 v15 = isub v9,v7
 jump block1(v15,v11,v10,v13,v12)
block2(v16:ptr,v17:i16):
 v18 = select v17,v16,v3
 v19 = load.i8 v18
 v20 = uext.i64 v19
 v21 = uext.i64 v17
 v22 = iadd v20,v21
 return v22
}
func @indirection(i64)->i64 {
block0(v0:i64):
 v1 = inttoptr.ptr v0
 v2 = load.ptr v1
 v3 = load.i8 v2
 v4 = uext.i64 v3
 return v4
}
func @recurse(i32,i64)->i64 {
block0(v0:i32,v1:i64):
 v2 = inttoptr.ptr v1
 v3 = trunc.i8 v0
 store volatile v3,v2
 v4 = iconst.i32 0
 v5 = icmp.eq v0,v4
 brif v5,block2,block1
block1:
 v6 = iconst.i32 1
 v7 = isub v0,v6
 v8 = call @recurse(v7,v1)
 v9 = load.i8 volatile v2
 v10 = uext.i64 v9
 v11 = iadd v8,v10
 return v11
block2:
 v12 = load.i8 volatile v2
 v13 = uext.i64 v12
 return v13
}
"#,
        &["cycle", "indirection", "recurse"],
        r#"
u[11]=17; u[19]=29;
for(let n=0;n<20;n++) assert.equal(e.cycle(n,11n,19n),n%2?29n+256n:17n+65535n);
dv.setUint32(65532,11,true); assert.equal(e.indirection(65532n),17n);
dv.setUint32(65532,0x80000000,true);
assert.throws(()=>e.indirection(65532n),WebAssembly.RuntimeError);
assert.throws(()=>e.indirection(65533n),WebAssembly.RuntimeError);
u.fill(0x93); assert.equal(e.recurse(12,7n),0n);
assert(u.every((x,i)=>x===(i===7?0:0x93)));
"#,
    );
}

#[test]
fn review_zero_pages_and_nonwrapping_access_footprints() {
    let source = r#"
func @read(i64,i64)->i64 {
block0(v0:i64,v1:i64):
 v2 = inttoptr.ptr v0
 v3 = ptradd v2,v1
 v4 = load.i64 v3
 return v4
}
func @write(i64,i64,i64) {
block0(v0:i64,v1:i64,v2:i64):
 v3 = inttoptr.ptr v0
 v4 = ptradd v3,v1
 store v2,v4
 return
}
func @ordered_trap(i64) {
block0(v0:i64):
 v1 = iconst.i64 0
 v2 = inttoptr.ptr v1
 v3 = iconst.i8 90
 store volatile v3,v2
 call @write(v0,v1,v0)
 v4 = iconst.i8 123
 store volatile v4,v2
 return
}
"#;
    execute_pages(
        source,
        &["read", "write", "ordered_trap"],
        0,
        r#"
for(const addr of [0n,1n,-1n,0x100000000n]) {
 assert.throws(()=>e.read(addr,0n),WebAssembly.RuntimeError);
 assert.throws(()=>e.write(addr,0n,-1n),WebAssembly.RuntimeError);
}
assert.equal(e.memory.grow(0),0);
"#,
    );
    execute(
        source,
        &["read", "write", "ordered_trap"],
        r#"
const value=-0x123456789abcdefn;
// The address wraps, but the eight-byte footprint never wraps.
for(const [base,offset] of [[-1n,1n],[0n,-4294967296n],
 [0x80000000n,-2147483648n],[0x100000000n,65528n]]) {
 const addr=Number(BigInt.asUintN(32,base+offset));
 u.fill(0xa6); e.write(base,offset,value);
 let bits=BigInt.asUintN(64,value);
 for(let i=0;i<u.length;i++) assert.equal(u[i],i>=addr && i<addr+8
  ? Number((bits>>BigInt(8*(i-addr)))&255n):0xa6);
 assert.equal(e.read(base,offset),value);
}
// Every partial overlap at the upper boundary, plus a would-be 32-bit wrap.
for(const addr of [65529n,65530n,65531n,65532n,65533n,65534n,65535n,
 0xfffffff9n,0xfffffffan,0xfffffffbn,0xfffffffcn,0xfffffffdn,0xfffffffen,0xffffffffn]) {
 u.fill(0xa6);
 assert.throws(()=>e.read(addr,0n),WebAssembly.RuntimeError);
 assert.throws(()=>e.write(addr,0n,value),WebAssembly.RuntimeError);
 assert(u.every(x=>x===0xa6));
}
u.fill(0xa6);
assert.throws(()=>e.ordered_trap(65535n),WebAssembly.RuntimeError);
assert(u.every((x,i)=>x===(i===0?90:0xa6)),
 'effects before a trapping direct call survive; later effects do not execute');
"#,
    );
}

#[test]
fn review_exhaustive_narrow_sign_chains_and_mixed_width_counts() {
    let mut source = String::from(
        r#"
func @chain(i64)->i64 {
block0(v0:i64):
 v1 = trunc.i8 v0
 v2 = sext.i16 v1
 v3 = sext.i32 v2
 v4 = sext.i64 v3
 return v4
}
func @signed16(i64)->i64 {
block0(v0:i64):
 v1 = trunc.i16 v0
 v2 = sext.i64 v1
 return v2
}
"#,
    );
    let mut names = vec!["chain".to_owned(), "signed16".to_owned()];
    for width in [32, 64] {
        for count_width in [8, 16] {
            for op in ["shl", "ushr", "sshr", "rotl", "rotr"] {
                let name = format!("{op}_{width}_{count_width}");
                source += &format!(
                    "func @{name}(i{width},i64)->i{width} {{\nblock0(v0:i{width},v1:i64):\n v2 = trunc.i{count_width} v1\n v3 = {op} v0,v2\n return v3\n}}\n"
                );
                names.push(name);
            }
        }
    }
    execute(
        &source,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        r#"
for(let x=0n;x<65536n;x++) {
 assert.equal(e.signed16(x),BigInt.asIntN(16,x));
 assert.equal(e.chain(x),BigInt.asIntN(8,x));
}
for(const width of [32,64]) for(const cw of [8,16]) {
 const w=BigInt(width), mask=(1n<<w)-1n;
 for(const x of [0n,1n,-1n,1n<<(w-1n),0x123456789abcdefn])
 for(const count of [0n,1n,31n,32n,63n,64n,127n,128n,255n,256n,
 32768n,65535n,65536n,-1n,-9223372036854775808n]) {
  const c=BigInt.asUintN(cw,count)%w, u=BigInt.asUintN(width,x), s=BigInt.asIntN(width,x);
  const wants={shl:u<<c,ushr:u>>c,sshr:s>>c,
   rotl:(u<<c)|(u>>(w-c)),rotr:(u>>c)|(u<<(w-c))};
  for(const [op,v] of Object.entries(wants)) {
   const got=e[`${op}_${width}_${cw}`](width===32?Number(s):s,count);
   const want=BigInt.asIntN(width,v&mask);
   assert.equal(BigInt(got),want,`${op} i${width} i${cw} x=${x} count=${count}`);
  }
 }
}
"#,
    );
}
