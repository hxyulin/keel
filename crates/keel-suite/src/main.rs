//! keel-suite: builds real programs with keel and clang, runs their test
//! suites, and times them. Each program is a directory under `suite/c/`
//! with a `suite.toml`; `suite/README.md` describes the format.
//!
//! ```text
//! cargo suite list
//! cargo suite test lua sqlite        # keel and keel -O, against the tests
//! cargo suite run lua/fib -n 10      # every configuration, timed
//! cargo suite compare aarch64-apple-darwin x86_64-unknown-linux-gnu
//! ```

mod build;
mod checks;
mod frontends;
mod manifest;
mod report;
mod run;
mod style;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use manifest::{Configs, Suite};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};
use style::{Paint, paint};

#[derive(Parser)]
#[command(
    name = "keel-suite",
    about = "Test and benchmark keel on real programs"
)]
struct Cli {
    /// When to color the output.
    #[arg(long, value_enum, default_value_t = Color::Auto, global = true)]
    color: Color,
    /// Parallel compile jobs (default: two for frontend checks, one per CPU otherwise).
    #[arg(short, long, global = true)]
    jobs: Option<usize>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand)]
enum Command {
    /// List the suites, their benchmarks and the configurations.
    List,
    /// Test suite runners by component, without application builds or timings.
    Check {
        /// Components to check (default: all).
        #[arg(value_enum)]
        groups: Vec<checks::Group>,
        /// Python interpreter for the runner tests.
        #[arg(long, default_value = "python3")]
        python: PathBuf,
    },
    /// Check C frontend compatibility and LLVM-imported C++/Rust correctness.
    TestFrontends(frontends::Options),
    /// Build suites with keel and run their test suites.
    Test {
        /// Suites to test (default: all).
        suites: Vec<String>,
        /// Configurations to build, comma-separated (default: the `test`
        /// list in suite/configs.toml).
        #[arg(short, long, value_delimiter = ',')]
        config: Vec<String>,
        /// Use a release build of keel (default: debug, with its assertions).
        #[arg(long)]
        release: bool,
    },
    /// Build every configuration, check they agree, and time the benchmarks.
    Run {
        /// `suite` or `suite/benchmark` (default: all).
        filter: Vec<String>,
        /// Configurations to build, comma-separated (default: all).
        #[arg(short, long, value_delimiter = ',')]
        config: Vec<String>,
        /// Timed runs of each benchmark in each configuration.
        #[arg(short = 'n', long, default_value_t = 5)]
        runs: u32,
        /// Untimed runs first.
        #[arg(short, long, default_value_t = 1)]
        warmup: u32,
        /// Do not save the results.
        #[arg(long)]
        no_save: bool,
    },
    /// Show a saved run: a results file, or a target triple for its latest.
    Show {
        /// Default: this host's target.
        run: Option<String>,
    },
    /// Compare two saved runs (results files or target triples).
    Compare { a: String, b: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    style::set_color(match cli.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    });
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{} {e:#}", paint("error:", Paint::Red));
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root")
}

fn target() -> &'static str {
    keel_ir::Target::default().triple()
}

fn run(cli: Cli) -> Result<bool> {
    let root = root();
    let jobs = cli
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    match cli.command {
        Command::Check { groups, python } => checks::run(&root, &groups, &python),
        Command::TestFrontends(options) => frontends::run(&root, options, cli.jobs.unwrap_or(2)),
        Command::List => {
            list(&root)?;
            Ok(true)
        }
        Command::Test {
            suites,
            config,
            release,
        } => {
            let configs = Configs::load(&root)?;
            let selected = configs.select(&config, &configs.test)?;
            let all = manifest::load_suites(&root)?;
            let chosen = choose(&all, &suites)?;
            let env = build::Env {
                keel: build::build_keel(&root, release)?,
                root,
                jobs,
            };
            let passed = run::test(&env, &chosen, &selected, &configs)?;
            println!();
            if passed {
                println!("{} all tests passed", style::ok_mark());
            } else {
                println!("{} some tests failed", style::fail_mark());
            }
            Ok(passed)
        }
        Command::Run {
            filter,
            config,
            runs,
            warmup,
            no_save,
        } => {
            let configs = Configs::load(&root)?;
            let all_names: Vec<String> = configs.config.iter().map(|c| c.name.clone()).collect();
            let selected = configs.select(&config, &all_names)?;
            let all = manifest::load_suites(&root)?;
            let chosen = choose_benches(&all, &filter)?;
            let env = build::Env {
                keel: build::build_keel(&root, true)?,
                root: root.clone(),
                jobs,
            };
            let mut results = run::Results {
                target: target().into(),
                host: output(&root, &["uname", "-n"]),
                keel: commit(&root),
                date: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                baseline: configs.baseline.clone(),
                configs: selected.iter().map(|c| c.name.clone()).collect(),
                builds: Vec::new(),
                benches: Vec::new(),
            };
            let timing = run::Timing { runs, warmup };
            run::bench(
                &env,
                &chosen,
                &selected,
                &configs.baseline,
                &timing,
                &mut results,
            )?;
            if !no_save {
                let file = report::save(&root, &results)?;
                let shown = file.strip_prefix(&root).unwrap_or(&file);
                println!(
                    "{}",
                    paint(format!("saved {}", shown.display()), Paint::Dim)
                );
            }
            Ok(results.benches.iter().all(|b| b.error.is_none()))
        }
        Command::Show { run } => {
            report::show(&report::load(&root, run.as_deref().unwrap_or(target()))?);
            Ok(true)
        }
        Command::Compare { a, b } => {
            report::compare(&report::load(&root, &a)?, &report::load(&root, &b)?);
            Ok(true)
        }
    }
}

fn list(root: &Path) -> Result<()> {
    let configs = Configs::load(root)?;
    for suite in manifest::load_suites(root)? {
        run::heading(&suite);
        let m = &suite.manifest;
        if let Some(test) = &m.test {
            let what = match &test.compare {
                Some(name) => format!("{} compared with {name}", test.run.len()),
                None => test.run.len().to_string(),
            };
            println!("  {} {what}", paint("tests     ", Paint::Dim));
        }
        if !m.bench.is_empty() {
            let names: Vec<&str> = m.bench.iter().map(|b| b.name.as_str()).collect();
            println!("  {} {}", paint("benchmarks", Paint::Dim), names.join(", "));
        }
    }
    println!();
    println!("{}", paint("configurations", Paint::Heading));
    for c in &configs.config {
        let compiler = match c.compiler {
            manifest::Compiler::Clang => "clang",
            manifest::Compiler::Keel => "keel cc",
        };
        let mut notes = Vec::new();
        if c.name == configs.baseline {
            notes.push("baseline");
        }
        if configs.test.contains(&c.name) {
            notes.push("tested");
        }
        println!(
            "  {:<12} {} {}",
            c.name,
            paint(format!("{compiler} {}", c.flags.join(" ")), Paint::Dim),
            paint(notes.join(", "), Paint::Cyan)
        );
    }
    println!(
        "\n{}",
        paint(format!("this host: {}", target()), Paint::Dim)
    );
    Ok(())
}

/// The suites named, or all.
fn choose<'a>(all: &'a [Suite], names: &[String]) -> Result<Vec<&'a Suite>> {
    if names.is_empty() {
        return Ok(all.iter().collect());
    }
    names.iter().map(|n| find_suite(all, n)).collect()
}

fn find_suite<'a>(all: &'a [Suite], name: &str) -> Result<&'a Suite> {
    all.iter().find(|s| s.name == name).ok_or_else(|| {
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        anyhow::anyhow!("no suite `{name}` (there are: {})", names.join(", "))
    })
}

/// The benchmarks `suite` and `suite/bench` filters select, by suite.
fn choose_benches<'a>(
    all: &'a [Suite],
    filters: &[String],
) -> Result<Vec<(&'a Suite, Vec<&'a manifest::Bench>)>> {
    let mut chosen: Vec<(&Suite, Vec<&manifest::Bench>)> = Vec::new();
    let everything = [String::new()];
    let filters = if filters.is_empty() {
        &everything[..]
    } else {
        filters
    };
    for filter in filters {
        let (suite_name, bench_name) = match filter.split_once('/') {
            Some((s, b)) => (s, Some(b)),
            None => (filter.as_str(), None),
        };
        let suites: Vec<&Suite> = if suite_name.is_empty() {
            all.iter().collect()
        } else {
            vec![find_suite(all, suite_name)?]
        };
        for suite in suites {
            let benches: Vec<&manifest::Bench> = suite
                .manifest
                .bench
                .iter()
                .filter(|b| bench_name.is_none_or(|n| b.name == n))
                .collect();
            if let Some(name) = bench_name.filter(|_| benches.is_empty()) {
                bail!("suite `{}` has no benchmark `{name}`", suite.name);
            }
            match chosen.iter_mut().find(|(s, _)| s.name == suite.name) {
                Some((_, list)) => {
                    for b in benches {
                        if !list.iter().any(|x| x.name == b.name) {
                            list.push(b);
                        }
                    }
                }
                None if !benches.is_empty() => chosen.push((suite, benches)),
                None => {}
            }
        }
    }
    Ok(chosen)
}

fn output(dir: &Path, argv: &[&str]) -> String {
    Process::new(argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The checked-out commit, with `-dirty` if there are changes.
fn commit(root: &Path) -> String {
    let hash = output(root, &["git", "rev-parse", "--short", "HEAD"]);
    let dirty = !output(root, &["git", "status", "--porcelain"]).is_empty();
    if dirty { format!("{hash}-dirty") } else { hash }
}
