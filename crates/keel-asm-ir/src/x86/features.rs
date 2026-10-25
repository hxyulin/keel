//! CPU capability contracts, independent of instruction-selection and allocation tiers.
//! Default is the historical POPCNT/SSE4.1 baseline; strict v1 is explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuLevel {
    V1,
    V2,
    V3,
}
impl CpuLevel {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "v1" => Ok(Self::V1),
            "v2" => Ok(Self::V2),
            "v3" => Ok(Self::V3),
            _ => Err(format!(
                "unknown x86 CPU level `{name}`; expected v1, v2, v3 or legacy"
            )),
        }
    }
}
include!("isa/feature_fields.rs");
impl Default for Features {
    fn default() -> Self {
        Self::legacy()
    }
}
impl Features {
    pub const fn none() -> Self {
        Self::NONE
    }
    pub const fn legacy() -> Self {
        Self {
            popcnt: true,
            sse41: true,
            ..Self::NONE
        }
    }
    pub const fn for_level(level: CpuLevel) -> Self {
        match level {
            CpuLevel::V1 => Self {
                explicit: true,
                ..Self::NONE
            },
            CpuLevel::V2 => Self {
                cx16: true,
                lahf_sahf: true,
                popcnt: true,
                sse3: true,
                ssse3: true,
                sse41: true,
                sse42: true,
                explicit: true,
                ..Self::NONE
            },
            CpuLevel::V3 => Self {
                cx16: true,
                lahf_sahf: true,
                popcnt: true,
                sse3: true,
                ssse3: true,
                sse41: true,
                sse42: true,
                avx: true,
                avx2: true,
                bmi1: true,
                bmi2: true,
                f16c: true,
                fma: true,
                lzcnt: true,
                movbe: true,
                xsave: true,
                osxsave: true,
                explicit: true,
                ..Self::NONE
            },
        }
    }
    pub const fn all() -> Self {
        Self {
            aes: true,
            adx: true,
            sha: true,
            vaes: true,
            vpclmulqdq: true,
            clflush: true,
            pclmulqdq: true,
            ..Self::for_level(CpuLevel::V3)
        }
    }
    /// Compatibility query: true only for the implicit legacy configuration.
    pub fn is_empty(self) -> bool {
        self == Self::legacy()
    }
    /// Parse modifiers on the historical compatibility baseline.
    pub fn parse(source: &str) -> Result<Self, String> {
        Self::legacy().with_features(source)
    }
    /// Apply independent bits. Enabling AVX enables XSAVE/OSXSAVE; other
    /// dependencies are checked after all modifiers. Legacy struct literals may
    /// omit the state-management bits for source migration compatibility.
    pub fn with_features(mut self, source: &str) -> Result<Self, String> {
        if source == "none" {
            return Ok(self);
        }
        for token in source.split(',') {
            let (enabled, name) = if let Some(s) = token.strip_prefix('-') {
                (false, s)
            } else {
                (true, token.strip_prefix('+').unwrap_or(token))
            };
            let flag = self
                .feature_mut(name)
                .ok_or_else(|| format!("unknown x86 target feature `{token}`"))?;
            *flag = enabled;
            if enabled && name == "avx" {
                self.xsave = true;
                self.osxsave = true;
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn validate(self) -> Result<(), &'static str> {
        for (enabled, dependency, name) in [
            (
                self.avx2 || self.f16c || self.fma || self.vaes || self.vpclmulqdq,
                self.avx,
                "AVX2/F16C/FMA/VAES/VPCLMULQDQ require AVX",
            ),
            (
                self.avx && self.explicit,
                self.xsave && self.osxsave,
                "AVX requires XSAVE and OSXSAVE",
            ),
        ] {
            if enabled && !dependency {
                return Err(name);
            }
        }
        Ok(())
    }
}
impl super::Opcode {
    pub fn required_features(self) -> Features {
        super::isa::opcode_features(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_empty_configurations_retain_the_public_contract() {
        assert_eq!(Features::NONE, Features::none());
        assert_eq!(Features::default(), Features::legacy());
        assert_ne!(Features::NONE, Features::default());
        assert!(!Features::NONE.is_empty());
        assert_eq!(
            Features::legacy().names().collect::<Vec<_>>(),
            ["popcnt", "sse41"]
        );
        assert_eq!(
            Features::all().names().collect::<Vec<_>>(),
            [
                "aes",
                "adx",
                "sha",
                "vaes",
                "vpclmulqdq",
                "avx",
                "bmi1",
                "bmi2",
                "clflush",
                "pclmulqdq",
                "cx16",
                "lahf_sahf",
                "popcnt",
                "sse3",
                "ssse3",
                "sse41",
                "sse42",
                "avx2",
                "f16c",
                "fma",
                "lzcnt",
                "movbe",
                "xsave",
                "osxsave",
            ]
        );
        let explicit = Features::for_level(CpuLevel::V1);
        assert_ne!(explicit, Features::NONE);
        assert_eq!(explicit.names().count(), 0);
        assert!(explicit.contains(Features::NONE));
        assert!(Features::NONE.contains(explicit));
    }

    #[test]
    fn capability_queries_and_cli_names_cover_every_public_bit() {
        let avx = Features {
            avx: true,
            xsave: true,
            osxsave: true,
            ..Features::NONE
        };
        macro_rules! check {
            ($($field:ident),* $(,)?) => {$(
                let bit = Features { $field: true, ..Features::NONE };
                assert!(Features::all().contains(bit), stringify!($field));
                assert!(!Features::NONE.contains(bit), stringify!($field));
                let mut missing = Features::all();
                missing.$field = false;
                assert!(!missing.contains(bit), stringify!($field));
                assert!(!missing.contains(Features::all()), stringify!($field));
                let mut expected = avx;
                expected.$field = true;
                assert_eq!(Features::NONE.with_features(concat!("avx,", stringify!($field))).unwrap(), expected);
                expected.$field = false;
                if expected.validate().is_ok() {
                    assert_eq!(avx.with_features(concat!("-", stringify!($field))).unwrap(), expected);
                }
            )*};
        }
        check!(
            aes, adx, sha, vaes, vpclmulqdq, avx, bmi1, bmi2, clflush, pclmulqdq, cx16, lahf_sahf,
            popcnt, sse3, ssse3, sse41, sse42, avx2, f16c, fma, lzcnt, movbe, xsave, osxsave
        );
        assert_eq!(
            Features::parse("pclmul").unwrap(),
            Features::parse("pclmulqdq").unwrap()
        );
        assert_eq!(
            Features::parse("+pclmul,-pclmulqdq").unwrap(),
            Features::legacy()
        );
    }

    #[test]
    fn modifiers_keep_token_order_side_effects_and_exact_errors() {
        let v1 = Features::for_level(CpuLevel::V1);
        assert!(v1.with_features("avx2,avx").is_ok());
        assert!(v1.with_features("-xsave,avx").unwrap().xsave);
        assert_eq!(
            v1.with_features("avx,-xsave").unwrap_err(),
            "AVX requires XSAVE and OSXSAVE"
        );
        assert_eq!(
            v1.with_features("avx2,-avx").unwrap_err(),
            "AVX2/F16C/FMA/VAES/VPCLMULQDQ require AVX"
        );
        let disabled = v1.with_features("avx,-avx").unwrap();
        assert!(!disabled.avx && disabled.xsave && disabled.osxsave && disabled.explicit);
        assert!(Features::NONE.with_features("avx,-xsave,-osxsave").is_ok());
        // The whole-source `none` shortcut also bypasses validation.
        let invalid = Features { avx2: true, ..v1 };
        assert_eq!(invalid.with_features("none").unwrap(), invalid);
        for token in ["", "+", "-", "explicit", "AES", " aes", "-unknown", "none"] {
            let source = format!("aes,{token}");
            assert_eq!(
                v1.with_features(&source).unwrap_err(),
                format!("unknown x86 target feature `{token}`")
            );
        }
        assert_eq!(
            v1.with_features("avx2,+unknown").unwrap_err(),
            "unknown x86 target feature `+unknown`"
        );
    }

    #[test]
    fn cumulative_levels_and_legacy_contract() {
        let v1 = Features::for_level(CpuLevel::V1);
        let v2 = Features::for_level(CpuLevel::V2);
        let v3 = Features::for_level(CpuLevel::V3);
        assert!(v2.contains(v1) && v3.contains(v2));
        assert!(!v1.popcnt && !v1.sse41 && !v1.avx);
        assert!(v2.popcnt && v2.sse41 && !v2.avx);
        assert!(v3.avx2 && v3.bmi1 && v3.bmi2 && v3.fma);
        assert!(v1.validate().is_ok() && v2.validate().is_ok() && v3.validate().is_ok());
        assert!(Features::default().is_empty());
        assert!(!v1.is_empty());
        assert_eq!(Features::parse("none").unwrap(), Features::legacy());
        assert_eq!(v1.with_features("none").unwrap(), v1);
        assert!(v1.with_features("avx2").is_err());
        assert!(v3.with_features("-avx").is_err());
        assert!(v3.with_features("-xsave").is_err());
        let bits = v1.with_features("popcnt,bmi2").unwrap();
        assert!(bits.popcnt && bits.bmi2 && !bits.sse41 && !bits.bmi1);
        assert!(!v1.contains(super::super::Opcode::Popcnt.required_features()));
        assert!(!v1.contains(super::super::Opcode::Roundsd.required_features()));
    }
}
