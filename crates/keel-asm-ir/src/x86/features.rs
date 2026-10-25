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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// CPUID.01H:ECX.AES[25], independent of psABI levels.
    pub aes: bool,
    pub adx: bool,
    pub sha: bool,
    pub vaes: bool,
    pub vpclmulqdq: bool,
    pub avx: bool,
    pub bmi1: bool,
    pub bmi2: bool,
    /// CPUID.01H:EDX.CLFSH[19]; independent of SSE2 and all psABI levels.
    pub clflush: bool,
    /// Independent CPUID.01H:ECX.PCLMULQDQ[1]; absent from psABI v3.
    pub pclmulqdq: bool,
    pub cx16: bool,
    pub lahf_sahf: bool,
    pub popcnt: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse41: bool,
    pub sse42: bool,
    pub avx2: bool,
    pub f16c: bool,
    pub fma: bool,
    pub lzcnt: bool,
    pub movbe: bool,
    pub xsave: bool,
    /// OS-enabled XSAVE state management (CPUID.OSXSAVE); AVX also needs XCR0 state at runtime.
    pub osxsave: bool,
    /// Tracks an explicit target even when its extension set is empty.
    pub explicit: bool,
}
impl Default for Features {
    fn default() -> Self {
        Self::legacy()
    }
}
impl Features {
    pub const NONE: Self = Self {
        aes: false,
        adx: false,
        sha: false,
        vaes: false,
        vpclmulqdq: false,
        avx: false,
        bmi1: false,
        bmi2: false,
        clflush: false,
        pclmulqdq: false,
        cx16: false,
        lahf_sahf: false,
        popcnt: false,
        sse3: false,
        ssse3: false,
        sse41: false,
        sse42: false,
        avx2: false,
        f16c: false,
        fma: false,
        lzcnt: false,
        movbe: false,
        xsave: false,
        osxsave: false,
        explicit: false,
    };
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
    pub fn contains(self, required: Self) -> bool {
        (!required.adx || self.adx)
            && (!required.sha || self.sha)
            && (!required.vaes || self.vaes)
            && (!required.vpclmulqdq || self.vpclmulqdq)
            && (!required.aes || self.aes)
            && (!required.avx || self.avx)
            && (!required.bmi1 || self.bmi1)
            && (!required.bmi2 || self.bmi2)
            && (!required.clflush || self.clflush)
            && (!required.pclmulqdq || self.pclmulqdq)
            && (!required.cx16 || self.cx16)
            && (!required.lahf_sahf || self.lahf_sahf)
            && (!required.popcnt || self.popcnt)
            && (!required.sse3 || self.sse3)
            && (!required.ssse3 || self.ssse3)
            && (!required.sse41 || self.sse41)
            && (!required.sse42 || self.sse42)
            && (!required.avx2 || self.avx2)
            && (!required.f16c || self.f16c)
            && (!required.fma || self.fma)
            && (!required.lzcnt || self.lzcnt)
            && (!required.movbe || self.movbe)
            && (!required.xsave || self.xsave)
            && (!required.osxsave || self.osxsave)
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
            let flag = match name {
                "aes" => &mut self.aes,
                "adx" => &mut self.adx,
                "sha" => &mut self.sha,
                "vaes" => &mut self.vaes,
                "vpclmulqdq" => &mut self.vpclmulqdq,
                "avx" => &mut self.avx,
                "bmi1" => &mut self.bmi1,
                "bmi2" => &mut self.bmi2,
                "pclmulqdq" | "pclmul" => &mut self.pclmulqdq,
                "clflush" => &mut self.clflush,
                "cx16" => &mut self.cx16,
                "lahf_sahf" => &mut self.lahf_sahf,
                "popcnt" => &mut self.popcnt,
                "sse3" => &mut self.sse3,
                "ssse3" => &mut self.ssse3,
                "sse41" => &mut self.sse41,
                "sse42" => &mut self.sse42,
                "avx2" => &mut self.avx2,
                "f16c" => &mut self.f16c,
                "fma" => &mut self.fma,
                "lzcnt" => &mut self.lzcnt,
                "movbe" => &mut self.movbe,
                "xsave" => &mut self.xsave,
                "osxsave" => &mut self.osxsave,
                _ => return Err(format!("unknown x86 target feature `{token}`")),
            };
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
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        [
            (self.aes, "aes"),
            (self.adx, "adx"),
            (self.sha, "sha"),
            (self.vaes, "vaes"),
            (self.vpclmulqdq, "vpclmulqdq"),
            (self.avx, "avx"),
            (self.bmi1, "bmi1"),
            (self.bmi2, "bmi2"),
            (self.clflush, "clflush"),
            (self.pclmulqdq, "pclmulqdq"),
            (self.cx16, "cx16"),
            (self.lahf_sahf, "lahf_sahf"),
            (self.popcnt, "popcnt"),
            (self.sse3, "sse3"),
            (self.ssse3, "ssse3"),
            (self.sse41, "sse41"),
            (self.sse42, "sse42"),
            (self.avx2, "avx2"),
            (self.f16c, "f16c"),
            (self.fma, "fma"),
            (self.lzcnt, "lzcnt"),
            (self.movbe, "movbe"),
            (self.xsave, "xsave"),
            (self.osxsave, "osxsave"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
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
