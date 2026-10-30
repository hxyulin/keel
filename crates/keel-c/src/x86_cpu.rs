//! Bounded runtime capability queries. AVX permissions include OS vector state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Register {
    Ebx,
    Ecx,
    Edx,
}
impl Register {
    pub(crate) const fn shift(self) -> i64 {
        match self {
            Self::Ebx => 32,
            Self::Ecx => 64,
            Self::Edx => 96,
        }
    }
}
/// Executable vector state required in addition to the queried CPUID bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VectorState {
    None,
    Avx,
    Avx512,
}
impl VectorState {
    pub(crate) const fn xcr0_mask(self) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Avx => Some(0x06),
            Self::Avx512 => Some(0xe6),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Feature {
    pub(crate) name: &'static str,
    pub(crate) leaf: u32,
    pub(crate) register: Register,
    pub(crate) mask: u32,
    pub(crate) state: VectorState,
}
/// CPUID leaf1 ECX must admit saving and executing AVX before XCR0 can grant it.
pub(crate) const AVX_CPUID: u32 = (1 << 26) | (1 << 27) | (1 << 28);
pub(crate) const FEATURES: &[Feature] = &[
    Feature {
        name: "monitor",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 3,
        state: VectorState::None,
    },
    Feature {
        name: "sse",
        leaf: 1,
        register: Register::Edx,
        mask: 1 << 25,
        state: VectorState::None,
    },
    Feature {
        name: "sse2",
        leaf: 1,
        register: Register::Edx,
        mask: 1 << 26,
        state: VectorState::None,
    },
    Feature {
        name: "sse3",
        leaf: 1,
        register: Register::Ecx,
        mask: 1,
        state: VectorState::None,
    },
    Feature {
        name: "ssse3",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 9,
        state: VectorState::None,
    },
    Feature {
        name: "sse4.1",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 19,
        state: VectorState::None,
    },
    Feature {
        name: "sse4.2",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 20,
        state: VectorState::None,
    },
    Feature {
        name: "avx",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 28,
        state: VectorState::Avx,
    },
    Feature {
        name: "avx2",
        leaf: 7,
        register: Register::Ebx,
        mask: 1 << 5,
        state: VectorState::Avx,
    },
    Feature {
        name: "avx512f",
        leaf: 7,
        register: Register::Ebx,
        mask: 1 << 16,
        state: VectorState::Avx512,
    },
    Feature {
        name: "avx512vl",
        leaf: 7,
        register: Register::Ebx,
        mask: (1 << 16) | (1 << 31),
        state: VectorState::Avx512,
    },
    Feature {
        name: "avx512dq",
        leaf: 7,
        register: Register::Ebx,
        mask: (1 << 16) | (1 << 17),
        state: VectorState::Avx512,
    },
    Feature {
        name: "avx512bw",
        leaf: 7,
        register: Register::Ebx,
        mask: (1 << 16) | (1 << 30),
        state: VectorState::Avx512,
    },
    Feature {
        name: "avx512cd",
        leaf: 7,
        register: Register::Ebx,
        mask: (1 << 16) | (1 << 28),
        state: VectorState::Avx512,
    },
    Feature {
        name: "aes",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 25,
        state: VectorState::None,
    },
    Feature {
        name: "pclmul",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 1,
        state: VectorState::None,
    },
    Feature {
        name: "popcnt",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 23,
        state: VectorState::None,
    },
    Feature {
        name: "bmi",
        leaf: 7,
        register: Register::Ebx,
        mask: 1 << 3,
        state: VectorState::None,
    },
    Feature {
        name: "bmi2",
        leaf: 7,
        register: Register::Ebx,
        mask: 1 << 8,
        state: VectorState::None,
    },
    Feature {
        name: "cx16",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 13,
        state: VectorState::None,
    },
    Feature {
        name: "movbe",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 22,
        state: VectorState::None,
    },
    Feature {
        name: "lzcnt",
        leaf: 0x8000_0001,
        register: Register::Ecx,
        mask: 1 << 5,
        state: VectorState::None,
    },
    Feature {
        name: "xsave",
        leaf: 1,
        register: Register::Ecx,
        mask: (1 << 26) | (1 << 27),
        state: VectorState::None,
    },
    Feature {
        name: "fma",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 12,
        state: VectorState::Avx,
    },
    Feature {
        name: "f16c",
        leaf: 1,
        register: Register::Ecx,
        mask: 1 << 29,
        state: VectorState::Avx,
    },
];
pub(crate) fn lookup(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|feature| feature.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Snapshot {
        basic_max: u32,
        extended_max: u32,
        leaf1: [u32; 4],
        leaf7: [u32; 4],
        extended: [u32; 4],
        xcr0: u64,
    }
    impl Snapshot {
        fn supports(&self, name: &str) -> bool {
            let feature = lookup(name).unwrap();
            let maximum = if feature.leaf >= 0x8000_0000 {
                self.extended_max
            } else {
                self.basic_max
            };
            if maximum < feature.leaf {
                return false;
            }
            let row = match feature.leaf {
                1 => self.leaf1,
                7 => self.leaf7,
                0x8000_0001 => self.extended,
                _ => unreachable!(),
            };
            let register = match feature.register {
                Register::Ebx => 1,
                Register::Ecx => 2,
                Register::Edx => 3,
            };
            row[register] & feature.mask == feature.mask
                && feature.state.xcr0_mask().is_none_or(|mask| {
                    self.basic_max >= 1
                        && self.leaf1[2] & AVX_CPUID == AVX_CPUID
                        && self.xcr0 & mask == mask
                })
        }
    }
    fn all() -> Snapshot {
        Snapshot {
            basic_max: 7,
            extended_max: 0x8000_0001,
            leaf1: [u32::MAX; 4],
            leaf7: [u32::MAX; 4],
            extended: [u32::MAX; 4],
            xcr0: 0xe6,
        }
    }
    #[test]
    fn avx_requires_cpu_and_os_state_for_every_dependent_feature() {
        for name in ["avx", "avx2", "fma", "f16c"] {
            assert!(all().supports(name));
            for xcr0 in [0, 2, 4] {
                let mut snapshot = all();
                snapshot.xcr0 = xcr0;
                assert!(!snapshot.supports(name), "{name} xcr0={xcr0}");
            }
            for bit in [26, 27, 28] {
                let mut snapshot = all();
                snapshot.leaf1[2] &= !(1 << bit);
                assert!(!snapshot.supports(name), "{name} CPUID bit {bit}");
            }
        }
        let mut snapshot = all();
        snapshot.xcr0 = 0;
        assert!(snapshot.supports("bmi2"));
        assert!(snapshot.supports("aes"));
    }
    #[test]
    fn absent_leaves_and_unset_bits_do_not_grant_capabilities() {
        for feature in FEATURES {
            assert!(all().supports(feature.name));
            let mut snapshot = all();
            if feature.leaf >= 0x8000_0000 {
                snapshot.extended_max = 0x8000_0000;
            } else {
                snapshot.basic_max = feature.leaf - 1;
            }
            assert!(
                !snapshot.supports(feature.name),
                "{} missing leaf",
                feature.name
            );
            let mut snapshot = all();
            let row = match feature.leaf {
                1 => &mut snapshot.leaf1,
                7 => &mut snapshot.leaf7,
                _ => &mut snapshot.extended,
            };
            let reg = match feature.register {
                Register::Ebx => 1,
                Register::Ecx => 2,
                Register::Edx => 3,
            };
            row[reg] &= !feature.mask;
            assert!(
                !snapshot.supports(feature.name),
                "{} missing bit",
                feature.name
            );
        }
        assert_eq!(lookup("avx512f").unwrap().state, VectorState::Avx512);
        assert_eq!(lookup("avx512vl").unwrap().state, VectorState::Avx512);
        assert!(lookup("arbitrary").is_none());
    }
    #[test]
    fn foundation_and_extensions_require_every_vector_state_component() {
        for name in ["avx512f", "avx512vl", "avx512dq", "avx512bw", "avx512cd"] {
            assert!(all().supports(name));
            for bit in [1, 2, 5, 6, 7] {
                let mut snapshot = all();
                snapshot.xcr0 &= !(1 << bit);
                assert!(!snapshot.supports(name), "{name}: missing XCR0 bit {bit}");
            }
            for bit in [26, 27, 28] {
                let mut snapshot = all();
                snapshot.leaf1[2] &= !(1 << bit);
                assert!(
                    !snapshot.supports(name),
                    "{name}: missing CPUID.1 ECX bit {bit}"
                );
            }
            let mut snapshot = all();
            snapshot.xcr0 = 6;
            assert!(snapshot.supports("avx2"));
            assert!(
                !snapshot.supports(name),
                "AVX state alone cannot grant AVX-512"
            );
        }
        let mut snapshot = all();
        snapshot.leaf7[1] &= !(1 << 16);
        assert!(!snapshot.supports("avx512f"));
        for name in ["avx512vl", "avx512dq", "avx512bw", "avx512cd"] {
            assert!(!snapshot.supports(name), "{name} also requires foundation");
        }
        let mut snapshot = all();
        snapshot.leaf7[1] &= !(1 << 31);
        assert!(snapshot.supports("avx512f"));
        assert!(!snapshot.supports("avx512vl"));
    }
    #[test]
    fn avx512_extension_bits_are_independent() {
        let extensions = [
            ("avx512vl", 31),
            ("avx512dq", 17),
            ("avx512bw", 30),
            ("avx512cd", 28),
        ];
        for (name, bit) in extensions {
            assert_eq!(lookup(name).unwrap().state, VectorState::Avx512);
            let mut snapshot = all();
            snapshot.leaf7[1] = (1 << 16) | (1 << bit);
            assert!(snapshot.supports("avx512f"));
            assert!(snapshot.supports(name), "{name}");
            for (other, _) in extensions {
                assert_eq!(
                    snapshot.supports(other),
                    other == name,
                    "{name} must not imply {other}"
                );
            }
            snapshot.leaf7[1] = 1 << 16;
            assert!(!snapshot.supports(name), "{name}: missing own bit {bit}");
            assert!(snapshot.supports("avx512f"));
            snapshot.leaf7[1] = 1 << bit;
            assert!(!snapshot.supports(name), "{name}: missing foundation bit");
        }
    }
}
