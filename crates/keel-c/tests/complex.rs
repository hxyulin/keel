//! Complex values and ABI interoperation, using Clang as an independent oracle.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::Target;
use std::{fs, path::Path, process::Command};

fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(
        output.status.success(),
        "{c:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "_Static_assert(!__builtin_types_compatible_p(float _Complex,double _Complex),\"component identity\");\n_Static_assert(_Generic((double _Complex)1,double _Complex:1,default:0),\"generic identity\");\n",
    );
    let mut harness =
        String::from("#define NAN __builtin_nan(\"\")\n#define INF __builtin_inf()\n");
    let mut main = String::from("int main(void) {\n");
    for (id, real) in [("f", "float"), ("d", "double")] {
        let defs = format!(
            r#"
            typedef {real} R_{id}; typedef {real} _Complex Z_{id};
            struct H_{id} {{ Z_{id} z; }};
            struct M_{id} {{ int tag; Z_{id} z; }};
            struct B_{id} {{ Z_{id} z[3]; int tag; }};
            struct N_{id} {{ struct H_{id} pair[2]; }};
            _Static_assert(sizeof(Z_{id}) == 2*sizeof({real}), "size");
            _Static_assert(_Alignof(Z_{id}) == _Alignof({real}), "alignment");
        "#
        );
        source.push_str(&defs);
        harness.push_str(&defs);
        let mut cases = vec![
            ("add", "a+b"),
            ("sub", "a-b"),
            ("mul", "a*b"),
            ("div", "a/b"),
            ("rmul", "a*(__real__ b)"),
            ("lmul", "(__real__ a)*b"),
            ("rdiv", "a/(__real__ b)"),
            ("ldiv", "(__real__ a)/b"),
            ("compound_mul", "({a*=b;a;})"),
            ("compound_div", "({a/=b;a;})"),
            ("self_mul", "({a*=a;a;})"),
            ("self_div", "({a/=a;a;})"),
            ("wide_compound_mul", "({a*=(double _Complex)b;a;})"),
            ("wide_compound_div", "({a/=(double _Complex)b;a;})"),
            ("compound_rmul", "({a*=(__real__ b);a;})"),
            ("compound_rdiv", "({a/=(__real__ b);a;})"),
            ("neg", "-a"),
            ("plus", "+a"),
            ("radd", "a+(__real__ b)"),
            ("ladd", "(__real__ a)+b"),
            ("rsub", "a-(__real__ b)"),
            ("lsub", "(__real__ a)-b"),
            ("construct", "__builtin_complex(__real__ a,__imag__ b)"),
            ("choose", "a ? a : b"),
        ];
        for (name, expression) in cases.drain(..) {
            source.push_str(&format!(
                "Z_{id} k_{id}_{name}(Z_{id} a,Z_{id} b) {{ return {expression}; }}\n"
            ));
            harness.push_str(&format!(
                "extern Z_{id} k_{id}_{name}(Z_{id},Z_{id}), c_{id}_{name}(Z_{id},Z_{id});\n"
            ));
            main.push_str(&format!(
                r#"for(int i=0;i<n;i++)for(int j=0;j<n;j++) {{
                Z_{id} a=__builtin_complex((R_{id})values[i],(R_{id})values[j]);
                Z_{id} b=__builtin_complex((R_{id})values[j],(R_{id})values[(i+3)%n]);
                Z_{id} k=k_{id}_{name}(a,b),c=c_{id}_{name}(a,b);
                if(!same_{id}(k,c))return __LINE__;
            }}
            "#
            ));
        }
        for (name, expression) in [
            ("eq", "a==b"),
            ("ne", "a!=b"),
            ("not", "!a"),
            ("bool", "(_Bool)a"),
            ("truth", "a?7:9"),
            ("and", "a&&b"),
            ("or", "a||b"),
            ("mixed_eq", "a==(__real__ b)"),
            ("mixed_ne", "(__real__ a)!=b"),
            ("not_eq", "!(a==b)"),
        ] {
            source.push_str(&format!(
                "int k_{id}_{name}(Z_{id} a,Z_{id} b) {{ return {expression}; }}\n"
            ));
            harness.push_str(&format!(
                "extern int k_{id}_{name}(Z_{id},Z_{id}),c_{id}_{name}(Z_{id},Z_{id});\n"
            ));
            main.push_str(&format!(
                r#"for(int i=0;i<n;i++)for(int j=0;j<n;j++) {{
                Z_{id} a=__builtin_complex((R_{id})values[i],(R_{id})values[j]);
                Z_{id} b=__builtin_complex((R_{id})values[j],(R_{id})values[i]);
                if(k_{id}_{name}(a,b)!=c_{id}_{name}(a,b))return __LINE__;
            }}
            "#
            ));
        }
        for (name, result, body) in [
            ("real_compound_add", "R", "r+=z; return r;"),
            ("real_compound_sub", "R", "r-=z; return r;"),
            ("real_compound_mul", "R", "r*=z; return r;"),
            ("real_compound_div", "R", "r/=z; return r;"),
            ("bool_compound_mul", "int", "_Bool b=r; b*=z; return b;"),
            ("bool_compound_div", "int", "_Bool b=r; b/=z; return b;"),
            ("bool_compound_add", "int", "_Bool b=r; b+=z; return b;"),
            ("bool_compound_sub", "int", "_Bool b=r; b-=z; return b;"),
        ] {
            let result = if result == "R" { real } else { result };
            source.push_str(&format!(
                "{result} k_{id}_{name}(R_{id} r,Z_{id} z){{{body}}}\n"
            ));
            harness.push_str(&format!(
                "extern {result} k_{id}_{name}(R_{id},Z_{id}),c_{id}_{name}(R_{id},Z_{id});\n"
            ));
            let comparison = if result == real {
                format!("!same_r_{id}(k_{id}_{name}(r,z),c_{id}_{name}(r,z))")
            } else {
                format!("k_{id}_{name}(r,z)!=c_{id}_{name}(r,z)")
            };
            main.push_str(&format!(
                r#"for(int i=0;i<n;i++)for(int j=0;j<n;j++) {{
                R_{id} r=(R_{id})values[i];
                Z_{id} z=__builtin_complex((R_{id})values[j],(R_{id})values[(i+2)%n]);
                if({comparison})return __LINE__;
            }}"#
            ));
        }
        for (case, expr) in [
            format!("__builtin_complex((R_{id})0.0,(R_{id})-0.0)+(R_{id})0.0"),
            format!("(R_{id})-0.0-__builtin_complex((R_{id})0.0,(R_{id})-0.0)"),
            format!("-__builtin_complex((R_{id})__builtin_inf(),(R_{id})__builtin_nan(\"\"))"),
            format!("__builtin_complex((R_{id})16777216,(R_{id})-0.0)+(R_{id})1-(R_{id})16777216"),
            "(float _Complex)__builtin_complex(1.234567890123,-0.0)".to_owned(),
            format!("__builtin_complex((R_{id})1.25,(R_{id})-2.5)*__builtin_complex((R_{id})3,(R_{id})4)"),
            format!("__builtin_complex((R_{id})1.25,(R_{id})-2.5)/__builtin_complex((R_{id})3,(R_{id})4)"),
            format!("__builtin_complex((R_{id})0.0,(R_{id})-0.0)*(R_{id})-2"),
            format!("(R_{id})-2*__builtin_complex((R_{id})0.0,(R_{id})-0.0)"),
            format!("__builtin_complex((R_{id})1.234567,(R_{id})9.876543)*__builtin_complex((R_{id})7.654321,(R_{id})3.456789)"),
            "1.234567890123456789i".to_owned(),
            "1.234567890123456789if".to_owned(),
            "0x1.000002p0fj".to_owned(),
            "0x1.00000100000000000001p0if".to_owned(),
            "0x0.ffffff00000000000001p-126if".to_owned(),
            "0x0.fffffffffffff8000001p-1022i".to_owned(),
            "-0.0i".to_owned(),
        ]
        .iter()
        .enumerate()
        {
            source.push_str(&format!("Z_{id} k_{id}_fold{case}={expr}; Z_{id} k_{id}_foldruntime{case}(void){{return {expr};}}\n"));
            harness.push_str(&format!(
                "extern Z_{id} k_{id}_fold{case},c_{id}_fold{case}; extern Z_{id} k_{id}_foldruntime{case}(void);\n"
            ));
            main.push_str(&format!(
                "if(!same_{id}(k_{id}_fold{case},c_{id}_fold{case}) || !same_{id}(k_{id}_fold{case},k_{id}_foldruntime{case}()))return __LINE__;\n"
            ));
        }
        main.push_str(&format!(
            r#"
            for(int a=0;a<n;a++)for(int b=0;b<n;b++)for(int c=0;c<n;c++)for(int d=0;d<n;d++) {{
                Z_{id} x=__builtin_complex((R_{id})values[a],(R_{id})values[b]);
                Z_{id} y=__builtin_complex((R_{id})values[c],(R_{id})values[d]);
                if(!same_{id}(k_{id}_mul(x,y),c_{id}_mul(x,y)))return __LINE__;
                if(!same_{id}(k_{id}_div(x,y),c_{id}_div(x,y)))return __LINE__;
            }}
        "#
        ));
        let prefix = (0..7)
            .map(|i| format!("R_{id} r{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let params = format!("{prefix},struct N_{id} a,R_{id} tail");
        let args = "r0,r1,r2,r3,r4,r5,r6,a,tail";
        let sum = "a.pair[0].z+a.pair[1].z+r0+r1+r2+r3+r4+r5+r6+tail";
        source.push_str(&format!(
            r#"
            extern Z_{id} cb_exhaust_{id}({params});
            Z_{id} k_{id}_nested_exhaust({params}) {{ return {sum}; }}
            Z_{id} k_{id}_nested_exhaust_call({params}) {{ return cb_exhaust_{id}({args}); }}
        "#
        ));
        harness.push_str(&format!(
            r#"
            Z_{id} cb_exhaust_{id}({params}) {{ return {sum}; }}
            extern Z_{id} k_{id}_nested_exhaust({params}),k_{id}_nested_exhaust_call({params});
        "#
        ));
        main.push_str(&format!(r#"{{
            struct N_{id} a={{{{ {{__builtin_complex((R_{id})2,(R_{id})3)}}, {{__builtin_complex((R_{id})5,(R_{id})7)}} }}}};
            Z_{id} want=cb_exhaust_{id}(1,2,3,4,5,6,7,a,11);
            if(!same_{id}(want,k_{id}_nested_exhaust(1,2,3,4,5,6,7,a,11)))return __LINE__;
            if(!same_{id}(want,k_{id}_nested_exhaust_call(1,2,3,4,5,6,7,a,11)))return __LINE__;
        }}"#));
        let params = (0..10)
            .map(|i| format!("Z_{id} a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sum = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("+");
        source.push_str(&format!(r#"
            extern Z_{id} callback_{id}({params});
            Z_{id} k_{id}_stack({params}) {{ return {sum}; }}
            Z_{id} k_{id}_call({params}) {{ return callback_{id}({args}); }}
            Z_{id} k_{id}_global = __builtin_complex(({real})-0.0,({real})-2.5);
            Z_{id} k_{id}_scalar = 3;
            Z_{id} k_{id}_constant = -__builtin_complex(({real})2,({real})-0.0) + ({real})1;
            Z_{id} k_{id}_local(Z_{id} a,Z_{id} b) {{ Z_{id} z={{a}}; z=b; __real__ z += 3; __imag__ z -= 2; return z; }}
            Z_{id} k_{id}_compound(Z_{id} a,Z_{id} b) {{ a+=b; a-=({real})2; return a; }}
            Z_{id} k_{id}_once(Z_{id} *p,int *i,Z_{id} b) {{ p[(*i)++] += b; return p[0]; }}
            Z_{id} k_{id}_rhs_mul(Z_{id} *p,int *i,int *j) {{ return p[(*i)++] *= __builtin_complex((R_{id})(*j)++,(R_{id})2); }}
            Z_{id} k_{id}_rhs_div(Z_{id} *p,int *i,int *j) {{ return p[(*i)++] /= __builtin_complex((R_{id})(*j)++,(R_{id})2); }}
            Z_{id} k_{id}_once_mul(Z_{id} *p,int *i,Z_{id} b) {{ return p[(*i)++] *= b; }}
            Z_{id} k_{id}_once_div(Z_{id} *p,int *i,Z_{id} b) {{ return p[(*i)++] /= b; }}
            Z_{id} k_{id}_capture(Z_{id} a,Z_{id} b) {{ return ({{Z_{id} t=a; t;}})+b; }}
            Z_{id} k_{id}_old(Z_{id} a,Z_{id} b) {{ a+=b; return a; }}
            Z_{id} k_{id}_volatile(Z_{id} a) {{ volatile Z_{id} v=a; Z_{id} b=v; __imag__ v=-0.0; return b; }}
            Z_{id} k_{id}_temp(Z_{id} a,Z_{id} b) {{ return __builtin_complex(__real__(a+b),__imag__(a-b)); }}
            Z_{id} k_{id}_self(Z_{id} a) {{ a=a; return a; }}
            Z_{id} k_{id}_fromreal({real} a) {{ return a; }}
            Z_{id} k_{id}_effects(int *i,int *j) {{ return __builtin_complex(({real})(*i)++,({real})(*j)++); }}
            Z_{id} k_{id}_address(Z_{id} a) {{ {real} *p=&__imag__ a; *p=-0.0; return a; }}
            struct __attribute__((packed)) P_{id} {{ char c; Z_{id} z; }};
            Z_{id} k_{id}_packed(Z_{id} a) {{ struct P_{id} p={{1,a}}; p.z+=2; return p.z; }}
            int k_{id}_short(Z_{id} a,int *p) {{ return a && ++*p; }}
            Z_{id} k_{id}_fromint(long a) {{ return a; }}
            long k_{id}_toint(Z_{id} a) {{ return (long)a; }}
            {real} k_{id}_toreal(Z_{id} a) {{ return a; }}
            struct H_{id} k_{id}_hfa(struct H_{id} a) {{ a.z+=1; return a; }}
            struct M_{id} k_{id}_mixed(struct M_{id} a) {{ a.z-=1; a.tag+=2; return a; }}
            struct B_{id} k_{id}_big(struct B_{id} a) {{ a.z[1]+=a.z[2]; a.tag+=3; return a; }}
            extern struct M_{id} cb_mixed_{id}(struct M_{id});
            struct M_{id} k_{id}_mixedcall(struct M_{id} a) {{ return cb_mixed_{id}(a); }}
        "#));
        harness.push_str(&format!(r#"
            int same_r_{id}({real} x,{real} y) {{ if(x!=x&&y!=y)return 1; return x==y && __builtin_signbit(x)==__builtin_signbit(y); }}
            int same_{id}(Z_{id} x,Z_{id} y) {{ return same_r_{id}(__real__ x,__real__ y)&&same_r_{id}(__imag__ x,__imag__ y); }}
            Z_{id} callback_{id}({params}) {{ return {sum}; }}
            struct M_{id} cb_mixed_{id}(struct M_{id} a) {{ a.z+=2; a.tag+=7; return a; }}
            extern Z_{id} k_{id}_stack({params}),k_{id}_call({params});
            extern Z_{id} k_{id}_global,k_{id}_scalar,k_{id}_constant;
            extern Z_{id} k_{id}_local(Z_{id},Z_{id}),k_{id}_compound(Z_{id},Z_{id}),k_{id}_once(Z_{id}*,int*,Z_{id}),k_{id}_capture(Z_{id},Z_{id}),k_{id}_old(Z_{id},Z_{id}),k_{id}_volatile(Z_{id}),k_{id}_temp(Z_{id},Z_{id});
            extern Z_{id} k_{id}_rhs_mul(Z_{id}*,int*,int*),k_{id}_rhs_div(Z_{id}*,int*,int*);
            extern Z_{id} k_{id}_once_mul(Z_{id}*,int*,Z_{id}),k_{id}_once_div(Z_{id}*,int*,Z_{id});
            extern Z_{id} k_{id}_self(Z_{id}),k_{id}_fromreal({real}),k_{id}_effects(int*,int*),k_{id}_address(Z_{id}),k_{id}_packed(Z_{id});
            extern int k_{id}_short(Z_{id},int*),c_{id}_short(Z_{id},int*);
            extern Z_{id} k_{id}_fromint(long); extern long k_{id}_toint(Z_{id}); extern {real} k_{id}_toreal(Z_{id});
            extern struct H_{id} k_{id}_hfa(struct H_{id});
            extern struct M_{id} k_{id}_mixed(struct M_{id}),k_{id}_mixedcall(struct M_{id});
            extern struct B_{id} k_{id}_big(struct B_{id});
        "#));
        main.push_str(&format!(r#"{{
            Z_{id} a=__builtin_complex((R_{id})2,(R_{id})-3), b=__builtin_complex((R_{id})4,(R_{id})5);
            Z_{id} (*volatile indirect)({params})=k_{id}_stack;
            Z_{id} want=callback_{id}(a,b,a,b,a,b,a,b,a,b);
            if(!same_{id}(want,indirect(a,b,a,b,a,b,a,b,a,b)))return __LINE__;
            if(!same_{id}(want,k_{id}_call(a,b,a,b,a,b,a,b,a,b)))return __LINE__;
            if(!same_{id}(k_{id}_global,__builtin_complex((R_{id})-0.0,(R_{id})-2.5)))return __LINE__;
            if(!same_{id}(k_{id}_scalar,(Z_{id})3)||!same_{id}(k_{id}_constant,__builtin_complex((R_{id})-1,(R_{id})0.0)))return __LINE__;
            if(!same_{id}(k_{id}_local(a,b),__builtin_complex((R_{id})7,(R_{id})3)))return __LINE__;
            if(!same_{id}(k_{id}_compound(a,b),a+b-2))return __LINE__;
            int i=0; Z_{id} x=a;
            if(!same_{id}(k_{id}_once(&x,&i,b),a+b)||i!=1||!same_{id}(x,a+b))return __LINE__;
            i=0; x=a;
            if(!same_{id}(k_{id}_once_mul(&x,&i,b),a*b)||i!=1||!same_{id}(x,a*b))return __LINE__;
            i=0; x=a;
            if(!same_{id}(k_{id}_once_div(&x,&i,b),a/b)||i!=1||!same_{id}(x,a/b))return __LINE__;
            int rhs=4; i=0; x=a; Z_{id} factor=__builtin_complex((R_{id})4,(R_{id})2);
            if(!same_{id}(k_{id}_rhs_mul(&x,&i,&rhs),a*factor)||i!=1||rhs!=5||!same_{id}(x,a*factor))return __LINE__;
            rhs=4; i=0; x=a;
            if(!same_{id}(k_{id}_rhs_div(&x,&i,&rhs),a/factor)||i!=1||rhs!=5||!same_{id}(x,a/factor))return __LINE__;
            if(!same_{id}(k_{id}_capture(a,b),a+b)||!same_{id}(k_{id}_old(a,b),a+b))return __LINE__;
            if(!same_{id}(k_{id}_volatile(a),a)||!same_{id}(k_{id}_temp(a,b),__builtin_complex((R_{id})6,(R_{id})-8)))return __LINE__;
            if(!same_{id}(k_{id}_fromint(-12345),(Z_{id})-12345)||k_{id}_toint(a)!=2||k_{id}_toreal(a)!=2)return __LINE__;
            for(int t=0;t<n;t++) {{
                R_{id} r=(R_{id})values[t]; Z_{id} z=__builtin_complex(r,r);
                if(!same_{id}(k_{id}_self(z),z))return __LINE__;
                if(!same_{id}(k_{id}_fromreal(r),(Z_{id})r)||!same_r_{id}(k_{id}_toreal(z),r))return __LINE__;
                int x=3,y=3; if(k_{id}_short(z,&x)!=c_{id}_short(z,&y)||x!=y)return __LINE__;
            }}
            int j=5,l=7; if(!same_{id}(k_{id}_effects(&j,&l),__builtin_complex((R_{id})5,(R_{id})7))||j!=6||l!=8)return __LINE__;
            if(!same_{id}(k_{id}_address(a),__builtin_complex((R_{id})2,(R_{id})-0.0))||!same_{id}(k_{id}_packed(a),a+2))return __LINE__;
            struct H_{id} h={{a}}; h=k_{id}_hfa(h); if(!same_{id}(h.z,a+1))return __LINE__;
            struct M_{id} m={{7,a}}; m=k_{id}_mixed(m); if(m.tag!=9||!same_{id}(m.z,a-1))return __LINE__;
            m=k_{id}_mixedcall(m); if(m.tag!=16||!same_{id}(m.z,a+1))return __LINE__;
            struct B_{id} big={{{{a,b,a}},7}}; big=k_{id}_big(big); if(big.tag!=10||!same_{id}(big.z[1],a+b))return __LINE__;
        }}
        "#));
    }
    for (name, expression) in [
        ("mul", "x*y"),
        ("div", "x/y"),
        ("assign_mul", "({x*=y;x;})"),
        ("assign_div", "({x/=y;x;})"),
    ] {
        source.push_str(&format!(
            "float _Complex k_rank_{name}(float _Complex x,double y){{return {expression};}}\n"
        ));
        harness.push_str(&format!("extern float _Complex k_rank_{name}(float _Complex,double),c_rank_{name}(float _Complex,double);\n"));
        main.push_str(&format!("for(int i=0;i<n;i++)for(int j=0;j<n;j++){{float _Complex x=__builtin_complex((float)values[i],(float)values[j]); double y=values[(i+3)%n]; if(!same_f(k_rank_{name}(x,y),c_rank_{name}(x,y)))return __LINE__;}}\n"));
    }
    source.push_str("float _Complex k_narrow(double _Complex x){return x;} double _Complex k_widen(float _Complex x){return x;} double _Complex k_precision(float _Complex x,double y){return x+y;}\n");
    harness.push_str("extern float _Complex k_narrow(double _Complex); extern double _Complex k_widen(float _Complex),k_precision(float _Complex,double);\n");
    main.push_str("{ double _Complex d=__builtin_complex(1.234567890123,-0.0); float _Complex f=(float _Complex)d; if(!same_f(k_narrow(d),f)||!same_d(k_widen(f),(double _Complex)f)||!same_d(k_precision(f,1.0),f+1.0))return __LINE__; } for(int i=0;i<n;i++)for(int j=0;j<n;j++){double _Complex d=__builtin_complex(values[i],values[j]); float _Complex f=(float _Complex)d; if(!same_f(k_narrow(d),f)||!same_d(k_widen(f),(double _Complex)f))return __LINE__;} return 0; }\n");
    harness.push_str("static double values[]={0.0,-0.0,1.25,-2.5,1.0e-300,1.0e300,1.4e-45,3.4e38,0x1p-149,0x1p-126,0x1p-1074,0x1p-1022,0x1.fffffffffffffp1023,0x1.fffffep127,-0x1p-149,-0x1p-1074,INF,-INF,NAN}; static int n=sizeof(values)/sizeof(values[0]);\n");
    harness.push_str(&main);
    (
        source,
        harness.replace("return __LINE__;", "return (__LINE__%254)+1;"),
    )
}
fn native(target: Target) -> bool {
    match target {
        Target::Aarch64AppleDarwin => cfg!(all(target_arch = "aarch64", target_os = "macos")),
        Target::Aarch64LinuxGnu => cfg!(all(target_arch = "aarch64", target_os = "linux")),
        Target::X86_64LinuxGnu => cfg!(all(target_arch = "x86_64", target_os = "linux")),
    }
}
fn clang_object(dir: &Path, source: &str, object: &str, triple: &str) {
    command(Command::new("clang").current_dir(dir).args([
        "-target",
        triple,
        "-std=gnu11",
        "-ffp-contract=off",
        "-O1",
        "-c",
        source,
        "-o",
        object,
    ]));
}
#[test]
fn clang_differential_and_bidirectional_abi() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-abi");
    fs::create_dir_all(&dir).unwrap();
    let (source, harness) = sources();
    fs::write(dir.join("oracle.c"), source.replace(" k_", " c_")).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for target in Target::NATIVE {
        let triple = match target {
            Target::Aarch64AppleDarwin => "arm64-apple-macos11",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        };
        clang_object(&dir, "oracle.c", "oracle.o", triple);
        clang_object(&dir, "harness.c", "harness.o", triple);
        for (selector, allocator, optimized, debug_info) in [
            (SelectionTier::Baseline, AllocationTier::Full, false, false),
            (SelectionTier::Trees, AllocationTier::Fast, false, true),
            (SelectionTier::Full, AllocationTier::Full, true, false),
        ] {
            let mut module =
                keel_c::compile(&source, "complex.c", keel_c::Options { target, debug_info })
                    .unwrap();
            assert!(
                module.decls.iter().all(|d| d.name.starts_with("callback_")
                    || d.name.starts_with("cb_")
                    || ["__mulsc3", "__muldc3", "__divsc3", "__divdc3"].contains(&d.name.as_str())),
                "unexpected helper declarations: {:?}",
                module.decls
            );
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let program =
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
            let object = match target {
                Target::Aarch64AppleDarwin => {
                    keel_asm::assemble_program_object(&program, Default::default())
                }
                Target::Aarch64LinuxGnu => {
                    keel_asm::assemble_elf_program_object(&program, Default::default())
                }
                Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object_with_features(
                    &program,
                    Default::default(),
                ),
            }
            .unwrap();
            // Both actual object formats must expose the explicit runtime ABI;
            // neither code generation nor optimization may silently drop it.
            use object::{Object, ObjectSymbol};
            let parsed = object::File::parse(object.as_slice()).unwrap();
            for helper in ["__mulsc3", "__muldc3", "__divsc3", "__divdc3"] {
                let name = if target == Target::Aarch64AppleDarwin {
                    format!("_{helper}")
                } else {
                    helper.to_owned()
                };
                assert!(
                    parsed
                        .symbols()
                        .any(|s| s.name().ok() == Some(name.as_str())
                            && s.is_undefined()
                            && !s.is_weak()),
                    "missing strong runtime import {name}"
                );
            }
            fs::write(dir.join("keel.o"), object).unwrap();
            if native(target) {
                command(Command::new("clang").current_dir(&dir).args([
                    "keel.o",
                    "oracle.o",
                    "harness.o",
                    "-o",
                    "run",
                ]));
                command(&mut Command::new(dir.join("run")));
                if target == Target::Aarch64AppleDarwin {
                    // Darwin arm64 division is in the compiler-rt builtins
                    // archive, not the arm64 libSystem TBD export interface.
                    let archive = Command::new("clang")
                        .arg("-print-libgcc-file-name")
                        .output()
                        .unwrap();
                    assert!(archive.status.success());
                    let archive = String::from_utf8(archive.stdout).unwrap();
                    let sdk = Command::new("xcrun")
                        .arg("--show-sdk-path")
                        .output()
                        .unwrap();
                    assert!(sdk.status.success());
                    let sdk = String::from_utf8(sdk.stdout).unwrap();
                    let root = Path::new(sdk.trim());
                    let dylib =
                        keel_link::Dylib::from_tbd(&root.join("usr/lib/libSystem.tbd"), root)
                            .unwrap();
                    for symbol in ["___muldc3", "___mulsc3"] {
                        assert!(dylib.exports.contains(symbol));
                    }
                    // ld is given the actual runtime archive explicitly; this
                    // route does not rely on Clang inserting it implicitly.
                    command(Command::new("xcrun").current_dir(&dir).args([
                        "ld",
                        "-arch",
                        "arm64",
                        "-platform_version",
                        "macos",
                        "11.0",
                        "16.0",
                        "-syslibroot",
                        sdk.trim(),
                        "-lSystem",
                        archive.trim(),
                        "keel.o",
                        "oracle.o",
                        "harness.o",
                        "-o",
                        "run-ld",
                    ]));
                    command(&mut Command::new(dir.join("run-ld")));
                }
            } else {
                eprintln!(
                    "{target:?}: IR verified and objects generated; runtime not tested on this host"
                );
                let keel = fs::read(dir.join("keel.o")).unwrap();
                for name in ["oracle.o", "harness.o"] {
                    assert_eq!(keel[..4], fs::read(dir.join(name)).unwrap()[..4]);
                }
            }
        }
    }
}
#[test]
fn imaginary_literal_lexical_diagnostics_match_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-literal-diagnostics");
    fs::create_dir_all(&dir).unwrap();
    for literal in ["0x1.0i", "0x1.fi", "0X.8JF", "0x1.0", "1.0ii", "1.0ifj"] {
        let source = format!("double _Complex z={literal};");
        fs::write(dir.join("literal.c"), &source).unwrap();
        assert!(
            !Command::new("clang")
                .current_dir(&dir)
                .args(["-std=gnu11", "-fsyntax-only", "literal.c"])
                .output()
                .unwrap()
                .status
                .success(),
            "Clang accepted {literal}"
        );
        for target in Target::NATIVE {
            assert!(
                keel_c::compile(
                    &source,
                    "literal.c",
                    keel_c::Options {
                        target,
                        debug_info: false,
                    }
                )
                .is_err(),
                "accepted {literal} for {target:?}"
            );
        }
    }
}

#[test]
fn unsupported_complex_operations_diagnose() {
    for target in Target::NATIVE {
        for source in [
            "int f(double _Complex a){return a<2;}",
            "int f(double _Complex a){return a%2;}",
            "double _Complex f(double _Complex a){return ++a;}",
            "double _Complex f(double _Complex a){return ~a;}",
            "long double _Complex x;",
            "long double _Complex f(long double _Complex a){return a;}",
            "_Complex int x;",
            "double _Imaginary z;",
            "double _Complex z=1i;",
            "double _Complex z=1.0li;",
            "double _Complex z=1.0il;",
            "double _Complex z=1.0ii;",
            "double _Complex z=1.0ijf;",
            "struct S {long double _Complex z;}; struct S f(struct S a){return a;}",
            "void f(double _Complex a){(__int128)a;}",
            "void f(__int128 a){(double _Complex)a;}",
            "__int128 x = (double _Complex)1;",
            "__int128 x = (double _Complex)0x1p100;",
            "double _Complex x = ((__int128)1 << 100);",
            "double _Complex x = (__int128)1;",
            "int f(double _Complex a){return __builtin_isnan(a);}",
            "void *f(double _Complex a){return (void*)a;}",
            "double _Complex f(void *a){return (double _Complex)a;}",
            "void v(int,...); void f(double _Complex a){v(0,a);}",
            "double _Complex f(int x,...){__builtin_va_list ap; __builtin_va_start(ap,x); return __builtin_va_arg(ap,double _Complex);}",
            "union U {double _Complex z; long x;}; union U f(union U x){return x;}",
            "struct __attribute__((packed)) P {char x; double _Complex z;}; struct P f(struct P x){return x;}",
        ] {
            assert!(
                keel_c::compile(
                    source,
                    "unsupported.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {source} for {target:?}"
            );
        }
    }
}

/// These register probes run even on the other architecture. Unlike object
/// emission alone they distinguish SysV's packed complex-float SSE eightbyte
/// from AAPCS64's two scalar HFA registers.
#[test]
fn complex_abi_register_probes_match_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-registers");
    fs::create_dir_all(&dir).unwrap();
    for target in Target::NATIVE {
        let triple = match target {
            Target::Aarch64AppleDarwin => "arm64-apple-macos11",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        };
        for (real, component, which) in [
            ("float", "__real__ b", "second"),
            ("double", "__real__ b", "second"),
            ("float", "__imag__ a", "imag"),
            ("double", "__imag__ a", "imag"),
        ] {
            let source = format!(
                "{real} probe({real} _Complex a, {real} _Complex b) {{ return {component}; }}"
            );
            fs::write(dir.join("probe.c"), &source).unwrap();
            command(
                Command::new("clang")
                    .current_dir(&dir)
                    .args(["-target", triple, "-O1", "-S", "probe.c", "-o", "clang.s"]),
            );
            let module = keel_c::compile(
                &source,
                "probe.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            assert!(module.decls.is_empty(), "unexpected runtime helper");
            let keel = keel_codegen::compile(&module, target).unwrap();
            let clang = fs::read_to_string(dir.join("clang.s")).unwrap();
            let reg = match (target, real, which) {
                (Target::X86_64LinuxGnu, "float", "second") => "xmm1",
                (Target::X86_64LinuxGnu, "double", "second") => "xmm2",
                (Target::X86_64LinuxGnu, "float", "imag") => "xmm0",
                (Target::X86_64LinuxGnu, "double", "imag") => "xmm1",
                (Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu, "float", "second") => "s2",
                (Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu, "double", "second") => "d2",
                (Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu, "float", "imag") => "s1",
                (Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu, "double", "imag") => "d1",
                _ => unreachable!(),
            };
            assert!(keel.contains(reg), "Keel missing {reg}: {keel}");
            let alias = if target != Target::X86_64LinuxGnu {
                format!("v{}", &reg[1..])
            } else {
                reg.to_owned()
            };
            assert!(
                clang.contains(reg) || clang.contains(&alias),
                "Clang missing {reg}: {clang}"
            );
            if target == Target::X86_64LinuxGnu && real == "float" {
                assert!(
                    !keel.contains("xmm2"),
                    "complex float consumed two SSE registers: {keel}"
                );
                assert!(!clang.contains("xmm2"), "Clang: {clang}");
            }
        }
        if target == Target::X86_64LinuxGnu {
            for real in ["float", "double"] {
                let source = format!(
                    "{real} _Complex result(void) {{ return __builtin_complex(({real})1,({real})2); }}"
                );
                fs::write(dir.join("result.c"), &source).unwrap();
                command(
                    Command::new("clang")
                        .current_dir(&dir)
                        .args(["-target", triple, "-O1", "-S", "result.c", "-o", "clang.s"]),
                );
                let module = keel_c::compile(
                    &source,
                    "result.c",
                    keel_c::Options {
                        target,
                        debug_info: false,
                    },
                )
                .unwrap();
                let keel = keel_codegen::compile(&module, target).unwrap();
                let clang = fs::read_to_string(dir.join("clang.s")).unwrap();
                let keel_return = keel.rsplit_once("# return").unwrap().1;
                for asm in [keel_return, clang.as_str()] {
                    assert!(asm.contains("xmm0"), "missing SSE return: {asm}");
                    assert_eq!(
                        asm.contains("xmm1"),
                        real == "double",
                        "wrong complex return classification: {asm}"
                    );
                    assert!(
                        !asm.contains("rdi"),
                        "unexpected hidden result pointer: {asm}"
                    );
                }
            }
        }
    }
}

#[test]
fn component_lvalue_alignment_matches_clang() {
    let source = r#"
        typedef double _Complex Z;
        struct __attribute__((packed)) P { char c; Z z; };
        struct P p;
        _Static_assert(__alignof__(__real__ p.z) == 8, "packed real");
        _Static_assert(__alignof__(__imag__ p.z) == 8, "packed imag");
        _Alignas(32) Z z;
        _Static_assert(__alignof__(__real__ z) == 8, "aligned real");
        _Static_assert(__alignof__(__imag__ z) == 8, "aligned imag");
    "#;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-alignment");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("alignment.c"), source).unwrap();
    for target in Target::NATIVE {
        let triple = match target {
            Target::Aarch64AppleDarwin => "arm64-apple-macos11",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        };
        clang_object(&dir, "alignment.c", "alignment.o", triple);
        keel_c::compile(
            source,
            "alignment.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
    }
}

#[test]
fn component_rvalues_and_const_objects_are_not_modifiable() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-lvalues");
    fs::create_dir_all(&dir).unwrap();
    for source in [
        "void f(double _Complex z){__real__ (+z)=1;}",
        "double *f(double _Complex z){return &__imag__ (+z);}",
        "void f(const double _Complex z){__real__ z=1;}",
        "void f(const double _Complex *z){++__imag__ *z;}",
        "typedef const float _Complex Z; void f(Z z){__imag__ z+=1;}",
        "double _Complex g(void); double *f(void){return &__real__ g();}",
    ] {
        fs::write(dir.join("lvalue.c"), source).unwrap();
        // Clang drops const on GNU component extraction. Keel retains the
        // object's qualification; compare only temporary addressability here.
        if !source.contains("const") {
            assert!(
                !Command::new("clang")
                    .current_dir(&dir)
                    .args(["-std=gnu11", "-fsyntax-only", "lvalue.c"])
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "Clang accepted {source}"
            );
        }
        for target in Target::NATIVE {
            assert!(
                keel_c::compile(
                    source,
                    "lvalue.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {source} for {target:?}"
            );
        }
    }
}

#[test]
fn static_arithmetic_range_and_final_binding_policy() {
    for target in Target::NATIVE {
        // Range-sensitive and exceptional runtime operations are intentionally
        // not folded using host arithmetic. In static storage they diagnose.
        for expression in [
            "__builtin_complex(0x1p1023,0.0)*2.0",
            "__builtin_complex(0x1p-1022,0.0)*0x1p-100",
            "__builtin_complex((double)__builtin_inf(),0.0)*2.0",
            "__builtin_complex(1.0,2.0)/__builtin_complex(0.0,0.0)",
            "__builtin_complex(1.0,2.0)/__builtin_complex(0x1p-1074,0.0)",
            "__builtin_complex(1.0f,2.0f)*__builtin_complex(0x1p127f,0.0f)",
        ] {
            assert!(
                keel_c::compile(
                    &format!("double _Complex z={expression};"),
                    "range.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err()
            );
            // Identical expression is valid in an executable context.
            keel_c::compile(
                &format!("double _Complex f(void){{return {expression};}}"),
                "range.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
        }
        let alias = if target == Target::Aarch64AppleDarwin {
            "___muldc3"
        } else {
            "__muldc3"
        };
        assert!(keel_c::compile(&format!("double _Complex other(double,double,double,double) __asm__(\"{alias}\"); double _Complex f(double _Complex a){{return a*a;}}"), "reserved.c", keel_c::Options {target, debug_info:false}).is_err());
        let source = "extern int x; double _Complex z=&x ? __builtin_complex(1.0,2.0)*3.0 : 0;";
        keel_c::compile(
            source,
            "binding.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        assert!(
            keel_c::compile(
                &format!("{source} extern int x __attribute__((weak));"),
                "binding.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .is_err()
        );
        for declaration in [
            "int __muldc3;",
            "double __muldc3(double);",
            "double _Complex __muldc3(double,double,double,double);",
        ] {
            assert!(
                keel_c::compile(
                    &format!("{declaration} double _Complex f(double _Complex a){{return a*a;}}"),
                    "reserved.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {declaration} for {target:?}"
            );
        }
    }
}

#[test]
fn raw_platform_complex_header() {
    let target = Target::default();
    if !native(target) {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("complex-header");
    fs::create_dir_all(&dir).unwrap();
    let source = r#"
        #include <complex.h>
        _Static_assert(_Generic(I, float _Complex:1,default:0), "I type");
        _Static_assert(_Generic(1.0fi, float _Complex:1,default:0), "fi type");
        _Static_assert(_Generic(1.0j, double _Complex:1,default:0), "j type");
        _Static_assert(_Generic(1.0FJ, float _Complex:1,default:0), "FJ type");
        float _Complex f=1.234567890123456789if;
        double _Complex d=1.234567890123456789i;
        double _Complex negative=-0.0i;
        int main(void) {
            volatile double r=2.0;
            double _Complex z=r+3.0*I, w=4.0-5.0*_Complex_I;
            double _Complex x=z*w;
            if(creal(x)!=23.0 || cimag(x)!=2.0) return 1;
            z*=w; z/=w;
            if(creal(z)!=r || cimag(z)!=3.0) return 2;
            if(__real__ f!=0.0f || __builtin_signbit(__real__ f)) return 3;
            if(__imag__ f!=1.234567890123456789f) return 4;
            if(__imag__ d!=1.234567890123456789) return 5;
            if(!__builtin_signbit(__real__ negative) || !__builtin_signbit(__imag__ negative)) return 6;
            return 0;
        }
    "#;
    fs::write(dir.join("raw.c"), source).unwrap();
    let sysroot = if cfg!(target_os = "macos") {
        let out = Command::new("xcrun")
            .arg("--show-sdk-path")
            .output()
            .unwrap();
        assert!(out.status.success());
        Some(String::from_utf8(out.stdout).unwrap().trim().into())
    } else {
        None
    };
    let own = keel_c::preprocess(
        source,
        dir.join("raw.c").to_str().unwrap(),
        &keel_c::PreprocessOptions {
            target,
            sysroot,
            ..Default::default()
        },
    )
    .unwrap();
    command(
        Command::new("clang")
            .current_dir(&dir)
            .args(["-E", "raw.c", "-o", "clang.i"]),
    );
    command(Command::new("clang").current_dir(&dir).args([
        "-ffp-contract=off",
        "raw.c",
        "-lm",
        "-o",
        "oracle",
    ]));
    command(&mut Command::new(dir.join("oracle")));
    for text in [own, fs::read_to_string(dir.join("clang.i")).unwrap()] {
        for optimized in [false, true] {
            let mut module = keel_c::compile(
                &text,
                "raw.c",
                keel_c::Options {
                    target,
                    debug_info: !optimized,
                },
            )
            .unwrap();
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let program = keel_codegen::compile_program_with_pipeline(
                &module,
                target,
                &CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                },
            )
            .unwrap();
            let object = match target {
                Target::Aarch64AppleDarwin => {
                    keel_asm::assemble_program_object(&program, Default::default())
                }
                Target::Aarch64LinuxGnu => {
                    keel_asm::assemble_elf_program_object(&program, Default::default())
                }
                Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object_with_features(
                    &program,
                    Default::default(),
                ),
            }
            .unwrap();
            fs::write(dir.join("raw.o"), object).unwrap();
            command(
                Command::new("clang")
                    .current_dir(&dir)
                    .args(["raw.o", "-lm", "-o", "run"]),
            );
            command(&mut Command::new(dir.join("run")));
        }
    }
}
