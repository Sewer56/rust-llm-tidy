//! DUP001 CLI acceptance with isolated Git baselines and unchanged source bytes.

use super::{cleanup, git, init_repo, run};
use rstest::rstest;
use rust_llm_tidy::config::DuplicationConfig;
use std::fs;

const BLOCK: &str = "load();\nclassify();\nrecord();\nflush();\nfinish();\n";

#[test]
fn duplication_should_compare_an_explicit_committed_baseline() {
    let repo = init_repo().expect("Git is required for DUP001 acceptance");
    let config = DuplicationConfig::default();
    let path = repo.join("input.js");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(&path, BLOCK.repeat(config.min_occurrences - 1)).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(&path, BLOCK.repeat(config.min_occurrences)).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "addition"]);

    let output = run(
        &repo,
        &["--diff-base", "HEAD~1", "--include", "DUP001", "--json"],
    );
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0]["line"],
        BLOCK.lines().count() * (config.min_occurrences - 1) + 1
    );
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
    cleanup(&repo);
}

#[rstest]
#[case::explicit_untracked(false, false, 1)]
#[case::explicit_staged(true, false, 1)]
#[case::implicit_untracked(false, true, 1)]
#[case::implicit_staged(true, true, 1)]
fn duplication_should_preserve_new_file_discovery(
    #[case] staged: bool,
    #[case] implicit: bool,
    #[case] count: usize,
) {
    let repo = init_repo().expect("Git is required for DUP001 acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(
        repo.join("input.js"),
        BLOCK.repeat(DuplicationConfig::default().min_occurrences),
    )
    .unwrap();
    if staged {
        git(&repo, &["add", "input.js"]);
    }
    let mut args = vec!["--include", "DUP001", "--json"];
    if !implicit {
        args.push("input.js");
    }

    let output = run(&repo, &args);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(findings.len(), count, "{findings:?}");
    cleanup(&repo);
}

#[rstest]
#[case::added_copy("addition", false, 1)]
#[case::all_changed("new", false, 1)]
#[case::historical_edit("one_line", false, 0)]
#[case::separated_short_runs("short_runs", false, 0)]
#[case::deletion_only("deletion", false, 0)]
#[case::empty_diff("unchanged", false, 0)]
#[case::cancelled_index("cancelled", false, 0)]
#[case::all_lines("unchanged", true, 1)]
fn duplication_should_query_only_net_changed_runs(
    #[case] scenario: &str,
    #[case] all_lines: bool,
    #[case] count: usize,
) {
    let repo = init_repo().expect("Git is required for DUP001 acceptance");
    let path = repo.join("input.js");
    let config = DuplicationConfig::default();
    let repeated = BLOCK.repeat(config.min_occurrences);
    let baseline = match scenario {
        "addition" => BLOCK.repeat(config.min_occurrences - 1),
        "new" => "baseline();\n".into(),
        "deletion" => format!("{repeated}remove();\n"),
        "short_runs" => format!(
            "{}old_a();\nold_b();\nrecord();\nold_d();\nold_e();\n",
            BLOCK.repeat(config.min_occurrences - 1)
        ),
        _ => repeated.clone(),
    };
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(&path, &baseline).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    if scenario == "cancelled" {
        fs::write(&path, format!("{repeated}{BLOCK}")).unwrap();
        git(&repo, &["add", "input.js"]);
    }
    let current = if scenario == "one_line" {
        repeated.replacen("classify();", "classify_new();", 1)
    } else {
        repeated
    };
    fs::write(&path, &current).unwrap();
    let mut args = vec!["input.js", "--include", "DUP001", "--json"];
    if all_lines {
        args.push("--all-lines");
    }

    let output = run(&repo, &args);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(findings.len(), count, "{findings:?}");
    if scenario == "addition" {
        assert_eq!(
            findings[0]["line"],
            BLOCK.lines().count() * (config.min_occurrences - 1) + 1
        );
        assert_eq!(findings[0]["severity"], "reminder");
        assert!(
            findings[0]["message"]
                .as_str()
                .unwrap()
                .contains("Locations: 1-5, 6-10, 11-15.")
        );
    }
    assert_eq!(fs::read_to_string(path).unwrap(), current);
    cleanup(&repo);
}

#[test]
fn duplication_should_respect_ignored_inputs() {
    let repo = init_repo().expect("Git is required for DUP001 acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(repo.join(".gitignore"), "ignored.js\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(
        repo.join("ignored.js"),
        BLOCK.repeat(DuplicationConfig::default().min_occurrences),
    )
    .unwrap();

    let output = run(
        &repo,
        &[".", "--all-lines", "--include", "DUP001", "--json"],
    );
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(findings.is_empty(), "{findings:?}");
    cleanup(&repo);
}
