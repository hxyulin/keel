//! Downloading, preparing, preprocessing and building a suite.
//!
//! Native mode includes each compiler's preprocessing. Shared mode compiles
//! identical Clang-preprocessed inputs to isolate compilation after preprocessing.

use crate::manifest::{Compiler, Config, Fetch, Suite};

use crate::style::{bar, spinner};
use anyhow::{Context, Result, anyhow, bail};
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    #[default]
    Native,
    Shared,
}

pub struct Env {
    pub root: PathBuf,
    pub keel: PathBuf,
    pub jobs: usize,
    pub input_mode: InputMode,
}

/// Builds the keel driver, in debug (with its assertions) or release.
pub fn build_keel(root: &Path, release: bool) -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .args(["build", "-q", "-p", "keel"])
        .current_dir(root);
    if release {
        command.arg("--release");
    }
    let progress = spinner(format!(
        "building keel ({})",
        if release { "release" } else { "debug" }
    ));
    let status = command.status()?;
    progress.finish_and_clear();
    if !status.success() {
        bail!("building keel failed");
    }
    let profile = if release { "release" } else { "debug" };
    Ok(root.join("target").join(profile).join("keel"))
}

/// A suite's downloaded and prepared compiler inputs.
pub struct Sources {
    pub work: PathBuf,
    /// Raw C in native mode, or Clang-preprocessed files in shared mode.
    pub pre: Vec<PathBuf>,
    /// Digest of the manifest, maintained inputs and compiler inputs.
    stamp: String,
}

pub fn sources(env: &Env, suite: &Suite) -> Result<Sources> {
    let work = suite.work(&env.root);
    fs::create_dir_all(&work)?;
    for fetch in &suite.manifest.fetch {
        download(&work, fetch).with_context(|| format!("fetching {}", fetch.url))?;
    }
    let build = &suite.manifest.build;
    let dir = work.join(&build.dir);
    let vars = suite.vars(&work, None);
    let mut suite_inputs = Vec::new();
    collect_suite_inputs(&suite.dir, &mut suite_inputs)?;
    suite_inputs.sort();
    let stamp = source_digest(
        &suite_inputs,
        &hash(&[&suite.text, std::env::consts::OS, std::env::consts::ARCH]),
    )?;

    let generated = work.join("gen");
    if !build.prepare.is_empty() && !stamp_matches(&generated, &stamp) {
        fs::create_dir_all(&generated)?;
        let progress = spinner(format!("{}: preparing sources", suite.name));
        for command in &build.prepare {
            run_checked(&vars.expand(command)?, &dir, None)?;
        }
        progress.finish_and_clear();
        write_stamp(&generated, &stamp)?;
    }

    let files = suite.source_files(&dir, &vars)?;
    if env.input_mode == InputMode::Native {
        let mut dependencies = files.clone();
        collect_headers(&dir, &mut dependencies)?;
        collect_headers(&suite.dir, &mut dependencies)?;
        dependencies.sort();
        dependencies.dedup();
        let stamp = source_digest(&dependencies, &stamp)?;
        return Ok(Sources {
            work,
            pre: files,
            stamp,
        });
    }
    let pre_dir = work.join("pre");
    let pre: Vec<PathBuf> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            pre_dir
                .join(format!("{i}-{}", f.file_stem().unwrap().to_string_lossy()))
                .with_extension("i")
        })
        .collect();
    {
        // Reprocess headers and compiler builtins on every invocation.
        fs::create_dir_all(&pre_dir)?;
        let progress = bar(files.len(), format!("{} preprocess", suite.name));
        let jobs: Vec<_> = files.iter().zip(&pre).collect();
        parallel(&jobs, env.jobs, &progress, |(file, out)| {
            let mut argv = vec!["clang".into(), "-E".into(), format!("-std={}", build.std)];
            argv.extend(build.define_flags());
            argv.extend(build.include_flags());
            argv.extend([path(file), "-o".into(), path(out)]);
            run_checked(&argv, &dir, None).map(drop)
        })?;
        progress.finish_and_clear();
        write_stamp(&pre_dir, &stamp)?;
    }
    let stamp = source_digest(&pre, &stamp)?;
    Ok(Sources { work, pre, stamp })
}

fn download(work: &Path, fetch: &Fetch) -> Result<()> {
    let name = fetch.url.rsplit('/').next().unwrap();
    let file = work.join(name);
    if !file.exists() {
        let progress = spinner(format!("downloading {name}"));
        let partial = work.join(format!("{name}.part"));
        let argv = ["curl", "-sSfL", "-o", &path(&partial), &fetch.url].map(String::from);
        run_checked(&argv, work, None)?;
        fs::rename(&partial, &file)?;
        progress.finish_and_clear();
    }
    let digest: String = Sha256::digest(fs::read(&file)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != fetch.sha256 {
        bail!(
            "{name} has SHA-256 {digest}, expected {}; delete it to download it again",
            fetch.sha256
        );
    }
    let unpacked = work.join(format!(".{name}.unpacked"));
    if !unpacked.exists() {
        let argv: Vec<String> = if name.ends_with(".zip") {
            ["unzip", "-q", "-o", name].map(String::from).into()
        } else {
            ["tar", "xf", name].map(String::from).into()
        };
        run_checked(&argv, work, None)?;
        fs::write(unpacked, "")?;
    }
    Ok(())
}

/// One configuration's build of a suite.
#[derive(Serialize, Deserialize, Clone)]
pub struct BuildStat {
    pub suite: String,
    pub config: String,
    /// Wall-clock time of the build, files compiled in parallel.
    pub wall_ms: f64,
    /// The compilers' time summed over files: in `keel` and in the
    /// assembler for keel builds, in clang otherwise.
    pub compile_ms: f64,
    pub keel_ms: f64,
    pub asm_ms: f64,
    /// Size of the executable's code.
    pub text_bytes: Option<u64>,
    #[serde(default)]
    pub input_mode: InputMode,
    #[serde(default)]
    pub input_sha256: String,
    #[serde(default)]
    pub config_flags: Vec<String>,
    #[serde(default)]
    pub jobs: usize,
    #[serde(default)]
    pub link_ms: f64,
    #[serde(default)]
    pub compiler_version: String,
}

pub struct Built {
    pub bin: PathBuf,
    pub stat: BuildStat,
    pub cached: bool,
}

pub fn build(env: &Env, suite: &Suite, sources: &Sources, config: &Config) -> Result<Built> {
    let out = sources.work.join("build").join(&config.name);
    fs::create_dir_all(&out)?;
    let bin = out.join(&suite.manifest.build.binary);
    let stamp = hash(&[&sources.stamp, &config.flags.join(" ")]);
    let stat_file = out.join("stat.json");

    let progress = bar(sources.pre.len(), format!("{} {}", suite.name, config.name));
    let start = Instant::now();
    let units: Vec<_> = sources.pre.iter().enumerate().collect();
    let times = parallel(&units, env.jobs, &progress, |(i, file)| {
        compile(env, suite, config, file, *i, &out, &sources.work)
    });
    progress.finish_and_clear();
    let times = times.with_context(|| format!("building {} with {}", suite.name, config.name))?;
    let objects: Vec<String> = sources
        .pre
        .iter()
        .enumerate()
        .map(|(i, _)| path(&out.join(format!("unit-{i}.o"))))
        .collect();
    let mut argv = vec!["clang".to_string()];
    argv.extend(objects);
    argv.extend(["-o".into(), path(&bin)]);
    argv.extend(suite.manifest.build.lib_flags());
    let link_start = Instant::now();
    run_checked(&argv, &out, None).with_context(|| format!("linking {}", suite.name))?;
    let link_ms = link_start.elapsed().as_secs_f64() * 1000.0;

    let sum = |f: fn(&(Duration, Duration)) -> Duration| -> f64 {
        times.iter().map(f).sum::<Duration>().as_secs_f64() * 1000.0
    };
    let (first, second) = (sum(|t| t.0), sum(|t| t.1));
    let (keel_ms, asm_ms) = match config.compiler {
        Compiler::Keel => (first, second),
        Compiler::Clang => (0.0, 0.0),
    };
    let stat = BuildStat {
        suite: suite.name.clone(),
        config: config.name.clone(),
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
        compile_ms: first + second,
        keel_ms,
        asm_ms,
        text_bytes: text_size(&bin),
        input_mode: env.input_mode,
        input_sha256: sources.stamp.clone(),
        config_flags: config.flags.clone(),
        jobs: env.jobs,
        link_ms,
        compiler_version: {
            let compiler = match config.compiler {
                Compiler::Clang => Path::new("clang"),
                Compiler::Keel => &env.keel,
            };
            if config.compiler == Compiler::Keel {
                source_digest(std::slice::from_ref(&env.keel), "keel-executable")?
            } else {
                String::from_utf8_lossy(&Command::new(compiler).arg("--version").output()?.stdout)
                    .trim()
                    .to_string()
            }
        },
    };
    fs::write(&stat_file, serde_json::to_string(&stat)?)?;
    write_stamp(&out, &stamp)?;
    Ok(Built {
        bin,
        stat,
        cached: false,
    })
}

/// Compiles one translation unit to an object; gives the time of each
/// step (keel and the assembler, or clang and nothing).
fn compile(
    env: &Env,
    suite: &Suite,
    config: &Config,
    file: &Path,
    index: usize,
    out: &Path,
    work: &Path,
) -> Result<(Duration, Duration)> {
    let stem = format!("unit-{index}");
    let dir = work.join(&suite.manifest.build.dir);
    let input_flags = input_flags(env.input_mode, config.compiler, &suite.manifest.build);
    let object = path(&out.join(&stem).with_extension("o"));
    let start = Instant::now();
    match config.compiler {
        Compiler::Clang => {
            let mut argv = vec!["clang".into(), "-w".into(), "-c".into()];
            argv.extend(input_flags.iter().cloned());
            argv.extend(config.flags.iter().cloned());
            argv.extend([path(file), "-o".into(), object]);
            run_checked(&argv, &dir, None)?;
            Ok((start.elapsed(), Duration::ZERO))
        }
        Compiler::Keel if config.flags.iter().any(|flag| flag == "--emit-object") => {
            let mut argv = vec![path(&env.keel), "cc".into()];
            argv.extend(input_flags.iter().cloned());
            argv.extend(config.flags.iter().cloned());
            argv.extend([path(file), "-o".into(), object]);
            run_checked(&argv, &dir, None)?;
            Ok((start.elapsed(), Duration::ZERO))
        }
        Compiler::Keel => {
            let asm = path(&out.join(&stem).with_extension("s"));
            let mut argv = vec![path(&env.keel), "cc".into()];
            argv.extend(input_flags.iter().cloned());
            argv.extend(config.flags.iter().cloned());
            argv.extend([path(file), "-o".into(), asm.clone()]);
            run_checked(&argv, &dir, None)?;
            let keel = start.elapsed();
            let start = Instant::now();
            let argv = ["clang", "-c", &asm, "-o", &object].map(String::from);
            run_checked(&argv, &dir, None)?;
            Ok((keel, start.elapsed()))
        }
    }
}

fn input_flags(mode: InputMode, compiler: Compiler, build: &crate::manifest::Build) -> Vec<String> {
    if mode == InputMode::Shared {
        return Vec::new();
    }
    let mut flags = Vec::new();
    // Keel's C frontend currently has one language mode and accepts no -std flag.
    if compiler == Compiler::Clang {
        flags.push(format!("-std={}", build.std));
    }
    flags.extend(build.define_flags());
    flags.extend(build.include_flags());
    flags
}

fn collect_suite_inputs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if entry.file_name() != "__pycache__" {
                collect_suite_inputs(&entry.path(), files)?;
            }
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn collect_headers(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_headers(&entry.path(), files)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e == "h" || e == "inc")
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn source_digest(files: &[PathBuf], stamp: &str) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(stamp.as_bytes());
    for file in files {
        hasher.update(file.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(fs::read(file)?);
        hasher.update([0]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Runs `f` on every item, on up to `jobs` threads; stops at the first error.
fn parallel<T: Sync, R: Send>(
    items: &[T],
    jobs: usize,
    progress: &ProgressBar,
    f: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let error = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() || error.lock().unwrap().is_some() {
                        break;
                    }
                    match f(&items[i]) {
                        Ok(r) => *results[i].lock().unwrap() = Some(r),
                        Err(e) => {
                            error.lock().unwrap().get_or_insert(e);
                        }
                    }
                    progress.inc(1);
                }
            });
        }
    });
    if let Some(e) = error.into_inner().unwrap() {
        return Err(e);
    }
    Ok(results
        .into_iter()
        .map(|r| r.into_inner().unwrap().unwrap())
        .collect())
}

/// Runs a command, failing with the end of its output if it fails.
pub fn run_checked(argv: &[String], dir: &Path, stdin: Option<&Path>) -> Result<Output> {
    let output = run(argv, dir, stdin)?;
    if !output.status.success() {
        bail!(
            "`{}` failed ({}){}",
            argv.join(" "),
            output.status,
            tail(&output, 20)
        );
    }
    Ok(output)
}

pub fn run(argv: &[String], dir: &Path, stdin: Option<&Path>) -> Result<Output> {
    let stdin = match stdin {
        Some(file) => Stdio::from(
            fs::File::open(file).with_context(|| format!("opening {}", file.display()))?,
        ),
        None => Stdio::null(),
    };
    Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .stdin(stdin)
        .output()
        .map_err(|e| anyhow!("running `{}`: {e}", argv[0]))
}

/// The last `n` lines of a command's output, indented, after a newline.
pub fn tail(output: &Output, n: usize) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..]
        .iter()
        .map(|l| format!("\n    {l}"))
        .collect()
}

/// The size of an executable's code, as `size` reports it.
fn text_size(bin: &Path) -> Option<u64> {
    let flag = if cfg!(target_os = "macos") {
        "-m"
    } else {
        "-A"
    };
    let output = Command::new("size").arg(flag).arg(bin).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = if cfg!(target_os = "macos") {
        text.lines().find(|l| l.contains("__text"))?
    } else {
        text.lines().find(|l| l.starts_with(".text "))?
    };
    // macOS: `Section __text: N`; Linux: `.text  N  addr`.
    let words: Vec<&str> = line.split_whitespace().collect();
    let word = if cfg!(target_os = "macos") {
        words.last()?
    } else {
        words.get(1)?
    };
    word.parse().ok()
}

fn path(p: &Path) -> String {
    p.display().to_string()
}

fn hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn stamp_matches(dir: &Path, stamp: &str) -> bool {
    fs::read_to_string(dir.join(".stamp")).is_ok_and(|s| s == stamp)
}

fn write_stamp(dir: &Path, stamp: &str) -> Result<()> {
    Ok(fs::write(dir.join(".stamp"), stamp)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_inputs_get_manifest_preprocessing_flags() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let suites = crate::manifest::load_suites(&root).unwrap();
        let build = &suites
            .iter()
            .find(|s| s.name == "yyjson")
            .unwrap()
            .manifest
            .build;
        let clang = input_flags(InputMode::Native, Compiler::Clang, build);
        assert!(clang.contains(&"-std=c11".into()));
        assert!(clang.contains(&"-Isrc".into()));
        let keel = input_flags(InputMode::Native, Compiler::Keel, build);
        assert!(keel.contains(&"-Isrc".into()));
        assert!(!keel.iter().any(|flag| flag.starts_with("-std=")));
        assert!(input_flags(InputMode::Shared, Compiler::Clang, build).is_empty());
        assert!(input_flags(InputMode::Shared, Compiler::Keel, build).is_empty());
    }
}
