//! Explicit public contracts and independent scalar mask oracles.
use keel_ir::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub vector: VectorType,
    pub mask: Type,
    pub operation: u8,
    pub predicate: u8,
    pub alias: bool,
}
impl Case {
    pub fn name(self) -> String {
        format!(
            "{}_{}_{}",
            self.id.info().name,
            self.predicate,
            u8::from(self.alias)
        )
    }
}
pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (width, lane, vector, mask) in [
        (128, "b", VectorType::I8x16, Type::I16),
        (128, "w", VectorType::I16x8, Type::I8),
        (256, "b", VectorType::I8x32, Type::I32),
        (256, "w", VectorType::I16x16, Type::I16),
        (512, "b", VectorType::I8x64, Type::I64),
        (512, "w", VectorType::I16x32, Type::I32),
    ] {
        for (operation, name) in [
            (0, format!("cmp{lane}")),
            (1, format!("cmpu{lane}")),
            (2, format!("test{lane}")),
            (3, format!("testn{lane}")),
            (4, format!("mov{lane}2m")),
            (5, format!("movm2{lane}")),
        ] {
            let name = format!("x86_avx512bw_{name}{width}");
            let id = Intrinsic::from_name(&name).unwrap_or_else(|| panic!("missing {name}"));
            for predicate in 0..if operation < 2 { 8 } else { 1 } {
                result.push(Case {
                    id,
                    vector,
                    mask,
                    operation,
                    predicate,
                    alias: false,
                });
                if operation < 4 {
                    result.push(Case {
                        id,
                        vector,
                        mask,
                        operation,
                        predicate,
                        alias: true,
                    });
                }
            }
        }
    }
    result
}
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut table = String::new();
    let mut references = String::new();
    for case in cases {
        let name = case.name();
        let bytes = case.vector.bytes();
        let bits = case.vector.lane_type().bytes().unwrap() * 8;
        declarations += &format!("extern void {name}(const void*,const void*,void*,uint64_t);\n");
        table += &format!(
            "{{\"{name}\",{name},ref_{name},{bytes},{bits},{},{},{}}},\n",
            case.operation,
            case.predicate,
            u8::from(case.alias)
        );
        references += &reference(*case);
    }
    include_str!("../vector_x86/x86_avx512_bw_mask.c")
        .replace("@DECLS@", &declarations)
        .replace("@TABLE@", &table)
        .replace("@REFERENCES@", &references)
}

pub fn api(case: Case) -> String {
    let prefix = match case.vector.bytes() {
        16 => "_mm",
        32 => "_mm256",
        64 => "_mm512",
        _ => unreachable!(),
    };
    let bits = case.vector.lane_type().bytes().unwrap() * 8;
    match case.operation {
        0 => format!("{prefix}_mask_cmp_epi{bits}_mask"),
        1 => format!("{prefix}_mask_cmp_epu{bits}_mask"),
        2 => format!("{prefix}_mask_test_epi{bits}_mask"),
        3 => format!("{prefix}_mask_testn_epi{bits}_mask"),
        4 => format!("{prefix}_movepi{bits}_mask"),
        5 => format!("{prefix}_movm_epi{bits}"),
        _ => unreachable!(),
    }
}
fn reference(case: Case) -> String {
    let width = case.vector.bytes() * 8;
    let vector = format!("__m{width}i");
    let name = case.name();
    let api = api(case);
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
    let b = if case.alias { "a" } else { "b" };
    let args = match case.operation {
        0 | 1 => format!("k,a,{b},{}", case.predicate),
        2 | 3 => format!("k,a,{b}"),
        4 => "a".into(),
        5 => "k".into(),
        _ => unreachable!(),
    };
    let store = if case.operation == 5 {
        if width == 512 {
            "_mm512_storeu_si512(out,result);".into()
        } else {
            format!("{prefix}_storeu_si{width}(({vector}*)out,result);")
        }
    } else {
        "memcpy(out,&result,8);".into()
    };
    let result = if case.operation == 5 {
        vector.clone()
    } else {
        "uint64_t".into()
    };
    format!(
        "__attribute__((target(\"avx512f,avx512bw,avx512vl\"))) static void ref_{name}(const void*pa,const void*pb,void*out,uint64_t k){{{vector} a={},b={};{result} result={api}({args});{store}}}\n",
        load("pa"),
        load("pb")
    )
}
