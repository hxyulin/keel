pub fn source() -> String {
    let cases = [
        (
            "positive",
            "v12 = icmp.sgt v10,v11\n v13 = select v12,v10,v11\n v14 = uext.i64 v13",
            "v14",
        ),
        (
            "balance",
            "v12 = icmp.slt v10,v11\n v13 = sext.i64 v10\n v14 = iconst.i64 7\n v15 = select v12,v13,v14",
            "v15",
        ),
        (
            "wrap_signed",
            "v12 = iconst.i32 2147483647\n v13 = iadd v10,v12\n v14 = sext.i64 v13",
            "v14",
        ),
        (
            "wrap_unsigned",
            "v12 = iconst.i32 -1\n v13 = isub v10,v12\n v14 = uext.i64 v13",
            "v14",
        ),
        (
            "nested",
            "v12 = iconst.i32 1\n v13 = iadd v10,v12\n v14 = icmp.ult v13,v10\n v15 = select v14,v13,v10\n v16 = sext.i64 v15\n v17 = iconst.i64 7\n v18 = select v14,v16,v17",
            "v18",
        ),
    ];
    let mut source = String::new();
    for (name, expr, contribution) in cases {
        source += &format!(
            "func @{name}(ptr,i64,i64,i64) -> i64 cc(c) {{
block0(v0: ptr,v1: i64,v2: i64,v3: i64):
 jump block1(v2,v3)
block1(v4: i64,v5: i64):
 v6 = icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7 = iconst.i64 2
 v8 = shl v4,v7
 v9 = ptradd v0,v8
 v10 = load.i32 v9
 v11 = iconst.i32 0
 {expr}
 v30 = iadd v5,{contribution}
 v31 = iconst.i64 1
 v32 = iadd v4,v31
 jump block1(v32,v30)
block3:
 return v5
}}\n"
        );
    }
    source
}
