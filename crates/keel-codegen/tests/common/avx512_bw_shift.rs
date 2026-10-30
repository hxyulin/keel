//! Explicit BW word-shift contracts and independent scalar/native references.
use keel_ir::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub vector: VectorType,
    pub mask: Type,
    pub direction: u8,
    pub kind: u8,
    pub immediate: u8,
    pub alias: bool,
}
impl Case {
    pub fn name(self) -> String {
        format!(
            "{}_{}_{}",
            self.id.info().name,
            self.immediate,
            u8::from(self.alias)
        )
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (width, vector, mask) in [
        (128, VectorType::I16x8, Type::I8),
        (256, VectorType::I16x16, Type::I16),
        (512, VectorType::I16x32, Type::I32),
    ] {
        for (direction, prefix) in [(0, "sll"), (1, "srl"), (2, "sra")] {
            for (kind, suffix) in [(0, "iw"), (1, "w"), (2, "vw")] {
                let name = format!("x86_avx512bw_{prefix}{suffix}{width}");
                let id = Intrinsic::from_name(&name).unwrap_or_else(|| panic!("missing {name}"));
                for &immediate in if kind == 0 {
                    &[0, 1, 8, 15, 16, 17, 255][..]
                } else {
                    &[0][..]
                } {
                    for alias in [false, true] {
                        cases.push(Case {
                            id,
                            vector,
                            mask,
                            direction,
                            kind,
                            immediate,
                            alias,
                        });
                    }
                }
            }
        }
    }
    cases
}
pub fn api(case: Case) -> String {
    let prefix = match case.vector.bytes() {
        16 => "_mm",
        32 => "_mm256",
        64 => "_mm512",
        _ => unreachable!(),
    };
    let direction = match case.direction {
        0 => "sll",
        1 => "srl",
        2 => "sra",
        _ => unreachable!(),
    };
    let suffix = match case.kind {
        0 => "i",
        1 => "",
        2 => "v",
        _ => unreachable!(),
    };
    format!("{prefix}_mask_{direction}{suffix}_epi16")
}
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut references = String::new();
    let mut table = String::new();
    for case in cases {
        let name = case.name();
        let bytes = case.vector.bytes();
        declarations +=
            &format!("extern void {name}(const void*,const void*,const void*,void*,uint64_t);\n");
        references += &reference(*case);
        table += &format!(
            "{{\"{name}\",{name},ref_{name},{bytes},{},{},{},{}}},\n",
            case.direction,
            case.kind,
            case.immediate,
            u8::from(case.alias)
        );
    }
    include_str!("../vector_x86/x86_avx512_bw_shift.c")
        .replace("@DECLS@", &declarations)
        .replace("@REFERENCES@", &references)
        .replace("@TABLE@", &table)
}
fn reference(case: Case) -> String {
    let width = case.vector.bytes() * 8;
    let vector = format!("__m{width}i");
    let prefix = match width {
        128 => "_mm",
        256 => "_mm256",
        512 => "_mm512",
        _ => unreachable!(),
    };
    let load = |p: &str| {
        if width == 512 {
            format!("_mm512_loadu_si512({p})")
        } else {
            format!("{prefix}_loadu_si{width}((const {vector}*){p})")
        }
    };
    let merge = if case.alias { "a" } else { "m" };
    let name = case.name();
    let api = api(case);
    let mut source = format!(
        "__attribute__((target(\"avx512f,avx512bw,avx512vl\"))) static void ref_{name}(const void*pm,const void*pa,const void*pc,void*out,uint64_t k){{{vector} m={},a={};",
        load("pm"),
        load("pa")
    );
    let count = match case.kind {
        0 => case.immediate.to_string(),
        1 => {
            source += "__m128i count=_mm_loadu_si128((const __m128i*)pc);";
            "count".into()
        }
        2 => {
            source += &format!("{vector} count={};", load("pc"));
            if case.alias {
                "a".into()
            } else {
                "count".into()
            }
        }
        _ => unreachable!(),
    };
    source += &format!("{vector} result={api}({merge},k,a,{count});");
    if width == 512 {
        source += "_mm512_storeu_si512(out,result);";
    } else {
        source += &format!("{prefix}_storeu_si{width}(({vector}*)out,result);");
    }
    source += "}\n";
    source
}
