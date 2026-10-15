//! Deliberately bounded TAPI v4 reader. YAML syntax is handled by serde_yaml;
//! only macOS arm64-compatible exports reachable through reexports are exposed.
use crate::{Result, fail};
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Dylib {
    pub install_name: String,
    pub exports: BTreeSet<String>,
    /// Exports participating in dyld process-wide weak coalescing.
    pub weak_exports: BTreeSet<String>,
}
fn scalar(v: &Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("TBD missing string `{key}`"))
}
fn list(v: &Value, key: &str) -> Result<Vec<String>> {
    if v[key].is_null() {
        return Ok(Vec::new());
    }
    v[key]
        .as_sequence()
        .ok_or_else(|| format!("TBD `{key}` must be a list"))?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("TBD `{key}` contains a non-string"))
        })
        .collect()
}
fn compatible(v: &Value) -> Result<bool> {
    // Apple's current libSystem SDK umbrella lists arm64e but is also the
    // arm64 link interface. Do not accept Catalyst, iOS, or arm64e.x1 alone.
    Ok(list(v, "targets")?
        .iter()
        .any(|s| matches!(s.as_str(), "arm64-macos" | "arm64e-macos")))
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
        for doc in serde_yaml::Deserializer::from_str(&text) {
            let value: Value = <Value as serde::Deserialize>::deserialize(doc)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let v = match value {
                Value::Tagged(t) => t.value,
                v => v,
            };
            if v["tbd-version"].as_u64() != Some(4) {
                return fail(format!("{}: only TAPI TBD v4 is supported", path.display()));
            }
            let install = scalar(&v, "install-name")?;
            first.get_or_insert(install.clone());
            if docs.insert(install, v).is_some() {
                return fail("duplicate TBD install-name");
            }
        }
        let install_name = first.ok_or("empty TBD file")?;
        let mut exports = BTreeSet::new();
        let mut weak_exports = BTreeSet::new();
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
                exports.extend(child.exports);
                weak_exports.extend(child.weak_exports);
                continue;
            };
            if !compatible(v)? {
                return fail(format!("TBD `{install}` has no macOS arm64 interface"));
            }
            for key in ["exports", "reexports"] {
                if let Some(groups) = v[key].as_sequence() {
                    for group in groups {
                        if compatible(group)? {
                            exports.extend(list(group, "symbols")?);
                            let weak = list(group, "weak-symbols")?;
                            exports.extend(weak.iter().cloned());
                            weak_exports.extend(weak);
                            exports.extend(list(group, "thread-local-symbols")?);
                            // TLV imports bind descriptor addresses; Objective-C synthesis remains unsupported.
                        }
                    }
                }
            }
            if let Some(groups) = v["reexported-libraries"].as_sequence() {
                for group in groups {
                    if compatible(group)? {
                        todo.extend(list(group, "libraries")?);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
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
