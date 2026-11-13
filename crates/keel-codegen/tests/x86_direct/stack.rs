//! Scalar stack ABI, separate outgoing homes, and large-frame unwind boundaries.
use super::{parse, request};
use keel_codegen::backend_profile;
use object::{Object, ObjectSection};
use std::fmt::Write;

fn source() -> String {
    let signature = std::iter::repeat_n("i64", 10).collect::<Vec<_>>().join(",");
    let params = (0..10)
        .map(|i| format!("v{i}:i64"))
        .collect::<Vec<_>>()
        .join(",");
    let forward = (0..10)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let reverse = (0..10)
        .rev()
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut source = format!(
        "decl @write8(i64,i64,i64,i64,i64,i64,i64,ptr)\n decl @foreign10({signature})->i64\ndecl @trace_large()->i64\ndecl @observer10(i64,i64,i64,i64,i64,i64,i8 sext,i16 uext,i32,ptr)->i64\n"
    );
    for name in ["local10", "fallback10"] {
        writeln!(
            source,
            "func @{name}({signature})->i64 {{\nblock0({params}):"
        )
        .unwrap();
        let mut value = 10;
        let mut sum = 0;
        for argument in 1..10 {
            writeln!(
                source,
                " v{value}=iconst.i64 {}\n v{}=imul v{argument},v{value}\n v{}=iadd v{sum},v{}",
                argument + 1,
                value + 1,
                value + 2,
                value + 1
            )
            .unwrap();
            sum = value + 2;
            value += 3;
        }
        if name == "fallback10" {
            writeln!(
                source,
                " v{value}=bswap v{sum}\n v{}=bswap v{value}",
                value + 1
            )
            .unwrap();
            sum = value + 1;
        }
        writeln!(source, " return v{sum}\n}}").unwrap();
    }
    writeln!(source,"func @large({signature})->i64 {{\n ss0=stack_slot 8192,align 16\n ss1=stack_slot 8,align 8\nblock0({params}):\n v10=stack_addr ss1\n store v9,v10\n v11=call @foreign10({reverse})\n v12=call @local10({forward})\n v13=call @fallback10({reverse})\n v14=call @trace_frame({forward})\n v15=load.i64 v10\n v16=iadd v11,v12\n v17=iadd v16,v13\n v18=iadd v17,v14\n v19=iadd v18,v15\n return v19\n}}").unwrap();
    writeln!(source,"func @trace_frame({signature})->i64 {{\nblock0({params}):\n v10=call @trace_large()\n v11=iadd v10,v9\n return v11\n}}").unwrap();
    let indirect_params = (1..=10)
        .map(|i| format!("v{i}:i64"))
        .collect::<Vec<_>>()
        .join(",");
    let indirect_reverse = (1..=10)
        .rev()
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(source,"func @indirect10(ptr,{signature})->i64 {{\nblock0(v0:ptr,{indirect_params}):\n v11=call_indirect v0({indirect_reverse}):({signature})->i64\n return v11\n}}").unwrap();
    source.push_str("func @mixed10(i64,i64,i64,i64,i64,i64,i8 sext,i16 uext,i32,ptr)->i64 {\nblock0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i8,v7:i16,v8:i32,v9:ptr):\n v10=sext.i64 v6\n v11=uext.i64 v7\n v12=uext.i64 v8\n v13=load.i64 v9\n v14=iadd v0,v1\n v15=iadd v14,v2\n v16=iadd v15,v3\n v17=iadd v16,v4\n v18=iadd v17,v5\n v19=iadd v18,v10\n v20=iadd v19,v11\n v21=iadd v20,v12\n v22=iadd v21,v13\n return v22\n}\n");
    source.push_str("func @narrow_stack(ptr)->i64 {\nblock0(v0:ptr):\n v1=iconst.i64 1\n v2=iconst.i64 2\n v3=iconst.i64 3\n v4=iconst.i64 4\n v5=iconst.i64 5\n v6=iconst.i64 6\n v7=iconst.i8 -1\n v8=iconst.i16 -1\n v9=iconst.i32 -2147483648\n v10=call @mixed10(v1,v2,v3,v4,v5,v6,v7,v8,v9,v0)\n v11=call @observer10(v1,v2,v3,v4,v5,v6,v7,v8,v9,v0)\n v12=iadd v10,v11\n return v12\n}\n");
    source.push_str("func @call_void(ptr,i64)->i64 {\nblock0(v0:ptr,v1:i64):\n v2=iconst.i64 1\n v3=iconst.i64 2\n v4=iconst.i64 3\n v5=iconst.i64 4\n v6=iconst.i64 5\n v7=iconst.i64 6\n call @write8(v2,v3,v4,v5,v6,v7,v1,v0)\n v8=load.i64 v0\n return v8\n}\n");
    source.push_str("func @large_homes(i64)->i64 {\nblock0(v0:i64):\n");
    for i in 1..=600 {
        writeln!(source, " v{i}=iadd v{},v0", i - 1).unwrap();
    }
    source.push_str(" return v600\n}\n");
    source
}

#[test]
fn stack_arguments_and_large_nonleaf_frames_match_c_and_unwind() {
    let source = source();
    super::integer::execute_count(
        "stack-arguments-large-frames",
        &source,
        include_str!("stack_oracle.c"),
        Some(8),
    );
}

#[test]
fn frames_accept_high_displacements_and_reject_ordinary_limit_overflow() {
    let limit = i32::MAX as u32 / 2;
    let module = parse(&format!(
        "func @f()->ptr {{\n ss0=stack_slot {},align 16\n ss1=stack_slot 8,align 8\nblock0:\n v0=stack_addr ss1\n return v0\n}}\n",
        limit - 128
    ));
    let req = request();
    let (result, report) = backend_profile::capture(|| req.program(&module));
    let program = result.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        1
    );
    let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let text = file.section_by_name(".text").unwrap().data().unwrap();
    assert_eq!(&text[..3], &[0x48, 0x81, 0xec]);
    let frame = u32::from_le_bytes(text[3..7].try_into().unwrap());
    assert!(frame > 1_000_000_000 && frame <= limit);
    assert!(
        text.windows(4)
            .any(|bytes| bytes == [0x48, 0x8d, 0x84, 0x24])
    );
    let module = parse(&format!(
        "func @f()->ptr {{\n ss0=stack_slot {},align 16\nblock0:\n v0=stack_addr ss0\n return v0\n}}\n",
        limit + 1
    ));
    let (result, report) = backend_profile::capture(|| req.program(&module));
    assert!(result.unwrap_err().message.contains("too large"));
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
}
