use keel_ir::{DataItem, Module, parse};
use keel_wasm::{
    Export, Options, Wasm32MemoryOptions, Wasm32RuntimeOptions, emit_module,
    emit_wasm32_memory_module, emit_wasm32_runtime_module,
};
use std::io::Write;
use std::process::{Command, Stdio};

fn options(names: &[&str]) -> Wasm32RuntimeOptions {
    Wasm32RuntimeOptions {
        memory: Wasm32MemoryOptions {
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
        },
        data_floor: 17,
        stack_bottom: 32768,
        stack_top: 65536,
    }
}
fn checked(source: &str) -> Module {
    parse(source).unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn execute(module: &Module, opts: &Wasm32RuntimeOptions, assertions: &str) {
    let bytes = emit_wasm32_runtime_module(module, opts).unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
    let js = format!(
        r#"
const fs=require('fs'), assert=require('assert/strict');
const bytes=fs.readFileSync(0); assert(WebAssembly.validate(bytes));
const mod=new WebAssembly.Module(bytes); assert.deepEqual(WebAssembly.Module.imports(mod),[]);
const instance=new WebAssembly.Instance(mod), e=instance.exports;
const u=new Uint8Array(e.memory.buffer), dv=new DataView(e.memory.buffer);
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
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn aligned_static_data_exact_cursors_bss_and_fixups() {
    let m = checked(
        r#"
data @tag = align 1 { i8 90 }
data @target = local align 16 { b"abcd", zero 5 }
data @record = hidden align 4 { i32 7, ptr @target + 3, i32 287454020, ptr @target + 9, b"\x55\x66" }
data @bss = align 32 { zero 31 }
func @read() -> i32 {
block0:
 v0 = symbol_addr @record
 v1 = iconst.i64 4
 v2 = ptradd v0,v1
 v3 = load.ptr v2
 v4 = load.i8 v3
 v5 = uext.i32 v4
 return v5
}
func @write(i32) {
block0(v0:i32):
 v1 = symbol_addr @bss
 store v0,v1
 return
}
func @record_addr() -> i64 {
block0:
 v0 = symbol_addr @record
 v1 = ptrtoint.i64 v0
 return v1
}
func @target_addr() -> i64 {
block0:
 v0 = symbol_addr @target
 v1 = ptrtoint.i64 v0
 return v1
}
"#,
    );
    execute(
        &m,
        &options(&["record_addr", "target_addr", "read", "write"]),
        r#"
assert.equal(e.record_addr(),44n); assert.equal(e.target_addr(),32n);
assert.equal(u[17],90); assert.deepEqual([...u.slice(32,41)],[97,98,99,100,0,0,0,0,0]);
assert.equal(dv.getInt32(44,true),7); assert.equal(dv.getUint32(48,true),35);
assert.equal(dv.getUint32(52,true),0x11223344); assert.equal(dv.getUint32(56,true),41);
assert.deepEqual([...u.slice(60,62)],[85,102]);
assert(u.slice(64,95).every(x=>x===0)); assert(u.slice(0,17).every(x=>x===0));
assert.equal(e.read(),100);
e.write(0x55667788); assert.equal(dv.getUint32(64,true),0x55667788);
assert.equal(u[63],0); assert.equal(u[68],0);
assert.deepEqual(Object.keys(e).sort(),['memory','read','record_addr','target_addr','write']);
const second=new WebAssembly.Instance(mod).exports;
u[32]=255; assert.equal(new Uint8Array(second.memory.buffer)[32],97);
assert.throws(()=>e.memory.grow(1),RangeError);
"#,
    );
    assert!(emit_module(&m, &Options::default()).is_err());
    assert!(emit_wasm32_memory_module(&m, &options(&[]).memory).is_err());
}
#[test]
fn runtime_rejects_unsupported_data_including_unused_metadata() {
    for source in [
        "decl data @external",
        "data @x = thread_local align 4 { i32 0 }",
        "data @x = weak align 4 { i32 0 }",
        "data @x = weak_odr align 4 { i32 0 }",
        "data @x = retain align 4 { i32 0 }",
        "data @x = align 8 { rtti_name @x }",
        "data @x = align 4 { ptr @f }\nfunc @f() {\nblock0:\n return\n}",
        "data @x = align 4 { i8 0, ptr @x }",
        "data @x = align 4 { ptr @x + 5 }",
        "data @x = align 1 { i8 256 }",
    ] {
        assert!(
            emit_wasm32_runtime_module(&checked(source), &options(&[])).is_err(),
            "{source}"
        );
    }
    let mut m = checked("data @x = align 4 { i32 0 }");
    m.data[0].section = Some(keel_ir::SectionName::new(".custom").unwrap());
    assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
    for ty in [keel_ir::Type::Ptr, keel_ir::Type::F64, keel_ir::Type::I128] {
        m.data[0].section = None;
        m.data[0].init.as_mut().unwrap().items = vec![DataItem::Int { ty, value: 0 }];
        assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
    }
}
#[test]
fn bounded_layout_configuration_and_sparse_large_bss() {
    let empty = Module::default();
    for (floor, bottom, top) in [
        (0, 32, 64),
        (33, 32, 64),
        (17, 48, 32),
        (17, 32, 65552),
        (17, 31, 64),
        (17, 32, 63),
    ] {
        let mut opts = options(&[]);
        opts.data_floor = floor;
        opts.stack_bottom = bottom;
        opts.stack_top = top;
        assert!(emit_wasm32_runtime_module(&empty, &opts).is_err());
    }
    let mut opts = options(&[]);
    let m = checked("data @large = align 16 { zero 4294967295 }");
    assert!(
        emit_wasm32_runtime_module(&m, &opts)
            .unwrap_err()
            .0
            .contains("overlaps")
    );
    // Near-4-GiB BSS is validated/encoded without materializing it or instantiating.
    opts.memory.pages = 65536;
    opts.data_floor = 16;
    opts.stack_bottom = 0xfffffff0;
    opts.stack_top = 0xfffffff0;
    let m = checked("data @large = align 16 { zero 4294967264 }");
    let bytes = emit_wasm32_runtime_module(&m, &opts).unwrap();
    assert!(bytes.len() < 200);
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .unwrap();
}
#[test]
fn representable_one_past_memory_fixup_and_oob_dereference() {
    let m = checked(
        r#"
data @p = align 4 { ptr @end + 65504 }
data @end = align 1 { zero 65504 }
func @read() -> i32 {
block0:
 v0 = symbol_addr @p
 v1 = load.ptr v0
 v2 = load.i32 v1
 return v2
}
"#,
    );
    let mut opts = options(&["read"]);
    opts.data_floor = 28;
    opts.stack_bottom = 65536;
    opts.stack_top = 65536;
    execute(
        &m,
        &opts,
        "assert.equal(dv.getUint32(28,true),65536); assert.throws(()=>e.read(),WebAssembly.RuntimeError);",
    );
}

#[test]
fn static_frames_nested_recursion_alignment_and_normal_returns() {
    let m = checked(
        r#"
data @global = align 16 { i64 123 }
func @addr() -> i64 {
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = ptrtoint.i64 v0
 return v1
}
func @choose(i32) -> i64 {
 ss0 = stack_slot 3, align 1
 ss1 = stack_slot 17, align 16
block0(v0:i32):
 v1 = stack_addr ss1
 v2 = ptrtoint.i64 v1
 brif v0, block1(v2), block2
block1(v3:i64):
 return v3
block2:
 v4 = call @addr()
 return v4
}
func @recur(i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0:i64):
 v1 = stack_addr ss0
 store v0,v1
 v2 = iconst.i64 0
 v3 = icmp.eq v0,v2
 brif v3, block1, block2
block1:
 return v2
block2:
 v4 = iconst.i64 1
 v5 = isub v0,v4
 v6 = call @recur(v5)
 v7 = load.i64 v1
 v8 = iadd v6,v7
 return v8
}
func @touch() {
 ss0 = stack_slot 16, align 16
block0:
 v0 = call @addr()
 v1 = symbol_addr @global
 store v0,v1
 return
}
func @slot_offsets() -> i64 {
 ss0 = stack_slot 3, align 1
 ss1 = stack_slot 2, align 2
 ss2 = stack_slot 8, align 8
block0:
 v0 = stack_addr ss0
 v1 = stack_addr ss1
 v2 = stack_addr ss2
 v3 = ptrtoint.i64 v0
 v4 = ptrtoint.i64 v1
 v5 = ptrtoint.i64 v2
 v6 = isub v4,v3
 v7 = isub v5,v3
 v8 = iconst.i64 100
 v9 = imul v6,v8
 v10 = iadd v9,v7
 return v10
}
"#,
    );
    execute(
        &m,
        &options(&["addr", "choose", "recur", "touch", "slot_offsets"]),
        r#"
assert.equal(e.addr(),65520n);
for(let i=0;i<80;i++) {
 assert.equal(e.choose(1),65504n);
 assert.equal(e.choose(0),65472n);
 assert.equal(e.addr(),65520n);
 assert.equal(e.recur(BigInt(i)),BigInt(i*(i+1)/2));
 assert.equal(e.addr(),65520n);
 e.touch(); assert.equal(dv.getBigInt64(32,true),65504n);
 assert.equal(e.addr(),65520n); assert.equal(e.slot_offsets(),408n);
}
assert.equal(u[31],0);
"#,
    );
    assert!(emit_wasm32_memory_module(&m, &options(&[]).memory).is_err());
}

#[test]
fn exhaustion_checks_before_frame_stores_and_preserves_data_canaries() {
    let m = checked(
        r#"
data @guard = align 16 { zero 65504 }
func @recur(i32) -> i32 {
 ss0 = stack_slot 8, align 8
block0(v0:i32):
 v1 = stack_addr ss0
 store v0,v1
 brif v0, block1, block2
block1:
 v2 = iconst.i32 1
 v3 = isub v0,v2
 v4 = call @recur(v3)
 v5 = load.i32 v1
 v6 = iadd v4,v5
 return v6
block2:
 return v0
}
"#,
    );
    let mut opts = options(&["recur"]);
    opts.data_floor = 16;
    opts.stack_bottom = 65520;
    execute(
        &m,
        &opts,
        r#"
u.fill(0xa5,16,65520);
assert.equal(e.recur(0),0);
assert.throws(()=>e.recur(1),WebAssembly.RuntimeError);
assert(u.slice(16,65520).every(x=>x===0xa5));
// Discard the trapped instance; no SP recovery or further call is attempted.
"#,
    );
    opts = options(&["f"]);
    opts.data_floor = 16;
    opts.stack_bottom = 16;
    opts.stack_top = 32;
    for size in [48u32, 0x80000000, 0xfffffff0] {
        let underflow = checked(&format!(
            "func @f() {{\n ss0=stack_slot {size}, align 16\nblock0:\n v0=stack_addr ss0\n v1=iconst.i32 123\n store v1,v0\n return\n}}"
        ));
        execute(
            &underflow,
            &opts,
            "u.fill(0x5a); assert.throws(()=>e.f(),WebAssembly.RuntimeError); assert(u.every(x=>x===0x5a));",
        );
    }
}

#[test]
fn stack_overflow_metadata_and_unsupported_operations_are_rejected() {
    let mut m = checked("func @f() {\n ss0=stack_slot 1, align 16\nblock0:\n return\n}");
    m.functions[0].create_stack_slot(1, 32);
    assert!(
        emit_wasm32_runtime_module(&m, &options(&[]))
            .unwrap_err()
            .0
            .contains("alignment")
    );
    let mut m = checked("func @f() {\nblock0:\n return\n}");
    m.functions[0].create_stack_slot(u32::MAX, 16);
    assert!(
        emit_wasm32_runtime_module(&m, &options(&[]))
            .unwrap_err()
            .0
            .contains("frame")
    );
    m.functions[0].create_stack_slot(u32::MAX, 16);
    assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
    for body in [
        "v0 = symbol_addr @f",
        "v0 = intrinsic stack_save()",
        "v0 = iconst.i64 16\n v1 = intrinsic stack_alloc(v0)",
    ] {
        let m = checked(&format!("func @f() {{\nblock0:\n {body}\n return\n}}"));
        assert!(
            emit_wasm32_runtime_module(&m, &options(&[])).is_err(),
            "{body}"
        );
    }
}

#[test]
fn high_bit_static_addresses_and_fixup_bits_without_large_instantiation() {
    let m = checked("data @p = align 4 { ptr @p + 8, i32 305419896 }");
    let mut opts = options(&[]);
    opts.memory.pages = 65536;
    opts.data_floor = 0x80000000;
    opts.stack_bottom = 0xfffffff0;
    opts.stack_top = 0xfffffff0;
    let bytes = emit_wasm32_runtime_module(&m, &opts).unwrap();
    let mut segments = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::DataSection(section) = payload.unwrap() {
            for segment in section {
                let segment = segment.unwrap();
                let wasmparser::DataKind::Active {
                    memory_index,
                    offset_expr,
                } = segment.kind
                else {
                    panic!("passive data");
                };
                assert_eq!(memory_index, 0);
                let mut reader = offset_expr.get_operators_reader();
                let wasmparser::Operator::I32Const { value } = reader.read().unwrap() else {
                    panic!("nonconstant offset");
                };
                segments.push((value as u32, segment.data.to_vec()));
            }
        }
    }
    assert_eq!(
        segments,
        vec![
            (0x80000000, vec![8, 0, 0, 128]),
            (0x80000004, vec![0x78, 0x56, 0x34, 0x12])
        ]
    );
}

#[test]
fn runtime_retains_scalar_call_abi_and_rejects_unused_unsupported_contracts() {
    for source in [
        "func @f(ptr) {\nblock0(v0:ptr):\n return\n}",
        "func @f() -> i8 {\nblock0:\n v0=iconst.i8 0\n return v0\n}",
        "decl @external()",
        "func @f() {\nblock0:\n v0=intrinsic stack_save()\n intrinsic stack_restore(v0)\n return\n}",
    ] {
        assert!(
            emit_wasm32_runtime_module(&checked(source), &options(&[])).is_err(),
            "{source}"
        );
    }
    let empty = Module::default();
    let mut opts = options(&[]);
    opts.memory.export_name.clear();
    assert!(emit_wasm32_runtime_module(&empty, &opts).is_err());
    opts = options(&[]);
    opts.memory.pages = 65537;
    assert!(emit_wasm32_runtime_module(&empty, &opts).is_err());
}

#[test]
fn integer_initializer_footprints_are_exact_little_endian() {
    let m = checked(
        r#"
data @ints = align 1 { i8 -1, i16 -2, i32 -3, i64 -4, b"\x5a", zero 3, i16 65535 }
data @next = align 1 { i8 51 }
"#,
    );
    execute(
        &m,
        &options(&[]),
        r#"
assert.deepEqual([...u.slice(17,20)],[255,254,255]);
assert.equal(dv.getInt32(20,true),-3); assert.equal(dv.getBigInt64(24,true),-4n);
assert.deepEqual([...u.slice(32,39)],[90,0,0,0,255,255,51]);
assert.equal(u[16],0); assert.equal(u[39],0);
"#,
    );
}

#[test]
fn review_stack_pointer_aliases_parallel_phi_cycles_and_callee_frames() {
    let mut m = checked(
        r#"
func @cycle(i32) -> i64 {
 ss0 = stack_slot 8, align 8
 ss1 = stack_slot 8, align 8
block0(v0:i32):
 v1 = stack_addr ss0
 v2 = stack_addr ss1
 v3 = stack_addr ss0
 v4 = iconst.i64 11
 v5 = iconst.i64 22
 store v4,v1
 store v5,v3
 jump block2(v1,v2,v0)
block1:
 return v4
block2(v6:ptr,v7:ptr,v8:i32):
 brif v8, block3, block4(v6,v7)
block3:
 v9 = iconst.i32 1
 v10 = isub v8,v9
 jump block2(v7,v6,v10)
block4(v11:ptr,v12:ptr):
 call @leaf()
 v13 = load.i64 v11
 v14 = load.i64 v12
 v15 = iconst.i64 100
 v16 = imul v13,v15
 v17 = iadd v16,v14
 return v17
}
func @leaf() {
 ss0 = stack_slot 16, align 16
block0:
 v0 = stack_addr ss0
 v1 = iconst.i64 -1
 store v1,v0
 return
}
"#,
    );
    let f = &mut m.functions[0];
    let insts = f.block_insts(f.entry_block().unwrap()).to_vec();
    // Keep the aliased StackAddr producer in layout: its raw destination must
    // not overwrite the replacement's local with the other slot's address.
    f.replace_all_uses(f.first_result(insts[2]), f.first_result(insts[1]));
    let gap = f.blocks().nth(1).unwrap();
    f.remove_block(gap);
    execute(
        &m,
        &options(&["cycle"]),
        r#"
for(let n=0;n<40;n++) {
 assert.equal(e.cycle(n), n%2 ? 2211n : 1122n);
 assert.equal(dv.getBigInt64(65520,true),11n);
 assert.equal(dv.getBigInt64(65528,true),22n);
 assert.equal(dv.getBigInt64(65504,true),-1n);
 assert.equal(u[65503],0);
}
"#,
    );
}

#[test]
fn review_zero_frames_restore_sp_and_allow_an_empty_stack_region() {
    let m = checked(
        r#"
func @empty(i32) -> i64 {
block0(v0:i32):
 brif v0, block1, block2
block1:
 v1 = iconst.i64 123
 return v1
block2:
 v2 = call @framed()
 return v2
}
func @framed() -> i64 {
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = ptrtoint.i64 v0
 return v1
}
"#,
    );
    let mut opts = options(&["empty"]);
    opts.stack_bottom = 65520;
    execute(
        &m,
        &opts,
        "for(let i=0;i<100;i++) { assert.equal(e.empty(0),65520n); assert.equal(e.empty(1),123n); }",
    );
    // An unused framed definition does not reserve space until called.
    opts.stack_bottom = opts.stack_top;
    execute(
        &m,
        &opts,
        "assert.equal(e.empty(1),123n); assert.throws(()=>e.empty(0),WebAssembly.RuntimeError);",
    );
}

#[test]
fn review_malformed_runtime_references_return_diagnostics() {
    use keel_ir::{DataRef, EntityRef, InstData, StackSlot, Symbol};
    let original = checked(
        "data @x = align 4 { i32 7 }\nfunc @f() {\n ss0=stack_slot 4, align 4\nblock0:\n v0=stack_addr ss0\n v1=symbol_addr @x\n return\n}",
    );
    for invalid in [
        InstData::StackAddr {
            slot: StackSlot::new(u32::MAX as usize),
        },
        InstData::SymbolAddr {
            symbol: Symbol::Data(DataRef::new(u32::MAX as usize)),
        },
    ] {
        let mut m = original.clone();
        let f = &mut m.functions[0];
        let inst = f.block_insts(f.entry_block().unwrap())[0];
        *f.inst_data_mut(inst) = invalid;
        assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
    }
    let mut m = original.clone();
    m.functions[0].import_data("undeclared");
    assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
    let mut m = original;
    m.data[0].init.as_mut().unwrap().align = 0;
    assert!(emit_wasm32_runtime_module(&m, &options(&[])).is_err());
}
