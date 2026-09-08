//! Isolated subprocess checks for explicit Cargo project-discovery permission.

use rust_llm_tidy::{RunOptions, config, run};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};

/// Act as a Cargo sentinel, isolated library caller, or parent test harness.
fn main() {
    if env::args().nth(1).as_deref() == Some("metadata") {
        fs::write(env::var_os("TIDY_DISCOVERY_SENTINEL").unwrap(), "invoked").unwrap();
        process::exit(1);
    }

    if env::var_os("TIDY_DISCOVERY_PROBE").is_some() {
        discovery_probe();
    } else {
        discovery_should_require_permission_and_enabled_visibility();
        println!("Cargo discovery permission scenarios passed");
    }
}

/// Exercise library discovery without mutating the parent test environment.
fn discovery_probe() {
    let Ok(mode) = env::var("TIDY_DISCOVERY_PROBE") else {
        return;
    };
    let source = PathBuf::from(env::var_os("TIDY_DISCOVERY_SOURCE").unwrap());
    let config_path = source.parent().unwrap().join("excluded.yml");
    let compiled =
        (mode == "config_excluded").then(|| config::load_and_compile(&config_path).unwrap());
    let excluded = mode == "excluded" || mode == "allowed_excluded";
    let options = RunOptions {
        paths: vec![source],
        lint_scope: Some(config::ReportingScope::All),
        cargo_discovery: mode.starts_with("allowed") || mode == "config_excluded",
        exclude: if excluded {
            vec!["vis".into()]
        } else {
            Vec::new()
        },
        ..RunOptions::default()
    };

    let report = run(&options, compiled.as_ref()).unwrap();

    if !report.warnings.is_empty() {
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("crate-aware vis unavailable"))
        );
    }
}

/// Run each permission/selection scenario with an isolated environment.
fn discovery_should_require_permission_and_enabled_visibility() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\n",
    )
    .unwrap();
    let source = directory.path().join("input.rs");
    fs::write(&source, "fn main() {}\n").unwrap();
    fs::write(
        directory.path().join("excluded.yml"),
        "exclude:\n  - rules: [vis]\n",
    )
    .unwrap();
    let sentinel = directory.path().join("cargo-sentinel");

    for (mode, invoked) in [
        ("default", false),
        ("excluded", false),
        ("allowed", true),
        ("allowed_excluded", false),
        ("config_excluded", false),
    ] {
        let output = Command::new(env::current_exe().unwrap())
            .env("TIDY_DISCOVERY_PROBE", mode)
            .env("TIDY_DISCOVERY_SOURCE", &source)
            .env("TIDY_DISCOVERY_SENTINEL", &sentinel)
            .env("CARGO", env::current_exe().unwrap())
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(sentinel.exists(), invoked, "{mode}");
        if invoked {
            fs::remove_file(&sentinel).unwrap();
        }
    }
}
