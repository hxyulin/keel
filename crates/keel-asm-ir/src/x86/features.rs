//! Explicit ISA legality for x86. The default keeps the existing baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub avx: bool,
    pub bmi1: bool,
    pub bmi2: bool,
}
impl Features {
    pub const fn all() -> Self {
        Self {
            avx: true,
            bmi1: true,
            bmi2: true,
        }
    }
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
    pub fn contains(self, required: Self) -> bool {
        (!required.avx || self.avx)
            && (!required.bmi1 || self.bmi1)
            && (!required.bmi2 || self.bmi2)
    }
    /// Comma-separated names, optionally prefixed with + or -. `none` resets all.
    pub fn parse(source: &str) -> Result<Self, String> {
        if source == "none" {
            return Ok(Self::default());
        }
        let mut out = Self::default();
        for token in source.split(',') {
            let (enabled, name) = if let Some(s) = token.strip_prefix('-') {
                (false, s)
            } else {
                (true, token.strip_prefix('+').unwrap_or(token))
            };
            let flag = match name {
                "avx" => &mut out.avx,
                "bmi1" => &mut out.bmi1,
                "bmi2" => &mut out.bmi2,
                _ => {
                    return Err(format!(
                        "unknown x86 target feature `{token}`; expected avx,bmi1,bmi2 or none"
                    ));
                }
            };
            *flag = enabled;
        }
        Ok(out)
    }
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        [(self.avx, "avx"), (self.bmi1, "bmi1"), (self.bmi2, "bmi2")]
            .into_iter()
            .filter_map(|(enabled, name)| enabled.then_some(name))
    }
}
impl super::Opcode {
    pub fn required_features(self) -> Features {
        use super::Opcode::*;
        match self {
            Vaddss | Vaddsd | Vsubss | Vsubsd | Vmulss | Vmulsd | Vdivss | Vdivsd | Vsqrtss
            | Vsqrtsd | Vminss | Vminsd | Vmaxss | Vmaxsd | Vucomiss | Vucomisd | Vmovss
            | Vmovsd | Vmovaps | Vmovups | Vmovapd | Vmovupd | Vandps | Vandpd | Vorps | Vorpd
            | Vxorps | Vxorpd | Vroundss | Vroundsd | Vzeroupper => Features {
                avx: true,
                ..Features::default()
            },
            _ => Features::default(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_feature_lists_and_independent_legality() {
        assert_eq!(
            Features::parse("+avx,bmi1,-avx,bmi2").unwrap(),
            Features {
                avx: false,
                bmi1: true,
                bmi2: true
            }
        );
        assert!(Features::all().contains(Features {
            bmi2: true,
            ..Features::default()
        }));
        assert!(!Features::default().contains(Features {
            avx: true,
            ..Features::default()
        }));
        for s in ["", "none,avx", "avx,", "avx2", "+", "bmi"] {
            assert!(Features::parse(s).is_err(), "{s}");
        }
    }
}
