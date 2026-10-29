//! Generated x86 capability policy, independent of selection and allocation tiers.
//! Default is the v1 baseline. Feature sets describe a target, never host detection.
include!("isa/feature_fields.rs");
include!("isa/cpu_profiles.rs");

impl Features {
    pub const fn none() -> Self {
        Self::NONE
    }

    /// Parse modifiers from the empty optional-capability baseline.
    pub fn parse(source: &str) -> Result<Self, String> {
        Self::NONE.with_features(source)
    }
    pub fn with_features(self, source: &str) -> Result<Self, String> {
        self.with_feature_lists([source])
    }
    /// Apply ordered edits and validate once after every list has been applied.
    /// Positive edits enable prerequisite closure; negative edits clear one bit.
    pub fn with_feature_lists<'a>(
        mut self,
        sources: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, String> {
        for source in sources {
            if source.is_empty() {
                continue;
            }
            if source == "none" {
                self = Self::NONE;
                continue;
            }
            for token in source.split(',') {
                let (enabled, name) = if let Some(name) = token.strip_prefix('-') {
                    (false, name)
                } else {
                    (true, token.strip_prefix('+').unwrap_or(token))
                };
                if !self.set_feature(name, enabled) {
                    return Err(format!("unknown x86 target feature `{token}`"));
                }
            }
        }
        self.validate()?;
        Ok(self)
    }
}
impl super::Opcode {
    pub fn required_features(self) -> Features {
        if let super::Opcode::Evex(op) = self {
            return op.kind.required_features();
        }
        super::isa::opcode_features(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_profiles_are_cumulative_and_default_to_v1() {
        assert_eq!(Features::default(), Features::NONE);
        assert_eq!(Features::for_level(CpuLevel::V1), Features::NONE);
        assert!(Features::NONE.is_empty());
        let v2 = Features::for_level(CpuLevel::V2);
        let v3 = Features::for_level(CpuLevel::V3);
        assert!(v3.contains(v2) && v2.contains(Features::NONE));
        assert_eq!(
            v2.names().collect::<Vec<_>>(),
            [
                "cx16",
                "lahf_sahf",
                "popcnt",
                "sse3",
                "ssse3",
                "sse41",
                "sse42"
            ]
        );
        assert_eq!(v3.names().count(), 17);
        assert!(v3.validate().is_ok());
        assert_eq!(Features::all_known().names().count(), 38);
        assert!(Features::all_known().validate().is_ok());
        for invalid in ["legacy", "v4", "V1", ""] {
            assert!(CpuLevel::parse(invalid).is_err());
        }
    }
    #[test]
    fn positive_edits_enable_transitive_prerequisites() {
        for feature in ["avx2", "fma", "f16c", "vaes", "vpclmulqdq"] {
            let f = Features::parse(feature).unwrap();
            assert!(f.avx && f.xsave && f.osxsave);
            assert!(f.validate().is_ok());
        }
        assert_eq!(
            Features::parse("pclmul").unwrap(),
            Features::parse("pclmulqdq").unwrap()
        );
        assert_eq!(
            Features::parse("bmi2").unwrap().names().collect::<Vec<_>>(),
            ["bmi2"]
        );
    }
    #[test]
    fn modifiers_are_ordered_and_validation_cannot_be_bypassed() {
        assert!(Features::parse("avx2,-avx").is_err());
        assert!(Features::parse("avx,-xsave").is_err());
        assert!(Features::parse("avx2,-avx,+avx").is_ok());
        assert!(
            Features::NONE
                .with_feature_lists(["avx2,-avx", "+avx"])
                .is_ok()
        );
        assert!(
            Features {
                avx: true,
                ..Features::NONE
            }
            .with_features("")
            .is_err()
        );
        assert!(
            Features {
                avx2: true,
                ..Features::NONE
            }
            .validate()
            .is_err()
        );
        assert!(
            Features::all_known()
                .with_features("none")
                .unwrap()
                .is_empty()
        );
        assert!(Features::parse("").unwrap().is_empty());
        assert!(Features::parse("avx2,none").is_err());
        for invalid in ["+", "-", "explicit", "AES", " aes", "unknown"] {
            assert!(Features::parse(invalid).is_err());
        }
    }
    #[test]
    fn fingerprints_cover_every_feature_and_ignore_field_order() {
        // Independent assignment pin. Prerequisite closure is deliberately
        // excluded by clearing just the named bit from an all-known set.
        let names = [
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
            "avx512f",
            "avx512vl",
            "rdtscp",
            "fsgsbase",
            "rdrand",
            "invpcid",
            "prefetchw",
            "rdseed",
            "waitpkg",
            "amx_tile",
            "amx_int8",
            "amx_bf16",
            "amx_fp16",
            "apx_f",
        ];
        let all = Features::all_known();
        assert_eq!(all.fingerprint(), (1 << 38) - 1);
        for (bit, name) in names.iter().enumerate() {
            let mut missing = all;
            assert!(missing.set_feature(name, false));
            assert_eq!(
                all.fingerprint() ^ missing.fingerprint(),
                1 << bit,
                "{name}"
            );
            assert!(!missing.contains(all));
        }
    }
}
