pub fn source() -> String {
    let mut source = String::new();
    for wide in [32, 64] {
        let shift = if wide == 32 { 2 } else { 3 };
        let bytes = wide / 8;
        source.push_str(&format!(
            "func @multi_{wide}(ptr,i64,i64,i{wide},i{wide},i{wide},ptr) cc(c) {{
block0(v0: ptr,v1: i64,v2: i64,v3: i{wide},v4: i{wide},v5: i{wide},v6: ptr):
 jump block1(v2,v3,v4,v5)
block1(v7: i64,v8: i{wide},v9: i{wide},v10: i{wide}):
 v11 = icmp.ult v7,v1
 brif v11,block2,block3
block2:
 v12 = iconst.i64 {shift}
 v13 = shl v7,v12
 v14 = ptradd v0,v13
 v15 = load.i{wide} v14
 v16 = trunc.i16 v15
 v17 = sext.i{wide} v16
 v18 = iadd v8,v15
 v19 = iadd v17,v9
 v20 = iconst.i{wide} 1
 v21 = iadd v10,v20
 v22 = iconst.i64 1
 v23 = iadd v7,v22
 jump block1(v23,v18,v19,v21)
block3:
 store v8,v6
 v24 = iconst.i64 {bytes}
 v25 = ptradd v6,v24
 store v9,v25
 v26 = iconst.i64 {}
 v27 = ptradd v6,v26
 store v10,v27
 return
}}\n",
            bytes * 2
        ));
    }
    source
}
