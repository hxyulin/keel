//! The suite registry: `suite/configs.toml` and one `suite/c/<name>/suite.toml`
//! per program. `suite/README.md` describes the format.

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// `suite/configs.toml`: the ways a program is built.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configs {
    /// What benchmark ratios are against.
    pub baseline: String,
    /// What `test` builds by default.
    pub test: Vec<String>,
    pub config: Vec<Config>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub compiler: Compiler,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compiler {
    /// `clang -c` on the preprocessed file.
    Clang,
    /// `keel cc`, then `clang -c` on its assembly.
    Keel,
}

impl Configs {
    pub fn load(root: &Path) -> Result<Configs> {
        let path = root.join("suite/configs.toml");
        let text = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let configs: Configs =
            toml::from_str(&text).with_context(|| format!("in {}", path.display()))?;
        for name in configs.test.iter().chain([&configs.baseline]) {
            configs.get(name)?;
        }
        Ok(configs)
    }

    pub fn get(&self, name: &str) -> Result<&Config> {
        self.config.iter().find(|c| c.name == name).ok_or_else(|| {
            let names: Vec<_> = self.config.iter().map(|c| c.name.as_str()).collect();
            anyhow!(
                "no configuration `{name}` (there are: {})",
                names.join(", ")
            )
        })
    }

    /// The configurations named, in the order given, or `default` if none are.
    pub fn select(&self, names: &[String], default: &[String]) -> Result<Vec<Config>> {
        let names = if names.is_empty() { default } else { names };
        names.iter().map(|n| self.get(n).cloned()).collect()
    }
}

/// One program: `suite/c/<name>/suite.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub description: String,
    #[serde(default)]
    pub fetch: Vec<Fetch>,
    pub build: Build,
    pub test: Option<Test>,
    #[serde(default)]
    pub bench: Vec<Bench>,
}

/// An archive to download, check and unpack into the work directory.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fetch {
    pub url: String,
    pub sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    /// Where `prepare` and the preprocessor run, relative to the work
    /// directory; `sources` and `includes` are relative to it.
    pub dir: String,
    /// The `-std=` of the preprocessor.
    pub std: String,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    /// Libraries to link, without `-l`.
    #[serde(default)]
    pub libs: Vec<String>,
    /// Commands run once, before preprocessing (to generate sources).
    #[serde(default)]
    pub prepare: Vec<String>,
    /// Files, or `*` patterns in a file name (`src/*.c`).
    pub sources: Vec<String>,
    /// File names left out of `sources`.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// The executable's name.
    pub binary: String,
    /// Additions for some hosts.
    #[serde(default)]
    pub when: Vec<When>,
}

/// Defines and libraries added when the host matches.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// `macos` or `linux`.
    pub os: Option<String>,
    /// `aarch64` or `x86_64`.
    pub arch: Option<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub libs: Vec<String>,
}

impl When {
    fn applies(&self) -> bool {
        self.os
            .as_deref()
            .is_none_or(|os| os == std::env::consts::OS)
            && self
                .arch
                .as_deref()
                .is_none_or(|arch| arch == std::env::consts::ARCH)
    }
}

impl Build {
    fn when(&self) -> impl Iterator<Item = &When> {
        self.when.iter().filter(|w| w.applies())
    }

    pub fn define_flags(&self) -> Vec<String> {
        let extra = self.when().flat_map(|w| &w.defines);
        self.defines
            .iter()
            .chain(extra)
            .map(|d| format!("-D{d}"))
            .collect()
    }

    pub fn include_flags(&self) -> Vec<String> {
        self.includes.iter().map(|i| format!("-I{i}")).collect()
    }

    pub fn lib_flags(&self) -> Vec<String> {
        let extra = self.when().flat_map(|w| &w.libs);
        self.libs
            .iter()
            .chain(extra)
            .map(|l| format!("-l{l}"))
            .collect()
    }
}

/// The program's own tests: each command must succeed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Test {
    /// Where the commands run, relative to the work directory.
    pub dir: Option<String>,
    pub run: Vec<String>,
    /// A file fed to each command's standard input.
    pub stdin: Option<String>,
    /// A configuration whose output each command's must equal.
    pub compare: Option<String>,
}

/// A timed workload. Every configuration must print the same as the
/// baseline before it is timed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bench {
    pub name: String,
    pub run: String,
    pub stdin: Option<String>,
}

pub struct Suite {
    pub name: String,
    /// `suite/c/<name>`, where the manifest and its data files are.
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// The manifest's text, part of what decides if a cached build is stale.
    pub text: String,
}

/// Every `suite/c/*/suite.toml`, by name.
pub fn load_suites(root: &Path) -> Result<Vec<Suite>> {
    let mut suites = Vec::new();
    for entry in fs::read_dir(root.join("suite/c"))? {
        let dir = entry?.path();
        let path = dir.join("suite.toml");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let manifest = toml::from_str(&text).with_context(|| format!("in {}", path.display()))?;
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        suites.push(Suite {
            name,
            dir,
            manifest,
            text,
        });
    }
    suites.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(suites)
}

impl Suite {
    /// Where the suite is downloaded and built: `target/suite/<name>`.
    pub fn work(&self, root: &Path) -> PathBuf {
        root.join("target/suite").join(&self.name)
    }

    /// The variables of the suite's commands; `{bin}` once there is a build.
    pub fn vars(&self, work: &Path, bin: Option<&Path>) -> Vars {
        let mut words = vec![
            ("suite", self.dir.display().to_string()),
            ("work", work.display().to_string()),
            ("gen", work.join("gen").display().to_string()),
        ];
        if let Some(bin) = bin {
            words.push(("bin", bin.display().to_string()));
        }
        let build = &self.manifest.build;
        Vars {
            words,
            lists: vec![
                ("defines", build.define_flags()),
                ("libs", build.lib_flags()),
            ],
        }
    }

    /// The source files, resolved in the build directory `dir`.
    pub fn source_files(&self, dir: &Path, vars: &Vars) -> Result<Vec<PathBuf>> {
        let build = &self.manifest.build;
        let mut files = Vec::new();
        for pattern in &build.sources {
            let path = dir.join(vars.word(pattern)?);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some((prefix, suffix)) = name.split_once('*') else {
                if !path.is_file() {
                    bail!("source `{}` does not exist", path.display());
                }
                files.push(path);
                continue;
            };
            let parent = path.parent().unwrap();
            let mut found: Vec<PathBuf> = fs::read_dir(parent)
                .with_context(|| parent.display().to_string())?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    let n = p.file_name().unwrap().to_string_lossy();
                    n.len() >= prefix.len() + suffix.len()
                        && n.starts_with(prefix)
                        && n.ends_with(suffix)
                })
                .collect();
            if found.is_empty() {
                bail!("no sources match `{}`", path.display());
            }
            found.sort();
            files.extend(found);
        }
        files.retain(|f| {
            let name = f.file_name().unwrap().to_string_lossy();
            !build.exclude.iter().any(|e| *e == name)
        });
        let mut stems: Vec<_> = files.iter().map(|f| f.file_stem().unwrap()).collect();
        stems.sort();
        if let Some(w) = stems.windows(2).find(|w| w[0] == w[1]) {
            bail!("two sources are named `{}`", w[0].to_string_lossy());
        }
        Ok(files)
    }
}

/// Substitutes `{name}` in commands. A word that is just a list variable
/// (`{defines}`, `{libs}`) becomes one argument per element. Commands are
/// split at whitespace and run without a shell.
pub struct Vars {
    words: Vec<(&'static str, String)>,
    lists: Vec<(&'static str, Vec<String>)>,
}

impl Vars {
    pub fn expand(&self, command: &str) -> Result<Vec<String>> {
        let mut argv = Vec::new();
        for word in command.split_whitespace() {
            let name = word.strip_prefix('{').and_then(|w| w.strip_suffix('}'));
            if let Some((_, list)) = self.lists.iter().find(|(k, _)| Some(*k) == name) {
                argv.extend(list.iter().cloned());
            } else {
                argv.push(self.word(word)?);
            }
        }
        if argv.is_empty() {
            bail!("empty command");
        }
        Ok(argv)
    }

    pub fn word(&self, word: &str) -> Result<String> {
        let mut out = String::new();
        let mut rest = word;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let close = rest[open..]
                .find('}')
                .ok_or_else(|| anyhow!("unclosed `{{` in `{word}`"))?
                + open;
            let name = &rest[open + 1..close];
            let (_, value) = self
                .words
                .iter()
                .find(|(k, _)| *k == name)
                .ok_or_else(|| anyhow!("unknown variable `{{{name}}}` in `{word}`"))?;
            out.push_str(value);
            rest = &rest[close + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Every manifest in the repository parses, and its configurations exist.
    #[test]
    fn registry_loads() {
        let configs = Configs::load(&root()).unwrap();
        let suites = load_suites(&root()).unwrap();
        assert!(suites.len() >= 3);
        for suite in &suites {
            if let Some(compare) = suite
                .manifest
                .test
                .as_ref()
                .and_then(|t| t.compare.as_ref())
            {
                configs.get(compare).unwrap();
            }
            let vars = suite.vars(Path::new("/w"), Some(Path::new("/b")));
            for bench in &suite.manifest.bench {
                vars.expand(&bench.run).unwrap();
            }
        }
    }

    #[test]
    fn expansion() {
        let vars = Vars {
            words: vec![("bin", "/b/lua".into()), ("gen", "/g".into())],
            lists: vec![("libs", vec!["-lm".into(), "-ldl".into()])],
        };
        assert_eq!(
            vars.expand("{bin} -o {gen}/x.c {libs}").unwrap(),
            ["/b/lua", "-o", "/g/x.c", "-lm", "-ldl"]
        );
        assert!(vars.expand("{nope}").is_err());
    }
}
