//! Deliberately bounded TAPI v4 reader. `yaml` parses the YAML subset that
//! TAPI writes; only macOS arm64-compatible exports reachable through
//! reexports are exposed.
mod yaml;
use crate::{Result, fail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use yaml::Node;

#[derive(Clone, Debug)]
pub struct Dylib {
    pub install_name: String,
    pub exports: SymbolSet,
    /// Exports participating in dyld process-wide weak coalescing.
    pub weak_exports: SymbolSet,
}
fn scalar(v: &Node<'_>, key: &str) -> Result<String> {
    v.get(key)
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("TBD missing string `{key}`").into())
}
fn sequence<'n, 'a>(v: &'n Node<'a>, key: &str) -> Result<&'n [Node<'a>]> {
    let value = v.get(key);
    if value.is_null() {
        Ok(&[])
    } else {
        value
            .as_seq()
            .ok_or_else(|| format!("TBD `{key}` must be a list").into())
    }
}
fn list<'n>(v: &'n Node<'_>, key: &str) -> Result<impl Iterator<Item = &'n str>> {
    let values = sequence(v, key)?;
    // Validate the entire list before iteration: a matching target must not
    // hide a malformed later entry through short-circuiting.
    if values.iter().any(|value| value.as_str().is_none()) {
        return fail(format!("TBD `{key}` contains a non-string"));
    }
    Ok(values.iter().map(|value| value.as_str().unwrap()))
}
fn compatible(v: &Node<'_>) -> Result<bool> {
    // Apple's current libSystem SDK umbrella lists arm64e but is also the
    // arm64 link interface. Do not accept Catalyst, iOS, or arm64e.x1 alone.
    Ok(list(v, "targets")?.any(|s| matches!(s, "arm64-macos" | "arm64e-macos")))
}
impl Dylib {
    pub fn from_tbd(path: &Path, sysroot: &Path) -> Result<Self> {
        let mut seen = BTreeSet::new();
        Self::load(path, sysroot, &mut seen)
    }
    fn load(path: &Path, root: &Path, seen: &mut BTreeSet<PathBuf>) -> Result<Self> {
        if !seen.insert(path.to_owned()) {
            return fail(format!("cyclic TBD reexport: {}", path.display()));
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut docs = BTreeMap::new();
        let mut first = None;
        for v in yaml::documents(&text).map_err(|e| format!("{}: {e}", path.display()))? {
            if v.get("tbd-version").as_u64() != Some(4) {
                return fail(format!("{}: only TAPI TBD v4 is supported", path.display()));
            }
            let install = scalar(&v, "install-name")?;
            first.get_or_insert(install.clone());
            if docs.insert(install, v).is_some() {
                return fail("duplicate TBD install-name");
            }
        }
        let install_name = first.ok_or("empty TBD file")?;
        let mut exports = SymbolSet::default();
        let mut weak_exports = SymbolSet::default();
        let mut visited = BTreeSet::new();
        let mut todo = vec![install_name.clone()];
        while let Some(install) = todo.pop() {
            if !visited.insert(install.clone()) {
                continue;
            }
            let Some(v) = docs.get(&install) else {
                let relative = install
                    .strip_prefix('/')
                    .ok_or("relative TBD reexport unsupported")?;
                let path = root.join(relative).with_extension("tbd");
                let child = Self::load(&path, root, seen)?;
                exports.extend(child.exports.iter());
                weak_exports.extend(child.weak_exports.iter());
                continue;
            };
            if !compatible(v)? {
                return fail(format!("TBD `{install}` has no macOS arm64 interface"));
            }
            for key in ["exports", "reexports"] {
                for group in sequence(v, key)? {
                    if compatible(group)? {
                        exports.extend(list(group, "symbols")?);
                        let weak = list(group, "weak-symbols")?;
                        for name in weak {
                            exports.insert(name);
                            weak_exports.insert(name);
                        }
                        exports.extend(list(group, "thread-local-symbols")?);
                        // TLV imports bind descriptor addresses; Objective-C synthesis remains unsupported.
                    }
                }
            }
            for group in sequence(v, "reexported-libraries")? {
                if compatible(group)? {
                    todo.extend(list(group, "libraries")?.map(str::to_owned));
                }
            }
        }
        seen.remove(path);
        Ok(Self {
            install_name,
            exports,
            weak_exports,
        })
    }
}

/// A set of symbol names held in one buffer. An interface exports thousands
/// of names; storing them together avoids one allocation per name, and the
/// hash table answers the linker's lookups without ordering the names.
#[derive(Clone, Default)]
pub struct SymbolSet {
    text: String,
    /// The end of each name in `text`; a name starts where the previous ends.
    ends: Vec<usize>,
    /// Open-addressed table of (name index plus one, high hash bits); a zero
    /// index marks a free slot.
    slots: Vec<(u32, u32)>,
}

impl SymbolSet {
    pub fn len(&self) -> usize {
        self.ends.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
    /// The names in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        (0..self.ends.len()).map(|i| self.name(i))
    }
    pub fn contains(&self, name: &str) -> bool {
        !self.slots.is_empty() && self.find(name, hash(name)).is_ok()
    }
    /// Add `name`; returns whether it was absent.
    pub fn insert(&mut self, name: &str) -> bool {
        if (self.ends.len() + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let hash = hash(name);
        match self.find(name, hash) {
            Ok(_) => false,
            Err(slot) => {
                self.text.push_str(name);
                self.ends.push(self.text.len());
                let index = u32::try_from(self.ends.len()).expect("symbol set too large");
                self.slots[slot] = (index, hash);
                true
            }
        }
    }
    fn name(&self, index: usize) -> &str {
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        &self.text[start..self.ends[index]]
    }
    /// The index of `name`, or the free slot where it belongs.
    fn find(&self, name: &str, tag: u32) -> std::result::Result<usize, usize> {
        let mask = self.slots.len() - 1;
        let mut slot = tag as usize & mask;
        loop {
            match self.slots[slot] {
                (0, _) => return Err(slot),
                (index, t) if t == tag && self.name(index as usize - 1) == name => {
                    return Ok(index as usize - 1);
                }
                _ => slot = (slot + 1) & mask,
            }
        }
    }
    fn grow(&mut self) {
        let size = (self.slots.len() * 2).max(64);
        let old = std::mem::replace(&mut self.slots, vec![(0, 0); size]);
        let mask = size - 1;
        for (index, tag) in old.into_iter().filter(|&(index, _)| index != 0) {
            // Names are distinct, and the tag holds the index bits: no comparison.
            let mut slot = tag as usize & mask;
            while self.slots[slot].0 != 0 {
                slot = (slot + 1) & mask;
            }
            self.slots[slot] = (index, tag);
        }
    }
}

/// FxHash over eight-byte words with a final mix. The table index comes from
/// the low bits of the result, which each slot stores to compare and rehash.
fn hash(name: &str) -> u32 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h = name.len() as u64;
    let (words, rest) = name.as_bytes().as_chunks::<8>();
    for word in words {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*word)).wrapping_mul(K);
    }
    if !rest.is_empty() {
        let mut word = [0; 8];
        word[..rest.len()].copy_from_slice(rest);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(word)).wrapping_mul(K);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    ((h ^ (h >> 33)) >> 32) as u32
}

impl std::fmt::Debug for SymbolSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.iter().collect();
        names.sort_unstable();
        f.debug_set().entries(names).finish()
    }
}

impl PartialEq for SymbolSet {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && other.iter().all(|name| self.contains(name))
    }
}
impl Eq for SymbolSet {}

impl PartialEq<BTreeSet<String>> for SymbolSet {
    fn eq(&self, other: &BTreeSet<String>) -> bool {
        self.len() == other.len() && other.iter().all(|name| self.contains(name))
    }
}

impl<S: AsRef<str>> Extend<S> for SymbolSet {
    fn extend<I: IntoIterator<Item = S>>(&mut self, names: I) {
        for name in names {
            self.insert(name.as_ref());
        }
    }
}

impl<S: AsRef<str>> FromIterator<S> for SymbolSet {
    fn from_iter<I: IntoIterator<Item = S>>(names: I) -> Self {
        let mut set = Self::default();
        set.extend(names);
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symbol_set_deduplicates_across_growth() {
        let names: Vec<String> = (0..5000).map(|i| format!("_symbol_{i}")).collect();
        let mut set = SymbolSet::default();
        assert!(!set.contains("_symbol_0"));
        for name in names.iter().chain(&names) {
            set.insert(name);
        }
        assert!(set.insert("") && !set.insert(""));
        assert_eq!(set.len(), names.len() + 1);
        assert!(set.contains("") && set.contains("_symbol_4999") && !set.contains("_symbol_5000"));
        let mut expected: BTreeSet<String> = names.into_iter().collect();
        expected.insert(String::new());
        assert_eq!(set, expected);
        assert_eq!(set.iter().nth(1), Some("_symbol_1"));
    }

    #[test]
    fn matching_target_does_not_hide_invalid_later_entries() {
        let value = &yaml::documents("targets: [arm64-macos, 42]").unwrap()[0];
        assert_eq!(
            compatible(value).unwrap_err(),
            "TBD `targets` contains a non-string"
        );
    }

    #[test]
    fn malformed_interface_containers_are_rejected() {
        let root = std::env::temp_dir().join(format!("keel-tbd-containers-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("lib.tbd");
        for key in ["exports", "reexports", "reexported-libraries"] {
            for invalid in ["42", "{ symbols: [_puts] }"] {
                std::fs::write(&path, format!("--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: /usr/lib/libtest.dylib\n{key}: {invalid}\n")).unwrap();
                let error = Dylib::from_tbd(&path, &root).unwrap_err();
                assert!(
                    error.contains(&format!("`{key}` must be a list")),
                    "{key}: {error}"
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filters_architecture_and_follows_only_reachable_reexports() {
        let root = std::env::temp_dir().join(format!("keel-tbd-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("lib.tbd");
        std::fs::write(
            &p,
            r#"--- !tapi-tbd
tbd-version: 4
targets: [arm64-macos]
install-name: '/usr/lib/libtest.dylib'
exports:
  - targets: [x86_64-macos]
    symbols: [_wrong]
  - targets: [arm64-macos]
    symbols: [_right]
    weak-symbols: [_weak]
    thread-local-symbols: [_thread]
reexported-libraries:
  - targets: [arm64-macos]
    libraries: ['/usr/lib/libchild.dylib']
--- !tapi-tbd
tbd-version: 4
targets: [arm64e-macos]
install-name: '/usr/lib/libchild.dylib'
exports:
  - targets: [arm64e-macos]
    symbols: [_child]
--- !tapi-tbd
tbd-version: 4
targets: [arm64-macos]
install-name: '/usr/lib/libunrelated.dylib'
exports:
  - targets: [arm64-macos]
    symbols: [_unrelated]
"#,
        )
        .unwrap();
        let d = Dylib::from_tbd(&p, &root).unwrap();
        assert_eq!(
            d.exports,
            BTreeSet::from([
                "_right".into(),
                "_child".into(),
                "_weak".into(),
                "_thread".into()
            ])
        );
        assert_eq!(d.weak_exports, BTreeSet::from(["_weak".into()]));
        std::fs::remove_dir_all(root).unwrap();
    }
}
