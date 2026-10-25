//! Shared compiler/assembler feature resolution. Target-option provenance is
//! separate from the capability set, including explicit baseline requests.
use keel_asm::x86::{CpuLevel, Features};

#[derive(Default)]
pub(crate) struct X86Options {
    level: Option<CpuLevel>,
    modifiers: Vec<String>,
}
impl X86Options {
    pub(crate) fn set_level(&mut self, name: &str) -> Result<(), String> {
        if self.level.is_some() {
            return Err("--cpu-level may only be specified once".into());
        }
        self.level = Some(CpuLevel::parse(name)?);
        Ok(())
    }
    pub(crate) fn push_features(&mut self, source: &str) {
        self.modifiers.push(source.into());
    }
    pub(crate) fn supplied(&self) -> bool {
        self.level.is_some() || !self.modifiers.is_empty()
    }
    pub(crate) fn resolve(&self) -> Result<Features, String> {
        self.level
            .map(Features::for_level)
            .unwrap_or_default()
            .with_feature_lists(self.modifiers.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_position_and_repeated_modifiers_have_one_resolution() {
        let mut before = X86Options::default();
        before.push_features("-avx2");
        before.set_level("v3").unwrap();
        before.push_features("aes");
        let mut after = X86Options::default();
        after.set_level("v3").unwrap();
        after.push_features("-avx2");
        after.push_features("aes");
        assert_eq!(before.resolve().unwrap(), after.resolve().unwrap());
        let f = after.resolve().unwrap();
        assert!(!f.avx2 && f.aes);
        assert!(after.set_level("v2").is_err());
        assert!(X86Options::default().set_level("legacy").is_err());
        after.push_features("none");
        assert!(after.resolve().unwrap().is_empty());
        assert!(after.supplied());
    }
    #[test]
    fn missing_prerequisites_are_checked_after_all_edits() {
        let mut options = X86Options::default();
        options.push_features("avx2,-avx");
        options.push_features("avx");
        assert!(options.resolve().unwrap().avx2);
        options.push_features("-xsave");
        assert!(options.resolve().is_err());
    }
}
