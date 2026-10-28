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
fn sequence<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    if v[key].is_null() {
        Ok(&[])
    } else {
        v[key]
            .as_sequence()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("TBD `{key}` must be a list"))
    }
}
fn list<'a>(v: &'a Value, key: &str) -> Result<impl Iterator<Item = &'a str>> {
    let values = sequence(v, key)?;
    // Validate the entire list before iteration: a matching target must not
    // hide a malformed later entry through short-circuiting.
    if values.iter().any(|value| value.as_str().is_none()) {
        return fail(format!("TBD `{key}` contains a non-string"));
    }
    Ok(values.iter().map(|value| value.as_str().unwrap()))
}
fn compatible(v: &Value) -> Result<bool> {
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
                for group in sequence(v, key)? {
                    if compatible(group)? {
                        exports.extend(list(group, "symbols")?.map(str::to_owned));
                        let weak = list(group, "weak-symbols")?;
                        for name in weak {
                            exports.insert(name.to_owned());
                            weak_exports.insert(name.to_owned());
                        }
                        exports.extend(list(group, "thread-local-symbols")?.map(str::to_owned));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_target_does_not_hide_invalid_later_entries() {
        let value: Value = serde_yaml::from_str("targets: [arm64-macos, 42]").unwrap();
        assert_eq!(
            compatible(&value).unwrap_err(),
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
