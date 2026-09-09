//! Check Git snapshot collection and conservative line remapping.
//!
//! Use private repositories with fixed Git identity, clock, and configuration
//! to exercise net changes, baselines, renames, and collection limits. Compare
//! exact eligible ranges after moves, edits, and duplicate-line changes.

use super::*;
use core::ops::RangeInclusive;
use rstest::rstest;
use std::path::{Path, PathBuf};
use std::process::Command;

#[rstest]
#[case::empty("", vec![])]
#[case::unterminated("one", vec![1..=1])]
#[case::trailing_newline("one\n", vec![1..=1])]
#[case::blank_line("one\n\n", vec![1..=2])]
fn all_should_include_only_existing_lines(
    #[case] source: &str,
    #[case] expected: Vec<RangeInclusive<usize>>,
) {
    assert_eq!(ChangedLines::all(source).ranges(), expected);
}

// Net Git changes and repository discovery.
#[rstest]
#[case::unchanged("one\ntwo\n", None, vec![])]
#[case::unstaged("one\nnew\n", None, vec![2..=2])]
#[case::staged("one\nnew\n", Some("one\nnew\n"), vec![2..=2])]
#[case::cancelled("one\ntwo\n", Some("one\nstaged\n"), vec![])]
#[case::mixed("working\ntwo\n", Some("one\nstaged\n"), vec![1..=1])]
#[case::deletion_only("one\n", None, vec![])]
#[case::empty("", None, vec![])]
fn collect_should_compare_head_to_current_input(
    #[case] current: &str,
    #[case] staged: Option<&str>,
    #[case] expected: Vec<RangeInclusive<usize>>,
) {
    let repo = repository(Some(("input.rs", "one\ntwo\n")));
    let path = repo.path().join("input.rs");
    if let Some(staged) = staged {
        std::fs::write(&path, staged).unwrap();
        git(repo.path(), &["add", "input.rs"]);
    }
    std::fs::write(&path, current).unwrap();

    let snapshot = snapshot(&path, None);

    assert_eq!(&*snapshot.source, current);
    assert_eq!(snapshot.changed.ranges(), expected);
}

#[test]
fn collect_should_discover_multiple_repositories_outside_cwd() {
    let first = repository(Some(("input.rs", "old\n")));
    let second = repository(Some(("input.rs", "unchanged\n")));
    let paths = [
        first.path().join("input.rs"),
        second.path().join("input.rs"),
    ];
    std::fs::write(&paths[0], "new\n").unwrap();

    let result = collect(&paths, None).unwrap();

    assert_eq!(
        result.snapshots[&paths[0]]
            .as_ref()
            .unwrap()
            .changed
            .ranges(),
        core::slice::from_ref(&(1..=1))
    );
    assert!(
        result.snapshots[&paths[1]]
            .as_ref()
            .unwrap()
            .changed
            .is_empty()
    );
}

#[rstest]
#[case::pure("one\ntwo\nthree\nfour\n", vec![])]
#[case::edited("one\ntwo\nthree\nnew\n", vec![4..=4])]
fn collect_should_follow_staged_renames(
    #[case] current: &str,
    #[case] expected: Vec<RangeInclusive<usize>>,
) {
    let repo = repository(Some(("old.rs", "one\ntwo\nthree\nfour\n")));
    git(repo.path(), &["mv", "old.rs", "新\nname.rs"]);
    let path = repo.path().join("新\nname.rs");
    std::fs::write(&path, current).unwrap();

    let snapshot = snapshot(&path, None);

    assert_eq!(snapshot.changed.ranges(), expected);
}

#[test]
fn collect_should_ignore_external_diff_and_text_conversion() {
    let repo = repository(Some(("input.rs", "source\n")));
    let path = repo.path().join("input.rs");
    std::fs::write(repo.path().join(".gitattributes"), "*.rs diff=fixture\n").unwrap();
    git(
        repo.path(),
        &["config", "diff.external", "nonexistent-forbidden-diff"],
    );
    git(
        repo.path(),
        &[
            "config",
            "diff.fixture.textconv",
            "nonexistent-forbidden-textconv",
        ],
    );
    git(
        repo.path(),
        &["config", "core.fsmonitor", "nonexistent-forbidden-monitor"],
    );
    std::fs::write(&path, "changed\n").unwrap();

    let snapshot = snapshot(&path, None);

    assert_eq!(snapshot.changed, ChangedLines::all("changed\n"));
}

#[rstest]
#[case::untracked("new.rs", false)]
#[case::staged("new.rs", true)]
#[case::unicode("新しい файл.rs", false)]
#[case::newline("line\nbreak.rs", true)]
#[case::literal_pathspec(":(glob)*.rs", true)]
fn collect_should_include_all_lines_of_new_files(#[case] name: &str, #[case] staged: bool) {
    let repo = repository(Some(("initial.rs", "initial\n")));
    let path = repo.path().join(name);
    std::fs::write(&path, "new\nlines\n").unwrap();
    if staged {
        git(repo.path(), &["--literal-pathspecs", "add", "--", name]);
    }

    let snapshot = snapshot(&path, None);

    assert_eq!(snapshot.changed, ChangedLines::all("new\nlines\n"));
}

#[test]
fn collect_should_leave_deleted_files_without_anchors() {
    let repo = repository(Some(("input.rs", "one\ntwo\n")));
    let path = repo.path().join("input.rs");
    std::fs::remove_file(&path).unwrap();

    let snapshot = snapshot(&path, None);

    assert!(snapshot.source.is_empty());
    assert!(snapshot.changed.is_empty());
}

#[test]
fn collect_should_preserve_duplicate_path_spellings() {
    let repo = repository(Some(("input.rs", "source\n")));
    let path = repo.path().join("input.rs");
    let alias = repo.path().join(".").join("input.rs");
    let paths = [path.clone(), alias.clone(), path.clone()];

    let result = collect(&paths, None).unwrap();

    assert!(result.snapshots[&path].as_ref().unwrap().changed.is_empty());
    assert!(
        result.snapshots[&alias]
            .as_ref()
            .unwrap()
            .changed
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn collect_should_preserve_non_utf8_git_paths() {
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;

    let repo = repository(Some(("initial.rs", "initial\n")));
    let path = repo
        .path()
        .join(std::ffi::OsStr::from_bytes(b"input-\xff.rs"));
    std::fs::write(&path, "source\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "nonutf8"]);
    std::fs::write(&path, "changed\n").unwrap();

    let snapshot = snapshot(&path, None);

    assert_eq!(snapshot.changed, ChangedLines::all("changed\n"));
}

#[test]
fn collect_should_read_linked_worktree_inputs() {
    let repo = repository(Some(("input.rs", "source\n")));
    let linked = tempfile::tempdir().unwrap();
    let directory = linked.path().join("linked");
    git(
        repo.path(),
        &["worktree", "add", "--detach", directory.to_str().unwrap()],
    );
    let path = directory.join("input.rs");
    std::fs::write(&path, "changed\n").unwrap();

    let snapshot = snapshot(&path, None);

    assert_eq!(snapshot.changed, ChangedLines::all("changed\n"));
}

#[rstest]
#[case::unknown("not-a-ref")]
#[case::option("--all")]
fn collect_should_reject_bad_explicit_baselines(#[case] baseline: &str) {
    let repo = repository(Some(("input.rs", "source\n")));

    let result = collect(&[repo.path().join("input.rs")], Some(baseline));

    assert!(result.is_err());
}

#[rstest]
#[case::standalone(false)]
#[case::unborn(true)]
fn collect_should_reject_explicit_baseline_without_head(#[case] initialized: bool) {
    let directory = if initialized {
        repository(None)
    } else {
        tempfile::tempdir().unwrap()
    };
    let path = directory.path().join("input.rs");
    std::fs::write(&path, "source\n").unwrap();

    let result = collect(&[path], Some("HEAD"));

    assert!(result.is_err());
}

#[test]
fn collect_should_reject_input_count_before_discovery() {
    let paths = vec![PathBuf::from("missing.rs"); MAX_INPUT_PATHS + 1];

    let error = collect(&paths, None).unwrap_err();

    assert!(error.to_string().contains("input list exceeds"));
}

#[test]
fn collect_should_reject_non_utf8_source() {
    let repo = repository(Some(("input.rs", "source\n")));
    let path = repo.path().join("input.rs");
    std::fs::write(&path, [0xff]).unwrap();

    let result = collect(&[path], None);

    assert!(result.is_err());
}

#[test]
fn collect_should_reject_oversized_source_before_reading() {
    let repo = repository(Some(("input.rs", "source\n")));
    let path = repo.path().join("input.rs");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_SOURCE_BYTES as u64 + 1)
        .unwrap();

    let result = collect(&[path], None);

    assert!(result.is_err());
}

#[cfg(unix)]
#[test]
fn collect_should_reject_symlink_sources() {
    let repo = repository(Some(("input.rs", "source\n")));
    let path = repo.path().join("link.rs");
    std::os::unix::fs::symlink("input.rs", &path).unwrap();

    let result = collect(&[path], None);

    assert!(result.is_err());
}

#[test]
fn collect_should_reject_unrelated_explicit_history() {
    let repo = repository(Some(("input.rs", "source\n")));
    git(repo.path(), &["checkout", "--orphan", "other"]);
    git(repo.path(), &["commit", "-m", "unrelated"]);

    let result = collect(&[repo.path().join("input.rs")], Some("main"));

    assert!(result.is_err());
}

#[test]
fn collect_should_use_merge_base_instead_of_reference_tip() {
    let repo = repository(Some(("input.rs", "one\ntwo\n")));
    let path = repo.path().join("input.rs");
    git(repo.path(), &["checkout", "-b", "other"]);
    std::fs::write(&path, "other\ntwo\n").unwrap();
    git(repo.path(), &["commit", "-am", "other"]);
    git(repo.path(), &["checkout", "main"]);
    std::fs::write(&path, "one\nmain\n").unwrap();
    git(repo.path(), &["commit", "-am", "main"]);

    let snapshot = snapshot(&path, Some("other"));

    assert_eq!(snapshot.changed.ranges(), core::slice::from_ref(&(2..=2)));
}

#[rstest]
#[case::standalone(false)]
#[case::unborn(true)]
fn collect_should_warn_without_implicit_baseline(#[case] initialized: bool) {
    let directory = if initialized {
        repository(None)
    } else {
        tempfile::tempdir().unwrap()
    };
    let path = directory.path().join("input.rs");
    std::fs::write(&path, "source\n").unwrap();

    let result = collect(core::slice::from_ref(&path), None).unwrap();

    assert!(result.snapshots[&path].is_none());
    assert_eq!(result.warnings.len(), 1);
}

#[rstest]
#[case::intersection(3, 7, true)]
#[case::boundary(4, 4, true)]
#[case::gap(5, 7, false)]
#[case::zero(0, 4, false)]
#[case::reversed(8, 2, false)]
fn overlaps_should_match_inclusive_spans(
    #[case] start: usize,
    #[case] end: usize,
    #[case] expected: bool,
) {
    let ranges = ChangedLines::new([2..=4, 8..=10]);

    assert_eq!(ranges.overlaps(start, end), expected);
}

// Construction and overlap.
#[test]
fn ranges_should_merge_sorted_adjacent_spans() {
    let ranges = ChangedLines::new([8..=10, 3..=4, 1..=2, 4..=6, 0..=9, 10..=usize::MAX]);

    assert_eq!(ranges.ranges(), &[1..=6, 8..=usize::MAX]);
}

// Conservative transform mapping.
#[rstest]
#[case::unchanged("old\nnew\n", "old\nnew\n", vec![2..=2], vec![2..=2])]
#[case::moved("old\nnew\n", "new\nold\n", vec![2..=2], vec![1..=1])]
#[case::edited("old\nnew\n", "old\nnewer\n", vec![2..=2], vec![])]
#[case::inserted("new\n", "tool\nnew\n", vec![1..=1], vec![2..=2])]
#[case::mixed_duplicates("same\nsame\n", "same\n", vec![2..=2], vec![])]
#[case::eligible_duplicates("same\nsame\n", "same\n", vec![1..=2], vec![1..=1])]
#[case::extra_copies("new\n", "new\nnew\n", vec![1..=1], vec![])]
#[case::deleted("new\n", "", vec![1..=1], vec![])]
#[case::line_endings("new\r\n", "new\n", vec![1..=1], vec![])]
fn remap_should_preserve_only_unambiguous_input_text(
    #[case] source: &str,
    #[case] transformed: &str,
    #[case] changed: Vec<RangeInclusive<usize>>,
    #[case] expected: Vec<RangeInclusive<usize>>,
) {
    let snapshot = ChangedLineSnapshot {
        source: source.into(),
        changed: ChangedLines::new(changed),
    };

    let actual = snapshot.remap(transformed);

    assert_eq!(actual.ranges(), expected);
}

#[test]
fn remap_should_reject_output_over_line_limit() {
    let snapshot = ChangedLineSnapshot {
        source: "new\n".into(),
        changed: ChangedLines::all("new\n"),
    };
    let output = "\n".repeat(MAX_SOURCE_LINES + 1);

    assert!(snapshot.remap(&output).is_empty());
}

#[test]
fn snapshots_should_remain_stable_after_file_mutation() {
    let repo = repository(Some(("input.rs", "old\n")));
    let path = repo.path().join("input.rs");
    std::fs::write(&path, "new\n").unwrap();
    let snapshot = snapshot(&path, None);

    std::fs::write(&path, "tool\n").unwrap();

    assert_eq!(&*snapshot.source, "new\n");
    assert!(snapshot.remap("tool\n").is_empty());
}

/// Build a private repository with an optional initial tracked input.
fn repository(initial: Option<(&str, &str)>) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--initial-branch=main"]);
    git(directory.path(), &["config", "commit.gpgsign", "false"]);
    if let Some((name, source)) = initial {
        std::fs::write(directory.path().join(name), source).unwrap();
        git(directory.path(), &["add", "--", name]);
        git(directory.path(), &["commit", "-m", "initial"]);
    }

    directory
}

/// Extract one successful snapshot while checking that collection had no fallback.
fn snapshot(path: &Path, baseline: Option<&str>) -> ChangedLineSnapshot {
    let mut result = collect(&[path.to_path_buf()], baseline).unwrap();

    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    result.snapshots.remove(path).unwrap().unwrap()
}

/// Run fixture Git with fixed identity, clock, locale, and private configuration.
fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .env("LC_ALL", "C")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
