//! The skip contract, checked by running this binary's ignored `skipping` test
//! in a child process with different environments.

use std::process::Command;

use keel_test_support::{TempDir, skip};

#[test]
#[ignore = "run by skip_logs_and_require_fails"]
fn skipping() {
    skip!("tool {} missing", "example");
}

fn child() -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "skipping",
            "--test-threads",
            "2",
            "--nocapture",
        ])
        .env_remove("KEEL_SKIP_LOG")
        .env_remove("KEEL_REQUIRE_ORACLES");
    command
}

#[test]
fn skip_logs_and_require_fails() {
    let dir = TempDir::new("skip-log");
    let log = dir.join("skips.tsv");
    let run = child().env("KEEL_SKIP_LOG", &log).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("KEEL-SKIP: tool example missing"));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "keel-test-support::skip::skipping\ttool example missing\n"
    );

    let run = child()
        .env("KEEL_SKIP_LOG", &log)
        .env("KEEL_REQUIRE_ORACLES", "1")
        .output()
        .unwrap();
    assert!(!run.status.success(), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("KEEL_REQUIRE_ORACLES=1"));
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
}
