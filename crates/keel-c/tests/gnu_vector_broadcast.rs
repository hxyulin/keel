//! Frontend conversion boundaries, independent of backend instruction selection.
use keel_ir::Target;
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn source(base: &str, scalar: &str, expression: &str, bytes: usize) -> String {
    format!(
        "typedef {base} V __attribute__((vector_size({bytes}))); V f(V v,{scalar} s) {{ return {expression}; }}"
    )
}
fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "broadcast.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}
fn clang_accepts(source: &str) {
    let mut child = Command::new("clang")
        .args(["-std=gnu11", "-x", "c", "-fsyntax-only", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lossless_broadcast_conversion_boundaries() {
    for (base, scalar, expression) in [
        ("signed char", "int", "v+127"),
        ("signed char", "long", "-128L+v"),
        ("unsigned char", "int", "v+255"),
        ("short", "signed char", "v+s"),
        ("signed char", "_Bool", "v+s"),
        ("int", "unsigned short", "s-v"),
        ("long", "unsigned", "v+s"),
        ("unsigned", "unsigned", "v+s"),
        ("unsigned long", "unsigned", "s|v"),
        ("unsigned long", "int", "v+18446744073709551615UL"),
        ("long", "int", "v+9223372036854775807L"),
        ("float", "float", "v/s"),
        ("double", "float", "s-v"),
        ("float", "double", "v+1.5"),
        ("float", "double", "v+-0.0"),
        ("int", "long", "v+(int)s"),
        ("signed char", "int", "v+((1<<6)+63)"),
        ("unsigned char", "int", "(0?300:255)+v"),
        ("unsigned char", "int", "v+((~0UL)>>56)"),
        ("signed char", "int", "v+((255U+1U)==256U)"),
        ("signed char", "int", "v+(signed char)257"),
        ("unsigned char", "int", "v+sizeof(s++)"),
        ("float", "int", "v+(double)0.1f"),
        ("float", "int", "v+(float)s"),
    ] {
        let text = source(base, scalar, expression, 16);
        clang_accepts(&text);
        for target in Target::NATIVE {
            keel_ir::verify_module(&compile(&text, target).unwrap()).unwrap();
        }
    }
    for (base, scalar, expression) in [
        ("signed char", "int", "v+128"),
        ("signed char", "int", "v+-129"),
        ("unsigned char", "int", "v+256"),
        ("unsigned", "int", "v+-1"),
        ("long", "unsigned long", "v+18446744073709551615UL"),
        ("int", "long", "v+s"),
        ("int", "unsigned", "s+v"),
        ("unsigned", "int", "v+s"),
        ("float", "double", "v+s"),
        ("float", "double", "v+1.1"),
        ("float", "int", "v+s"),
        ("double", "long", "v+s"),
        ("int", "__int128", "v+s"),
        ("signed char", "int", "v+(long)(((__int128)1<<64)/256)"),
        ("signed char", "int", "v+(s++,1)"),
        ("signed char", "int", "v+((1<<6)+64)"),
        ("unsigned char", "int", "v+(1?300:255)"),
        ("signed char", "int", "v+(int)(unsigned char)255"),
        ("unsigned char", "int", "v+sizeof(int[256])"),
        ("float", "int", "v+(0.1+0.2)"),
        ("float", "int", "v+((double)0.1f-0.1)"),
        ("signed char", "int", "v+(long)(void*)1"),
        // Conservative boundaries, even when other compilers accept them.
        ("float", "int", "v+1"),
        ("int", "float", "v+1.0f"),
        ("int", "int", "v<<s"),
        ("int", "int", "s>>v"),
        ("int", "int", "v/s"),
    ] {
        let text = source(base, scalar, expression, 16);
        for target in Target::NATIVE {
            assert!(compile(&text, target).is_err(), "accepted {text}");
        }
    }
}

#[test]
fn compounds_and_distinct_vectors_stay_checked() {
    for target in Target::NATIVE {
        for expr in ["v+=s", "v-=s", "v*=s", "v&=s", "v|=s", "v^=s"] {
            let text = source("int", "int", expr, 16);
            clang_accepts(&text);
            keel_ir::verify_module(&compile(&text, target).unwrap()).unwrap();
        }
        for text in [
            source("int", "long", "v+=s", 16),
            source("int", "int", "s+=v", 16),
            source("int", "int", "v<<=s", 16),
            source("int", "int", "v=s", 16),
            "typedef int V __attribute__((vector_size(16))); typedef unsigned U __attribute__((vector_size(16))); V f(V v,U u){return v+u;}".into(),
        ] {
            assert!(compile(&text, target).is_err(), "accepted {text}");
        }
    }
}

#[test]
fn broadcasts_emit_real_x86_objects_with_explicit_feature_profiles() {
    for base in [
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned",
        "long",
        "unsigned long",
        "float",
        "double",
    ] {
        for bytes in [16, 32] {
            let mut text = source(base, base, "v+s", bytes);
            let floating = matches!(base, "float" | "double");
            let mask = match base {
                "unsigned char" => "signed char",
                "unsigned short" => "short",
                "unsigned" | "float" => "int",
                "unsigned long" | "double" => "long",
                _ => base,
            };
            text.push_str(&format!(
                "typedef {mask} M __attribute__((vector_size({bytes})));"
            ));
            let ops = if floating {
                vec!["+", "-", "*", "/"]
            } else {
                vec!["+", "-", "*", "&", "|", "^"]
            };
            for (i, op) in ops.into_iter().enumerate() {
                text.push_str(&format!(
                    "V left{i}(V v,{base} s){{return s{op}v;}} \
                     V compound{i}(V *p,int *i,{base} *s){{return p[(*i)++]{op}=(*s)++;}}"
                ));
            }
            for (i, op) in ["==", "!=", "<", "<=", ">", ">="].into_iter().enumerate() {
                text.push_str(&format!("M compare{i}(V v,{base} s){{return s{op}v;}}"));
            }
            // Exhaust SIMD argument registers and forward a broadcast result across
            // an actual call boundary, retaining the selected vector ABI.
            text.push_str(&format!(
                "extern V callback(V,V,V,V,V,V,V,V,V,V); \
                 V forward(V a,V b,V c,V d,V e,V f,V g,V h,V i,V j,{base} s){{\
                 return callback(a+s,b,c,d,e,f,g,h,i,j+s);}}"
            ));
            let module = compile(&text, Target::X86_64LinuxGnu).unwrap();
            keel_ir::verify_module(&module).unwrap();
            use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
            let features = Features::for_level(if bytes == 16 {
                CpuLevel::V1
            } else {
                CpuLevel::V3
            });
            let selector = FeatureSelector {
                inner: &keel_codegen::SelectionTier::Baseline,
                features,
            };
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &selector,
                allocator: &keel_codegen::regalloc::AllocationTier::Full,
            };
            let program = keel_codegen::compile_program_with_pipeline(
                &module,
                Target::X86_64LinuxGnu,
                &pipeline,
            )
            .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(&object[..4], b"\x7fELF");
            let selector = FeatureSelector {
                inner: &keel_codegen::SelectionTier::Baseline,
                features: Features::for_level(CpuLevel::V1),
            };
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &selector,
                allocator: &keel_codegen::regalloc::AllocationTier::Full,
            };
            if bytes == 32 {
                assert!(
                    keel_codegen::compile_program_with_pipeline(
                        &module,
                        Target::X86_64LinuxGnu,
                        &pipeline
                    )
                    .is_err()
                );
            }
        }
    }
}
