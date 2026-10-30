//! Explicit encoded widths, short payloads and independent conversion references.
use keel_ir::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub encoded: u16,
    pub byte: VectorType,
    pub word: VectorType,
    pub mask: Type,
    pub operation: u8,
    pub kind: u8,
    pub alias: bool,
}
impl Case {
    pub fn name(self) -> String {
        format!("{}_{}", self.id.info().name, u8::from(self.alias))
    }
    pub fn input(self) -> VectorType {
        if self.operation < 2 {
            self.byte
        } else {
            self.word
        }
    }
    pub fn output(self) -> VectorType {
        if self.operation < 2 {
            self.word
        } else {
            self.byte
        }
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (encoded, byte, word, mask) in [
        (128, VectorType::I8x16, VectorType::I16x8, Type::I8),
        (256, VectorType::I8x16, VectorType::I16x16, Type::I16),
        (512, VectorType::I8x32, VectorType::I16x32, Type::I32),
    ] {
        for (operation, name, kind) in [
            (0, "pmovsxbw", 0),
            (0, "pmovsxbwload", 1),
            (1, "pmovzxbw", 0),
            (1, "pmovzxbwload", 1),
            (2, "pmovwb", 2),
            (2, "pmovwbstore", 3),
            (3, "pmovswb", 2),
            (3, "pmovswbstore", 3),
            (4, "pmovuswb", 2),
            (4, "pmovuswbstore", 3),
        ] {
            let name = format!("x86_avx512bw_{name}{encoded}");
            let id = Intrinsic::from_name(&name).unwrap_or_else(|| panic!("missing {name}"));
            cases.push(Case {
                id,
                encoded,
                byte,
                word,
                mask,
                operation,
                kind,
                alias: false,
            });
            if kind == 0 || kind == 2 {
                cases.push(Case {
                    id,
                    encoded,
                    byte,
                    word,
                    mask,
                    operation,
                    kind,
                    alias: true,
                });
            }
        }
    }
    cases
}
pub fn directory(prefix: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
pub fn api(case: Case) -> String {
    let prefix = match case.encoded {
        128 => "_mm",
        256 => "_mm256",
        512 => "_mm512",
        _ => unreachable!(),
    };
    let op = match case.operation {
        0 => "cvtepi8_epi16",
        1 => "cvtepu8_epi16",
        2 => "cvtepi16_epi8",
        3 => "cvtsepi16_epi8",
        4 => "cvtusepi16_epi8",
        _ => unreachable!(),
    };
    if case.kind == 1 {
        format!("__keel{prefix}_mask_load_{op}")
    } else if case.kind == 3 {
        format!("{prefix}_mask_{}_storeu_epi8", op.trim_end_matches("_epi8"))
    } else {
        format!("{prefix}_mask_{op}")
    }
}
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut references = String::new();
    let mut table = String::new();
    for case in cases {
        let name = case.name();
        declarations += &format!("extern void {name}(const void*,void*,void*,uint64_t);\n");
        references += &reference(*case);
        table += &format!(
            "{{\"{name}\",{name},ref_{name},{},{},{},{},{},{}}},\n",
            case.encoded / 16,
            case.input().bytes(),
            case.output().bytes(),
            case.operation,
            case.kind,
            u8::from(case.alias)
        );
    }
    include_str!("../vector_x86/x86_avx512_bw_convert.c")
        .replace("@DECLS@", &declarations)
        .replace("@REFERENCES@", &references)
        .replace("@TABLE@", &table)
}
fn reference(case: Case) -> String {
    let vector = |bytes: u32| format!("__m{}i", bytes * 8);
    let load = |p: &str, bytes: u32| {
        if bytes == 64 {
            format!("_mm512_loadu_si512({p})")
        } else {
            let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
            format!(
                "{prefix}_loadu_si{}((const {}*){p})",
                bytes * 8,
                vector(bytes)
            )
        }
    };
    let store = |p: &str, v: &str, bytes: u32| {
        if bytes == 64 {
            format!("_mm512_storeu_si512({p},{v});")
        } else {
            let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
            format!(
                "{prefix}_storeu_si{}(({}*){p},{v});",
                bytes * 8,
                vector(bytes)
            )
        }
    };
    let name = case.name();
    let output = case.output().bytes();
    let input = case.input().bytes();
    let api = api(case);
    let mut source = format!(
        "__attribute__((target(\"avx512f,avx512bw,avx512vl\"))) static void ref_{name}(const void*pm,void*pa,void*out,uint64_t k){{"
    );
    if case.kind == 3 {
        source += &format!("{} a={};{api}(pa,k,a);", vector(input), load("pm", input));
    } else {
        source += &format!("{} r={};", vector(output), load("pm", output));
        if case.kind == 1 {
            let kmov = if case.encoded == 512 {
                "kmovd"
            } else {
                "kmovw"
            };
            let op = if case.operation == 0 {
                "vpmovsxbw"
            } else {
                "vpmovzxbw"
            };
            source += &format!(
                "__asm__ volatile(\"{kmov} %k[k],%%k1\\n\\t{op} (%[p]),%0%{{%%k1%}}\":\"+v\"(r):[p]\"r\"(pa),[k]\"r\"((unsigned)k):\"k1\",\"memory\");"
            );
        } else {
            let pointer = if case.alias { "pm" } else { "pa" };
            source += &format!(
                "{} a={};r={api}(r,k,a);",
                vector(input),
                load(pointer, input)
            );
        }
        source += &store("out", "r", output);
    }
    source += "}\n";
    source
}
