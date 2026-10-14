//! Downloading, preparing, preprocessing and building a suite.
//!
//! Every configuration compiles the same preprocessed files: `clang -E`
//! runs once per suite, so configurations differ only in the compiler.
//! clang builds are cached until the manifest, the host or the flags
//! change; keel builds never are, since keel is what is being tested.

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

pub struct Env {
    pub root: PathBuf,
    pub keel: PathBuf,
    pub jobs: usize,
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

/// A suite's sources, downloaded, prepared and preprocessed.
pub struct Sources {
    pub work: PathBuf,
    /// The preprocessed files.
    pub pre: Vec<PathBuf>,
    /// Changes when anything the preprocessed files depend on does.
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
    let stamp = hash(&[&suite.text, std::env::consts::OS, std::env::consts::ARCH]);

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
    let pre_dir = work.join("pre");
    let pre: Vec<PathBuf> = files
        .iter()
        .map(|f| pre_dir.join(f.file_stem().unwrap()).with_extension("i"))
        .collect();
    if !stamp_matches(&pre_dir, &stamp) || pre.iter().any(|p| !p.exists()) {
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
    if config.compiler == Compiler::Clang && stamp_matches(&out, &stamp) && bin.exists() {
        let stat = fs::read_to_string(&stat_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        if let Some(stat) = stat {
            return Ok(Built {
                bin,
                stat,
                cached: true,
            });
        }
    }

    let progress = bar(sources.pre.len(), format!("{} {}", suite.name, config.name));
    let start = Instant::now();
    let times = parallel(&sources.pre, env.jobs, &progress, |file| {
        compile(env, config, file, &out)
    });
    progress.finish_and_clear();
    let times = times.with_context(|| format!("building {} with {}", suite.name, config.name))?;
    let objects: Vec<String> = sources
        .pre
        .iter()
        .map(|f| path(&out.join(f.file_stem().unwrap()).with_extension("o")))
        .collect();
    let mut argv = vec!["clang".to_string()];
    argv.extend(objects);
    argv.extend(["-o".into(), path(&bin)]);
    argv.extend(suite.manifest.build.lib_flags());
    run_checked(&argv, &out, None).with_context(|| format!("linking {}", suite.name))?;

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
    };
    fs::write(&stat_file, serde_json::to_string(&stat)?)?;
    write_stamp(&out, &stamp)?;
    Ok(Built {
        bin,
        stat,
        cached: false,
    })
}

/// Compiles one preprocessed file to an object; gives the time of each
/// step (keel and the assembler, or clang and nothing).
fn compile(env: &Env, config: &Config, file: &Path, out: &Path) -> Result<(Duration, Duration)> {
    let stem = file.file_stem().unwrap();
    let object = path(&out.join(stem).with_extension("o"));
    let start = Instant::now();
    match config.compiler {
        Compiler::Clang => {
            let mut argv = vec!["clang".into(), "-w".into(), "-c".into()];
            argv.extend(config.flags.iter().cloned());
            argv.extend([path(file), "-o".into(), object]);
            run_checked(&argv, out, None)?;
            Ok((start.elapsed(), Duration::ZERO))
        }
        Compiler::Keel => {
            let asm = path(&out.join(stem).with_extension("s"));
            let mut argv = vec![path(&env.keel), "cc".into()];
            argv.extend(config.flags.iter().cloned());
            argv.extend([path(file), "-o".into(), asm.clone()]);
            run_checked(&argv, out, None)?;
            let keel = start.elapsed();
            let start = Instant::now();
            let argv = ["clang", "-c", &asm, "-o", &object].map(String::from);
            run_checked(&argv, out, None)?;
            Ok((keel, start.elapsed()))
        }
    }
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
