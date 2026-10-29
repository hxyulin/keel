use keel_ir::{Type, VectorType};
use std::{fs, path::PathBuf, process::Command};

pub fn operations(shape: VectorType) -> Vec<&'static str> {
    if shape.lane_type().is_float() {
        vec![
            "vfpadd",
            "vfpmin",
            "vfpmax",
            "vfadd",
            "vfsub",
            "vfmul",
            "vfdiv",
            "vfmin",
            "vfmax",
            "vfcopysign",
            "vfneg",
            "vfabs",
            "vfsqrt",
            "vfceil",
            "vffloor",
            "vftrunc",
            "vfnearest",
            "vfcmp.eq",
            "vfcmp.ne",
            "vfcmp.lt",
            "vfcmp.le",
            "vfcmp.gt",
            "vfcmp.ge",
            "vfcmp.ord",
            "vfcmp.uno",
        ]
    } else {
        let mut v = vec![
            "vpadd",
            "vpsmin",
            "vpsmax",
            "vpumin",
            "vpumax",
            "viadd",
            "visub",
            "vimul",
            "vsmin",
            "vsmax",
            "vumin",
            "vumax",
            "vsabd",
            "vuabd",
            "vshadd",
            "vuhadd",
            "vsrhadd",
            "vurhadd",
            "vshsub",
            "vuhsub",
            "vsadd_sat",
            "vuadd_sat",
            "vssub_sat",
            "vusub_sat",
            "vand",
            "vor",
            "vxor",
            "vshl",
            "vushr",
            "vsshr",
            "vineg",
            "vbnot",
            "vclz",
            "vctz",
            "vpopcnt",
            "vicmp.eq",
            "vicmp.ne",
            "vicmp.slt",
            "vicmp.sle",
            "vicmp.sgt",
            "vicmp.sge",
            "vicmp.ult",
            "vicmp.ule",
            "vicmp.ugt",
            "vicmp.uge",
        ];
        if shape.lane_type() != Type::I8 {
            v.push("vbswap");
        }
        v
    }
}

pub fn binary(op: &str) -> bool {
    op.starts_with("vicmp.")
        || op.starts_with("vfcmp.")
        || op.parse::<keel_ir::VectorBinaryOp>().is_ok()
}

pub fn name(shape: VectorType, op: &str) -> String {
    format!("{}_{}", shape, op.replace('.', "_"))
}

pub fn fixture() -> String {
    fixture_for(&VectorType::ALL_128)
}
pub fn fixture_for(shapes: &[VectorType]) -> String {
    let mut s = String::new();
    for &shape in shapes {
        let bytes = shape.bytes();
        let buffer = bytes + 1;
        let _ = (bytes, buffer);
        for op in operations(shape) {
            s += &format!(
                "func @{}(ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = {op} v3{}\n store align 1 v5, v2\n return\n}}\n",
                name(shape, op),
                if binary(op) { ", v4" } else { "" }
            );
        }
        let n = shape.lanes();
        let lane = shape.lane_type();
        let indices = (0..n)
            .map(|i| (if i % 2 == 0 { n - 1 - i } else { 2 * n - 1 - i }).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        s += &format!(
            "func @lanes_{shape}(ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vextract v4, {}\n v6 = vsplat.{shape} v5\n v7 = vinsert v6, v5, 0\n v8 = vshuffle v3, v7, [{indices}]\n store align 1 v8, v2\n return\n}}\n",
            n - 1
        );
        // Mask need not be canonical; all nonzero lane bits mean true.
        s += &format!(
            "func @select_{shape}(ptr, ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr, v3: ptr):\n v4 = load.{} align 1 v0\n v5 = load.{shape} align 1 v1\n v6 = load.{shape} align 1 v2\n v7 = vselect v4, v5, v6\n store align 1 v7, v3\n return\n}}\n",
            shape.mask()
        );
        s += &format!(
            "func @scalar_select_{shape}(i64, {shape}, {shape}) -> {shape} {{\nblock0(v0: i64, v1: {shape}, v2: {shape}):\n v3 = select v0, v1, v2\n return v3\n}}\n"
        );
        s += &format!(
            "func @insert_{shape}(ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{lane} align 1 v1\n v5 = vinsert v3, v4, {}\n store align 1 v5, v2\n return\n}}\n",
            n - 1
        );
    }
    s += "func @bits(i128) -> i128 {\nblock0(v0: i128):\n v1 = bitcast.i8x16 v0\n v2 = bitcast.f64x2 v1\n v3 = bitcast.i128 v2\n return v3\n}\n";
    s
}

pub fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("keel-vector-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn command(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{c:?}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Whether an entry of a NEON selector × allocator × pool matrix also compares
/// its `__text` with clang's assembly of the same text. One entry per selector
/// uses the normal registers, covering both allocators, and Full/Full adds the
/// spill pool. Typed/text parity and native execution still run in every entry.
pub fn clang_parity(
    selector: keel_codegen::SelectionTier,
    allocator: keel_codegen::regalloc::AllocationTier,
    spilled: bool,
) -> bool {
    use keel_codegen::{SelectionTier as S, regalloc::AllocationTier as A};
    matches!(
        (selector, allocator, spilled),
        (S::Baseline, A::Fast, false) | (S::Trees, A::Full, false) | (S::Full, A::Full, _)
    )
}

pub fn harness() -> String {
    harness_for(&VectorType::ALL_128)
}
pub fn harness_for(shapes: &[VectorType]) -> String {
    let mut s = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <math.h>\n#include <limits.h>\n",
    );
    for &shape in shapes {
        let bytes = shape.bytes();
        let buffer = bytes + 1;
        let _ = (bytes, buffer);
        for op in operations(shape) {
            s += &format!(
                "extern void {}(const void*,const void*,void*);\n",
                name(shape, op)
            );
        }
        s += &format!(
            "extern void lanes_{shape}(const void*,const void*,void*);\nextern void insert_{shape}(const void*,const void*,void*);\nextern void select_{shape}(const void*,const void*,const void*,void*);\n"
        );
    }
    s += "static __int128 floor_half(__int128 x){return x/2-(x<0&&x%2!=0); }\n";
    s += "extern __int128 bits(__int128);\nint main(void) {\nuint64_t patterns[]={0,1,2,7,8,15,16,31,32,63,64,127,128,255,256,0xffff,0x8000,0x80000000,0x8000000000000000ull,0xffffffffffffffffull,0x0123456789abcdefull};\n";
    for &shape in shapes {
        let bytes = shape.bytes();
        let buffer = bytes + 1;
        let _ = (bytes, buffer);
        let width = shape.lane_type().bytes().unwrap() * 8;
        let n = shape.lanes();
        let u = format!("uint{width}_t");
        let signed = format!("int{width}_t");
        let float = shape.lane_type().is_float();
        let cty = if float {
            if width == 32 {
                "float".to_string()
            } else {
                "double".to_string()
            }
        } else {
            u.clone()
        };
        s += &format!(
            "{{ {cty} a[{n}],b[{n}],r[{n}]; {u} mask[{n}]; unsigned char aa[{buffer}],bb[{buffer}],rr[{buffer}],mm[{buffer}];\n"
        );
        if float {
            s += &format!(
                "{cty} values[]={{0.0,-0.0,1.0,-1.0,1.5,-2.5,0.5,-0.5,INFINITY,-INFINITY,NAN,0x1p-126,0x1p-149,3.141592653589793,16777217.0}};\nfor(unsigned k=0;k<sizeof(values)/sizeof(values[0]);k++) {{ for(unsigned i=0;i<{n};i++) {{a[i]=values[(k+i)%15];b[i]=values[(k*3+i+2)%15];mask[i]=(i+k)%3==0?0:({u})patterns[(k+i)%21];}}\n"
            );
        } else {
            s += &format!(
                "for(unsigned k=0;k<21;k++) {{for(unsigned i=0;i<{n};i++) {{a[i]=({u})patterns[(k+i)%21];b[i]=({u})patterns[(k*3+i+2)%21];mask[i]=(i+k)%3==0?0:({u})patterns[(k+i)%21];}}\n"
            );
        }
        s += &format!("memcpy(aa+1,a,{bytes});memcpy(bb+1,b,{bytes});memcpy(mm+1,mask,{bytes});\n");
        for op in operations(shape) {
            let comparison = op.contains("cmp.");
            let out_ty = if comparison { u.clone() } else { cty.clone() };
            s += &format!(
                "{}(aa+1,bb+1,rr+1); {{ {out_ty} out[{n}];memcpy(out,rr+1,{bytes});for(unsigned i=0;i<{n};i++) {{\n",
                name(shape, op)
            );
            let pair = op
                .parse::<keel_ir::VectorBinaryOp>()
                .ok()
                .and_then(|p| p.pairwise_binary());
            let x = if pair.is_some() {
                format!("(i<{0}?a[2*i]:b[2*(i-{0})])", n / 2)
            } else {
                "a[i]".to_string()
            };
            let y = if pair.is_some() {
                format!("(i<{0}?a[2*i+1]:b[2*(i-{0})+1])", n / 2)
            } else {
                "b[i]".to_string()
            };
            let operation = pair
                .map(|p| p.to_string())
                .unwrap_or_else(|| op.to_string());
            let expression = match operation.as_str() {
                "viadd" => format!("({u})((uint64_t){x}+(uint64_t){y})"),
                "visub" => format!("({u})((uint64_t){x}-(uint64_t){y})"),
                "vimul" => format!("({u})((uint64_t){x}*(uint64_t){y})"),
                "vsmin" => format!("({signed}){x}<({signed}){y}?{x}:{y}"),
                "vsmax" => format!("({signed}){x}>({signed}){y}?{x}:{y}"),
                "vumin" => format!("{x}<{y}?{x}:{y}"),
                "vumax" => format!("{x}>{y}?{x}:{y}"),
                "vsabd" => format!(
                    "({signed}){x}<({signed}){y}?(__int128)({signed}){y}-(__int128)({signed}){x}:(__int128)({signed}){x}-(__int128)({signed}){y}"
                ),
                "vuabd" => format!("{x}<{y}?{y}-{x}:{x}-{y}"),
                "vshadd" | "vshsub" | "vsrhadd" => format!(
                    "floor_half((__int128)({signed}){x}{}(__int128)({signed}){y}{})",
                    if op == "vshsub" { "-" } else { "+" },
                    if op == "vsrhadd" { "+1" } else { "" }
                ),
                "vuhadd" | "vuhsub" | "vurhadd" => format!(
                    "floor_half((__int128){x}{}(__int128){y}{})",
                    if op == "vuhsub" { "-" } else { "+" },
                    if op == "vurhadd" { "+1" } else { "" }
                ),
                "vsadd_sat" | "vssub_sat" => {
                    let v = format!(
                        "((__int128)({signed}){x}{}(__int128)({signed}){y})",
                        if op == "vssub_sat" { "-" } else { "+" }
                    );
                    let min = format!("(-((__int128)1<<{}))", width - 1);
                    let max = format!("(((__int128)1<<{})-1)", width - 1);
                    format!("{v}<{min}?{min}:({v}>{max}?{max}:{v})")
                }
                "vuadd_sat" => format!("(__uint128_t){x}+{y}>({u})~({u})0?({u})~({u})0:{x}+{y}"),
                "vusub_sat" => format!("{x}<{y}?0:{x}-{y}"),
                "vand" => format!("{x}&{y}"),
                "vor" => format!("{x}|{y}"),
                "vxor" => format!("{x}^{y}"),
                "vshl" => format!("({u})((uint64_t){x}<<({y}&{}))", width - 1),
                "vushr" => format!("{x}>>({y}&{})", width - 1),
                "vsshr" => format!("({u})(({signed}){x}>>({y}&{}))", width - 1),
                "vineg" => format!("({u})(0ull-(uint64_t){x})"),
                "vbnot" => format!("({u})~{x}"),
                "vclz" => format!("{x}?__builtin_clzll((uint64_t){x})-{}:{width}", 64 - width),
                "vctz" => format!("{x}?__builtin_ctzll((uint64_t){x}):{width}"),
                "vpopcnt" => format!("__builtin_popcountll((uint64_t){x})"),
                "vbswap" => {
                    if width == 16 {
                        format!("({u})(({x}>>8)|({x}<<8))")
                    } else {
                        format!("__builtin_bswap{width}({x})")
                    }
                }
                "vfadd" => format!("{x}+{y}"),
                "vfsub" => format!("{x}-{y}"),
                "vfmul" => format!("{x}*{y}"),
                "vfdiv" => format!("{x}/{y}"),
                "vfmin" => format!(
                    "isnan({x})||isnan({y})?NAN:({x}==0&&{y}==0?(signbit({x})||signbit({y})?-0.0:0.0):({x}<{y}?{x}:{y}))"
                ),
                "vfmax" => format!(
                    "isnan({x})||isnan({y})?NAN:({x}==0&&{y}==0?(signbit({x})&&signbit({y})?-0.0:0.0):({x}>{y}?{x}:{y}))"
                ),
                "vfcopysign" => format!("copysign({x},{y})"),
                "vfneg" => format!("-{x}"),
                "vfabs" => format!("fabs({x})"),
                "vfsqrt" => format!("{}({x})", if width == 32 { "sqrtf" } else { "sqrt" }),
                "vfceil" => format!("ceil({x})"),
                "vffloor" => format!("floor({x})"),
                "vftrunc" => format!("trunc({x})"),
                "vfnearest" => format!("nearbyint({x})"),
                _ if comparison => {
                    let cc = op.split('.').nth(1).unwrap();
                    let mut x = x.to_string();
                    let mut y = y.to_string();
                    if cc.starts_with('s') {
                        x = format!("({signed}){x}");
                        y = format!("({signed}){y}");
                    }
                    let condition = match cc.trim_start_matches(['s', 'u']) {
                        "eq" => format!("{x}=={y}"),
                        "ne" => format!("{x}!={y}"),
                        "lt" => format!("{x}<{y}"),
                        "le" => format!("{x}<={y}"),
                        "gt" => format!("{x}>{y}"),
                        "ge" => format!("{x}>={y}"),
                        "ord" => format!("!isnan({x})&&!isnan({y})"),
                        "no" => format!("isnan({x})||isnan({y})"),
                        _ => panic!("{cc}"),
                    };
                    format!("({condition})?({u})~({u})0:0")
                }
                _ => panic!("{op}"),
            };
            s += &format!("{out_ty} expected=({expression});\n");
            let check = if float && !comparison {
                "((isnan(out[i])&&isnan(expected)) || (out[i]==expected && (out[i]!=0 || !!signbit(out[i])==!!signbit(expected))))"
            } else {
                "out[i]==expected"
            };
            s += &format!(
                "if(!({check})) {{fprintf(stderr,\"{} k=%u lane=%u actual=%Lg expected=%Lg\\n\",k,i,(long double)out[i],(long double)expected);return 1;}} }} }}\n",
                name(shape, op)
            );
        }
        s += &format!(
            "lanes_{shape}(aa+1,bb+1,rr+1);memcpy(r,rr+1,{bytes});for(unsigned i=0;i<{n};i++){{{cty} e=i%2==0?a[{n}-1-i]:b[{n}-1];if(memcmp(&e,&r[i],sizeof(e)))return 2;}}\n"
        );
        s += &format!(
            "insert_{shape}(aa+1,bb+1,rr+1);memcpy(r,rr+1,{bytes});for(unsigned i=0;i<{n};i++){{{cty} e=i=={n}-1?b[0]:a[i];if(memcmp(&e,&r[i],sizeof(e)))return 3;}}\n"
        );
        s += &format!(
            "select_{shape}(mm+1,aa+1,bb+1,rr+1);memcpy(r,rr+1,{bytes});for(unsigned i=0;i<{n};i++){{{cty} e=mask[i]?a[i]:b[i];if(memcmp(&e,&r[i],sizeof(e)))return 4;}}\n"
        );
        s += "} }\n";
    }
    s += "for(unsigned i=0;i<21;i++){__uint128_t x=((__uint128_t)patterns[i]<<64)|patterns[20-i];if((__uint128_t)bits((__int128)x)!=x)return 5;}\nreturn 0;}\n";
    s
}

pub fn abi_fixture(arm64: bool) -> (String, String) {
    abi_fixture_for(arm64, &VectorType::ALL_128)
}
pub fn abi_fixture_for(arm64: bool, shapes: &[VectorType]) -> (String, String) {
    let clobber = if arm64 {
        r#"__asm__ volatile("movi v31.16b,#0\n\tins v8.d[1],xzr\n\tins v9.d[1],xzr\n\tins v10.d[1],xzr\n\tins v11.d[1],xzr\n\tins v12.d[1],xzr\n\tins v13.d[1],xzr\n\tins v14.d[1],xzr\n\tins v15.d[1],xzr":::"v0","v1","v2","v3","v4","v5","v6","v7","v8","v9","v10","v11","v12","v13","v14","v15","v16","v17","v18","v19","v20","v21","v22","v23","v24","v25","v26","v27","v28","v29","v30","v31");"#
    } else {
        r#"__asm__ volatile("pxor %%xmm15,%%xmm15"::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7","xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15");"#
    };
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdarg.h>\nextern int memcmp(const void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void){\n");
    for &shape in shapes {
        let bytes = shape.bytes();
        let lane = shape.lane_type();
        let width = lane.bytes().unwrap() * 8;
        let cty = if lane.is_float() {
            if width == 32 {
                "float".into()
            } else {
                "double".into()
            }
        } else {
            format!("uint{width}_t")
        };
        let op = if lane.is_float() { "vfadd" } else { "viadd" };
        let types = std::iter::repeat_n(shape.to_string(), 10)
            .collect::<Vec<_>>()
            .join(", ");
        let params = (0..10)
            .map(|i| format!("v{i}: {shape}"))
            .collect::<Vec<_>>()
            .join(", ");
        ir += &format!(
            "decl @external_{shape}({shape}) -> {shape}\ndecl @variadic_{shape}(i64, ...) -> {shape}\nfunc @abi_{shape}({types}, i64, ptr) -> {shape} {{\nblock0({params}, v10: i64, v11: ptr):\n"
        );
        let mut value = "v0".to_string();
        for i in 1..10 {
            let n = 12 + 3 * (i - 1);
            ir += &format!(
                " v{n} = {op} {value}, {value}\n v{} = {op} v{n}, {value}\n v{} = {op} v{}, v{i}\n",
                n + 1,
                n + 2,
                n + 1
            );
            value = format!("v{}", n + 2);
        }
        ir += &format!(" store {value}, v11\n return {value}\n}}\n");
        ir += &format!(
            "func @call_abi_{shape}(ptr, ptr) {{\nblock0(v0: ptr, v1: ptr):\n v2 = load.{shape} v0\n v3 = iconst.i64 0\n v4 = call @abi_{shape}({}, v3, v1)\n store v4, v1\n return\n}}\n",
            std::iter::repeat_n("v2", 10).collect::<Vec<_>>().join(", ")
        );
        ir += &format!(
            "func @cross_call_{shape}(ptr, ptr) {{\nblock0(v0: ptr, v1: ptr):\n v2 = load.{shape} v0\n v3 = call @external_{shape}(v2)\n v4 = {op} v2, v3\n store v4, v1\n return\n}}\n"
        );
        ir += &format!(
            "func @tail_{shape}({shape}) -> {shape} {{\nblock0(v0: {shape}):\n return_call @external_{shape}(v0)\n}}\n"
        );
        ir += &format!(
            "func @indirect_{shape}(ptr, {shape}) -> {shape} {{\nblock0(v0: ptr, v1: {shape}):\n v2 = call_indirect v0(v1) : ({shape}) -> {shape}\n return v2\n}}\n"
        );
        ir += &format!(
            "func @varcall_{shape}({shape}) -> {shape} {{\nblock0(v0: {shape}):\n v1 = iconst.i64 10\n v2 = call @variadic_{shape}(v1, {})\n return v2\n}}\n",
            std::iter::repeat_n("v0", 10).collect::<Vec<_>>().join(", ")
        );
        ir += &format!(
            "func @swap_{shape}({shape}, {shape}, i64) -> {shape} {{\nblock0(v0: {shape}, v1: {shape}, v2: i64):\n jump block1(v0, v1, v2)\nblock1(v3: {shape}, v4: {shape}, v5: i64):\n v6 = iconst.i64 0\n v7 = icmp.eq v5, v6\n brif v7, block3(v3), block2\nblock2:\n v8 = iconst.i64 1\n v9 = isub v5, v8\n jump block1(v4, v3, v9)\nblock3(v10: {shape}):\n return v10\n}}\n"
        );
        // Keep enough vectors live to force >13 XMM values through a call.
        ir += &format!("func @pressure_{shape}(ptr, ptr) {{\nblock0(v0: ptr, v1: ptr):\n");
        for i in 0..18 {
            ir += &format!(
                " v{} = iconst.i64 {}\n v{} = ptradd v0, v{}\n v{} = load.{shape} v{}\n",
                2 + 3 * i,
                bytes * i,
                3 + 3 * i,
                2 + 3 * i,
                4 + 3 * i,
                3 + 3 * i
            );
        }
        ir += &format!(" v60 = call @external_{shape}(v4)\n");
        let mut last = "v60".to_string();
        for i in 0..18 {
            ir += &format!(" v{} = {op} {last}, v{}\n", 61 + i, 4 + 3 * i);
            last = format!("v{}", 61 + i);
        }
        ir += &format!(" store {last}, v1\n return\n}}\n");
        c += &format!(
            "typedef {cty} V_{shape} __attribute__((vector_size({bytes})));\nextern V_{shape} abi_{shape}({}, uint64_t,void*);\nextern void call_abi_{shape}(const void*,void*);\nextern void cross_call_{shape}(const void*,void*);\nextern V_{shape} tail_{shape}(V_{shape});\nextern V_{shape} indirect_{shape}(V_{shape}(*)(V_{shape}),V_{shape});\nextern V_{shape} varcall_{shape}(V_{shape});\nextern V_{shape} swap_{shape}(V_{shape},V_{shape},uint64_t);\nextern V_{shape} scalar_select_{shape}(uint64_t,V_{shape},V_{shape});\nextern void pressure_{shape}(const void*,void*);\n",
            std::iter::repeat_n(format!("V_{shape}"), 10)
                .collect::<Vec<_>>()
                .join(", ")
        );
        c += &format!(
            "__attribute__((noinline)) V_{shape} external_{shape}(V_{shape} a) {{{clobber}return a+a;}}\nV_{shape} variadic_{shape}(uint64_t n,...) {{va_list ap;va_start(ap,n);V_{shape} r={{0}};for(uint64_t i=0;i<n;i++)r+=va_arg(ap,V_{shape});va_end(ap);return r;}}\n"
        );
        main += &format!(
            "{{ V_{shape} a,b,r,out,inputs[18],args[10];for(unsigned i=0;i<{};i++){{a[i]=({cty})(i+1);b[i]=({cty})(2*i+1);}}\nfor(unsigned j=0;j<10;j++)for(unsigned i=0;i<sizeof(a)/sizeof(a[0]);i++)args[j][i]=({cty})((j+1)*(i+1));r=abi_{shape}({},0,&out); V_{shape} e=args[0];for(unsigned j=1;j<10;j++)e=e+e+e+args[j];if(memcmp(&r,&e,{bytes})||memcmp(&out,&e,{bytes}))return 10;\ne=a;for(unsigned j=1;j<10;j++)e=e+e+e+a;call_abi_{shape}(&a,&out);if(memcmp(&out,&e,{bytes}))return 11;\ncross_call_{shape}(&a,&out);e=a+a+a;if(memcmp(&out,&e,{bytes}))return 12;\nr=tail_{shape}(a);e=a+a;if(memcmp(&r,&e,{bytes}))return 13;\nr=indirect_{shape}(external_{shape},a);if(memcmp(&r,&e,{bytes}))return 14;\nr=varcall_{shape}(a);e=a;for(unsigned j=1;j<10;j++)e+=a;if(memcmp(&r,&e,{bytes}))return 15;\nr=swap_{shape}(a,b,3);if(memcmp(&r,&b,{bytes}))return 16;\nr=swap_{shape}(a,b,4);if(memcmp(&r,&a,{bytes}))return 17;\nfor(unsigned k=0;k<2;k++){{r=scalar_select_{shape}(k,a,b);e=k?a:b;if(memcmp(&r,&e,{bytes}))return 18;}}\ne=a+a;for(unsigned j=0;j<18;j++){{for(unsigned l=0;l<sizeof(a)/sizeof(a[0]);l++)inputs[j][l]=({cty})(a[l]+j);e+=inputs[j];}}pressure_{shape}(inputs,&out);if(memcmp(&out,&e,{bytes}))return 19; }}\n",
            shape.lanes(),
            (0..10)
                .map(|i| format!("args[{i}]"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    main += "return 0;}\n";
    c += &main;
    (ir, c)
}
