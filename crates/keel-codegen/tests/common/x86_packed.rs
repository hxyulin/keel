// x86-only oracle: deliberately retain both sources across a call and comparison.
pub fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "typedef __INT8_TYPE__ int8_t; typedef __UINT8_TYPE__ uint8_t;\ntypedef __INT16_TYPE__ int16_t; typedef __UINT16_TYPE__ uint16_t;\ntypedef __INT32_TYPE__ int32_t; typedef __UINT32_TYPE__ uint32_t;\ntypedef __INT64_TYPE__ int64_t; typedef __UINT64_TYPE__ uint64_t;\nextern void *memcpy(void *, const void *, __SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void) {\n");
    for (shape, ty, uty, lanes) in [
        ("i8x16", "int8_t", "uint8_t", 16),
        ("i16x8", "int16_t", "uint16_t", 8),
        ("i32x4", "int32_t", "uint32_t", 4),
        ("i64x2", "int64_t", "uint64_t", 2),
    ] {
        c += &format!(
            "typedef {ty} V_{shape} __attribute__((vector_size(16)));\n__attribute__((noinline)) V_{shape} opaque_{shape}(V_{shape} x) {{ __asm__ volatile(\"\" : \"+x\"(x)); return x; }}\n"
        );
        ir += &format!("decl @opaque_{shape}({shape}) -> {shape}\n");
        for (cc, symbol, unsigned) in [
            ("eq", "==", false),
            ("ne", "!=", false),
            ("slt", "<", false),
            ("sle", "<=", false),
            ("sgt", ">", false),
            ("sge", ">=", false),
            ("ult", "<", true),
            ("ule", "<=", true),
            ("ugt", ">", true),
            ("uge", ">=", true),
        ] {
            for reverse in [false, true] {
                let name = format!("packed_{shape}_{cc}_{}", u8::from(reverse));
                let (a, b) = if reverse { (6, 4) } else { (4, 6) };
                ir += &format!(
                    "func @{name}(ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v6 = call @opaque_{shape}(v3)\n v7 = vicmp.{cc} v{a}, v{b}\n store align 1 v7, v2\n v8 = iconst.i64 16\n v9 = ptradd v2, v8\n store align 1 v4, v9\n v10 = iconst.i64 32\n v11 = ptradd v2, v10\n store align 1 v6, v11\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                let cast = if unsigned { uty } else { ty };
                let (left, right) = if reverse { ("a", "b") } else { ("b", "a") };
                // Cartesian boundaries cover high equality, low unsigned ordering,
                // sign transitions, and opposing outcomes in the two i64 lanes.
                main += &format!(
                    "{{ uint64_t edge[]={{0,1,2,0x7fffffffULL,0x80000000ULL,0xffffffffULL,0x100000000ULL,0x100000001ULL,0x17fffffffULL,0x180000000ULL,0x1ffffffffULL,0x7fffffff00000000ULL,0x7fffffffffffffffULL,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,0xffffffff7fffffffULL,0xffffffff80000000ULL,0xfffffffffffffffeULL,0xffffffffffffffffULL}};\nfor(unsigned i=0;i<20;i++) for(unsigned j=0;j<20;j++) {{ {ty} a[{lanes}],b[{lanes}]; unsigned char input_a[17],input_b[17],output[49];\nfor(unsigned l=0;l<{lanes};l++) {{a[l]=({ty})edge[(i+7*l)%20]; b[l]=({ty})edge[(j+11*l)%20];}}\nmemcpy(input_a+1,a,16);memcpy(input_b+1,b,16);{name}(input_a+1,input_b+1,output+1);\n{ty} got[3*{lanes}];memcpy(got,output+1,48); for(unsigned l=0;l<{lanes};l++) {{ {ty} expected=(({cast}){left}[l] {symbol} ({cast}){right}[l]) ? ({ty})-1:0; if(got[l]!=expected||got[{lanes}+l]!=b[l]||got[2*{lanes}+l]!=a[l]) {{return 1;}} }} }} }}\n"
                );
            }
        }
    }
    main += "return 0; }\n";
    (ir, c + &main)
}
