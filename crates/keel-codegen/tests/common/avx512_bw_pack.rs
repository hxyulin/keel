//! Explicit BW pack/unpack contracts and independent references.
use keel_ir::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub input: VectorType,
    pub result: VectorType,
    pub mask: Type,
    pub operation: u8,
    pub alias: u8,
}
impl Case {
    pub fn name(self) -> String {
        format!("{}_{}", self.id.info().name, self.alias)
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (width, byte, word, dword) in [
        (128, VectorType::I8x16, VectorType::I16x8, VectorType::I32x4),
        (
            256,
            VectorType::I8x32,
            VectorType::I16x16,
            VectorType::I32x8,
        ),
        (
            512,
            VectorType::I8x64,
            VectorType::I16x32,
            VectorType::I32x16,
        ),
    ] {
        for (operation, name, input, result) in [
            (0, "packsswb", word, byte),
            (1, "packuswb", word, byte),
            (2, "packssdw", dword, word),
            (3, "packusdw", dword, word),
            (4, "unpcklbw", byte, byte),
            (5, "unpckhbw", byte, byte),
            (6, "unpcklwd", word, word),
            (7, "unpckhwd", word, word),
        ] {
            let name = format!("x86_avx512bw_{name}{width}");
            let id = Intrinsic::from_name(&name).unwrap_or_else(|| panic!("missing {name}"));
            let mask = match result.lanes() {
                4 | 8 => Type::I8,
                16 => Type::I16,
                32 => Type::I32,
                64 => Type::I64,
                _ => unreachable!(),
            };
            for alias in [0, 1, 2] {
                cases.push(Case {
                    id,
                    input,
                    result,
                    mask,
                    operation,
                    alias,
                });
            }
        }
    }
    cases
}
pub fn api(case: Case) -> String {
    let prefix = match case.result.bytes() {
        16 => "_mm",
        32 => "_mm256",
        64 => "_mm512",
        _ => unreachable!(),
    };
    let name = match case.operation {
        0 => "packs_epi16",
        1 => "packus_epi16",
        2 => "packs_epi32",
        3 => "packus_epi32",
        4 => "unpacklo_epi8",
        5 => "unpackhi_epi8",
        6 => "unpacklo_epi16",
        7 => "unpackhi_epi16",
        _ => unreachable!(),
    };
    format!("{prefix}_mask_{name}")
}
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut references = String::new();
    let mut table = String::new();
    for case in cases {
        let name = case.name();
        declarations +=
            &format!("extern void {name}(const void*,const void*,const void*,void*,uint64_t);\n");
        references += &reference(*case);
        table += &format!(
            "{{\"{name}\",{name},ref_{name},{},{},{},{},{}}},\n",
            case.result.bytes(),
            case.input.lane_type().bytes().unwrap() * 8,
            case.result.lane_type().bytes().unwrap() * 8,
            case.operation,
            case.alias
        );
    }
    include_str!("../vector_x86/x86_avx512_bw_pack.c")
        .replace("@DECLS@", &declarations)
        .replace("@REFERENCES@", &references)
        .replace("@TABLE@", &table)
}
fn reference(case: Case) -> String {
    let width = case.result.bytes() * 8;
    let vector = format!("__m{width}i");
    let prefix = match width {
        128 => "_mm",
        256 => "_mm256",
        512 => "_mm512",
        _ => unreachable!(),
    };
    let name = case.name();
    let api = api(case);
    let load = |p: &str| {
        if width == 512 {
            format!("_mm512_loadu_si512({p})")
        } else {
            format!("{prefix}_loadu_si{width}((const {vector}*){p})")
        }
    };
    let merge = if case.alias == 2 { "a" } else { "m" };
    let b = if case.alias == 1 { "a" } else { "b" };
    let mut source = format!(
        "__attribute__((target(\"avx512f,avx512bw,avx512vl\"))) static void ref_{name}(const void*pm,const void*pa,const void*pb,void*out,uint64_t k){{{vector} m={},a={},b={};{vector} result={api}({merge},k,a,{b});",
        load("pm"),
        load("pa"),
        load("pb")
    );
    if width == 512 {
        source += "_mm512_storeu_si512(out,result);";
    } else {
        source += &format!("{prefix}_storeu_si{width}(({vector}*)out,result);");
    }
    source += "}\n";
    source
}
