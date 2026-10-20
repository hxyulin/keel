pub fn source() -> String {
    let mut source = String::new();
    for wide in [32, 64] {
        for narrow in [8, 16, 32].into_iter().filter(|n| *n < wide) {
            for signed in [false, true] {
                let suffix = if signed { "s" } else { "u" };
                let op = if signed { "sext" } else { "uext" };
                let shift = if wide == 32 { 2 } else { 3 };
                source.push_str(&format!(
                    "func @cast_{wide}_{narrow}_{suffix}(ptr,i64,i64,i{wide}) -> i{wide} cc(c) {{
block0(v0: ptr,v1: i64,v2: i64,v3: i{wide}):
 jump block1(v2,v3)
block1(v4: i64,v5: i{wide}):
 v6 = icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7 = iconst.i64 {shift}
 v8 = shl v4,v7
 v9 = ptradd v0,v8
 v10 = load.i{wide} v9
 v11 = trunc.i{narrow} v10
 v12 = {op}.i{wide} v11
 v13 = iadd v5,v12
 v14 = iconst.i64 1
 v15 = iadd v4,v14
 jump block1(v15,v13)
block3:
 return v5
}}\n"
                ));
            }
        }
    }
    source
}
