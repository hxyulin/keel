//! Shared immutable BW cases and independent scalar/guard-page oracle.
use keel_ir::{Intrinsic, intrinsic::X86Avx512BwKind};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub alias: bool,
}
impl Case {
    pub fn name(self) -> String {
        format!(
            "{}{}",
            self.id.info().name,
            if self.alias { "_alias" } else { "" }
        )
    }
    pub fn operation(self) -> &'static str {
        let name = self.id.info().name.strip_prefix("x86_avx512bw_").unwrap();
        let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
        &stem[..stem.len() - 1]
    }
}
pub fn cases() -> Vec<Case> {
    Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_bw_core().is_some())
        .flat_map(|id| {
            let mut cases = vec![Case { id, alias: false }];
            if id.x86_avx512_bw_core().unwrap().kind == X86Avx512BwKind::Binary {
                cases.push(Case { id, alias: true });
            }
            cases
        })
        .collect()
}
pub fn runner(cases: &[Case]) -> String {
    let mut declarations = String::new();
    let mut table = String::new();
    for case in cases {
        let d = case.id.x86_avx512_bw_core().unwrap();
        let kind = match d.kind {
            X86Avx512BwKind::Binary => 0,
            X86Avx512BwKind::Move => 1,
            X86Avx512BwKind::Load => 2,
            X86Avx512BwKind::Store => 3,
        };
        let op = [
            "add", "sub", "adds", "subs", "addus", "subus", "mins", "minu", "maxs", "maxu",
        ]
        .iter()
        .position(|&op| op == case.operation())
        .unwrap_or(10);
        let name = case.name();
        declarations +=
            &format!("extern void {name}(const void*,const void*,const void*,void*,uint64_t);\n");
        table += &format!(
            "{{\"{name}\",{name},{},{},{op},{kind},{}}},\n",
            d.vector.bytes(),
            d.vector.lane_type().bytes().unwrap() * 8,
            u8::from(case.alias)
        );
    }
    include_str!("../vector_x86/x86_avx512_bw.c")
        .replace("@DECLS@", &declarations)
        .replace("@TABLE@", &table)
}
