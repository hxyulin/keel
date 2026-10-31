//! Independent source and destination widths for narrowing integer contracts.
use keel_ir::{Intrinsic, VectorType};
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub width: u16,
    pub saturation: u8,
    pub source_bits: u8,
    pub destination_bits: u8,
    pub store: bool,
    pub mode: u8,
    pub shape: &'static str,
}
impl Case {
    pub fn stem(self) -> String {
        format!(
            "pmov{}{}",
            ["", "s", "us"][usize::from(self.saturation)],
            self.shape
        )
    }
    pub fn intrinsic_name(self) -> String {
        format!(
            "x86_avx512vl_{}{}{}",
            self.stem(),
            if self.store { "store" } else { "" },
            self.width
        )
    }
    pub fn id(self) -> Intrinsic {
        Intrinsic::from_name(&self.intrinsic_name()).unwrap()
    }
    pub fn name(self) -> String {
        format!("{}_m{}", self.intrinsic_name(), self.mode)
    }
    pub fn source(self) -> VectorType {
        match (self.source_bits, self.width) {
            (32, 128) => VectorType::I32x4,
            (32, 256) => VectorType::I32x8,
            (64, 128) => VectorType::I64x2,
            (64, 256) => VectorType::I64x4,
            _ => unreachable!(),
        }
    }
    pub fn result(self) -> VectorType {
        match self.destination_bits {
            8 => VectorType::I8x16,
            16 => VectorType::I16x8,
            32 => VectorType::I32x4,
            _ => unreachable!(),
        }
    }
    pub fn api(self) -> String {
        let prefix = if self.width == 128 { "_mm" } else { "_mm256" };
        let operation = ["cvtepi", "cvtsepi", "cvtusepi"][usize::from(self.saturation)];
        let source = self.source_bits;
        let destination = self.destination_bits;
        if self.store {
            format!("{prefix}_mask_{operation}{source}_storeu_epi{destination}")
        } else {
            let mode = match self.mode {
                0 | 1 => "mask_",
                2 => "maskz_",
                3 => "",
                _ => unreachable!(),
            };
            format!("{prefix}_{mode}{operation}{source}_epi{destination}")
        }
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for width in [128, 256] {
        for saturation in 0..3 {
            for (shape, source_bits, destination_bits) in [
                ("db", 32, 8),
                ("dw", 32, 16),
                ("qb", 64, 8),
                ("qw", 64, 16),
                ("qd", 64, 32),
            ] {
                for mode in 0..4 {
                    cases.push(Case {
                        width,
                        saturation,
                        shape,
                        source_bits,
                        destination_bits,
                        store: false,
                        mode,
                    });
                }
                cases.push(Case {
                    width,
                    saturation,
                    shape,
                    source_bits,
                    destination_bits,
                    store: true,
                    mode: 0,
                });
            }
        }
    }
    cases
}
pub fn directory(prefix: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
/// Build one immutable system compiler reference object for every allocation route.
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut references = String::new();
    let mut table = String::new();
    for case in cases {
        let name = case.name();
        declarations += &format!("extern void {name}(const void*,void*,void*,uint64_t);\n");
        let width = case.width;
        let api = case.api();
        let mut source = format!(
            "__attribute__((target(\"avx512f,avx512vl\"))) static void ref_{name}(const void*pm,void*pa,void*out,uint64_t k){{__m{width}i a;memcpy(&a,{},{});",
            if case.store || case.mode == 1 {
                "pm"
            } else {
                "pa"
            },
            width / 8
        );
        if case.store {
            source += &format!("{api}(pa,k,a);");
        } else {
            source += "__m128i m,r;memcpy(&m,pm,16);";
            source += &match case.mode {
                0 | 1 => format!("r={api}(m,k,a);"),
                2 => format!("r={api}(k,a);"),
                3 => format!("r={api}(a);"),
                _ => unreachable!(),
            };
            source += "memcpy(out,&r,16);";
        }
        source += "}\n";
        references += &source;
        table += &format!(
            "{{\"{name}\",{name},ref_{name},{},{},{},{},{},{}}},\n",
            case.width,
            case.source_bits,
            case.destination_bits,
            case.saturation,
            u8::from(case.store),
            case.mode
        );
    }
    include_str!("avx512_vl_narrow.c")
        .replace("@DECLS@", &declarations)
        .replace("@REFERENCES@", &references)
        .replace("@TABLE@", &table)
}
