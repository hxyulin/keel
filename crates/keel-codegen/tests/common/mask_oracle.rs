// Shared ARM/x86 native oracle; compare scalar lane truth, never bit-blend truth.
pub fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n");
    let mut main = String::from(
        "int main(void) { uint64_t edges[]={0,1,2,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffffULL,0x8000000000000000ULL,~0ULL};\n",
    );
    for (src, dst, mode) in [
        (8, 8, "direct"),
        (16, 16, "direct"),
        (32, 32, "direct"),
        (64, 64, "direct"),
        (8, 16, "signed"),
        (8, 32, "signed"),
        (8, 64, "signed"),
        (16, 32, "signed"),
        (16, 64, "signed"),
        (32, 64, "signed"),
        (8, 64, "chain"),
        (32, 64, "unsigned"),
        (8, 64, "unsigned_chain"),
        (8, 8, "loaded"),
        (16, 16, "loaded"),
        (32, 32, "loaded"),
        (64, 64, "loaded"),
        (8, 16, "bitcast"),
        (8, 32, "bitcast"),
        (8, 64, "bitcast"),
        (32, 64, "phi"),
    ] {
        let from = format!("i{src}x{}", 128 / src);
        let to = format!("i{dst}x{}", 128 / dst);
        let name = format!("mask_{src}_{dst}_{mode}");
        let mut mask = String::from(" v5 = vicmp.sgt v3,v4\n");
        let mask_value = match mode {
            "direct" => 5,
            "signed" | "phi" => {
                mask += &format!(" v6 = swiden_low.{to} v5\n");
                6
            }
            "unsigned" => {
                mask += &format!(" v6 = uwiden_low.{to} v5\n");
                6
            }
            "chain" => {
                mask += " v13 = swiden_low.i16x8 v5\n v14 = swiden_low.i32x4 v13\n v6 = swiden_low.i64x2 v14\n";
                6
            }
            "unsigned_chain" => {
                mask += " v13 = uwiden_low.i16x8 v5\n v14 = swiden_low.i32x4 v13\n v6 = swiden_low.i64x2 v14\n";
                6
            }
            "bitcast" => {
                mask += &format!(" v6 = bitcast.{to} v5\n");
                6
            }
            "loaded" => {
                mask += &format!(" v6 = load.{to} align 1 v0\n");
                6
            }
            _ => unreachable!(),
        };
        if mode == "phi" {
            mask += " jump block1(v6)\nblock1(v15: i64x2):\n";
        }
        let m = if mode == "phi" { 15 } else { mask_value };
        ir += &format!(
            "func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{from} align 1 v0\n v4 = load.{from} align 1 v1\n{mask} v7 = bitcast.{to} v3\n v8 = bitcast.{to} v4\n v9 = vselect v{m},v7,v8\n v10 = vselect v{m},v8,v7\n v11 = vselect v{m},v{m},v7\n v12 = vselect v{m},v8,v{m}\n"
        );
        for (i, v) in [9, 10, 11, 12, m, 7, 8].into_iter().enumerate() {
            ir += &format!(
                " v{} = iconst.i64 {}\n v{} = ptradd v2,v{}\n store align 1 v{v},v{}\n",
                20 + 2 * i,
                16 * i,
                21 + 2 * i,
                20 + 2 * i,
                21 + 2 * i
            );
        }
        ir += " return\n}\n";
        c += &format!("extern void {name}(const void*,const void*,void*);\n");
        main += &format!(
            "{{ for(unsigned k=0;k<15;k++) for(unsigned j=0;j<15;j++) {{ unsigned char a[16],b[16],expected[112],out[128],aa[128],bb[128]; uint{dst}_t ma[{}];\n",
            128 / dst
        );
        main += "for(unsigned z=0;z<16;z++){a[z]=(unsigned char)(edges[(k+z/8)%15]>>(8*(z%8)));b[z]=(unsigned char)(edges[(j+z/8)%15]>>(8*(z%8)));}\n";
        main += &format!(
            "for(unsigned l=0;l<{};l++){{ int{src}_t x,y;memcpy(&x,a+l*{},sizeof(x));memcpy(&y,b+l*{},sizeof(y));",
            128 / dst,
            src / 8,
            src / 8
        );
        match mode {
            "loaded" => main += "memcpy(&ma[l],a+l*sizeof(ma[l]),sizeof(ma[l]));",
            "bitcast" => {
                main += &format!(
                    "ma[l]=0;for(unsigned q=0;q<{};q++){{ int8_t bx,by;memcpy(&bx,a+l*{}+q,1);memcpy(&by,b+l*{}+q,1);ma[l]|=(uint{dst}_t)(bx>by?255:0)<<(8*q);}}",
                    dst / 8,
                    dst / 8,
                    dst / 8
                )
            }
            "unsigned" => main += "ma[l]=x>y?UINT32_MAX:0;",
            "unsigned_chain" => main += "ma[l]=x>y?255:0;",
            _ => main += &format!("ma[l]=x>y?(uint{dst}_t)~(uint{dst}_t)0:0;"),
        }
        main += &format!(
            "uint{dst}_t av,bv,e[4];memcpy(&av,a+l*sizeof(av),sizeof(av));memcpy(&bv,b+l*sizeof(bv),sizeof(bv));e[0]=ma[l]?av:bv;e[1]=ma[l]?bv:av;e[2]=ma[l]?ma[l]:av;e[3]=ma[l]?bv:ma[l];for(unsigned t=0;t<4;t++)memcpy(expected+16*t+l*sizeof(av),&e[t],sizeof(av));}}memcpy(expected+64,ma,16);memcpy(expected+80,a,16);memcpy(expected+96,b,16);\n"
        );
        main += &format!(
            "for(unsigned alias=0;alias<3;alias++){{memcpy(aa+1,a,16);memcpy(bb+1,b,16);unsigned char *r=alias==1?aa+1:alias==2?bb+1:out+1;{name}(aa+1,bb+1,r);if(memcmp(r,expected,112)){{fprintf(stderr,\"{name} k=%u j=%u alias=%u\\n\",k,j,alias);return 1;}}}} }} }}\n"
        );
    }
    main += "puts(\"canonical mask oracle passed\");return 0;}\n";
    (ir, c + &main)
}
