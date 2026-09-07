//! Fence tests for the `rust-llm-tidy` CLI.
//!
//! The `fences` op flips nested backtick fences in comments, and the
//! `lints` TEXT005 check warns on untagged or bare-`ignore` fences.

use super::{run_command, temp_file_ext};
use std::fs;

/// `--include fences` reaches a code language: the nested fence inside
/// `#` comments flips its inner backtick delimiter to a tilde.
#[test]
fn include_fences_flips_the_nested_fence_in_hash_comments() {
    let source = "# ```text\n# ```rust\n# inner\n# ```\n# ```\n";
    let file = temp_file_ext("py");
    fs::write(&file, source).unwrap();
    let out = run_command(&["--include", "fences"], &file);
    assert!(
        out.status.success(),
        "--include fences on .py should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = fs::read_to_string(&file).unwrap();
    let _ = fs::remove_file(&file);
    assert!(
        after.contains("# ~~~rust"),
        "inner fence must flip under --include fences: {after}"
    );
}

// ── Language tiers ────────────────────────────────────────────────

/// TEXT005 end to end: the CLI warns once per untagged or bare-`ignore`
/// opening fence. Warnings keep the exit code 0.
///
/// `--exclude TEXT005` silences both findings.
#[test]
fn lints_warn_on_untagged_fences_and_exclude_silences_them() {
    let source = "\
intro

```
bare
```

```ignore
hidden
```
";
    // The opening fences sit on lines 3 and 7.
    let file = temp_file_ext("md");
    fs::write(&file, source).unwrap();

    let out = run_command(&["--include", "lints"], &file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "warning-severity lints must keep exit 0: {stderr}"
    );
    assert!(
        stderr.contains(":3: warning[TEXT005]: fenced code block has no language tag."),
        "expected bare-fence warning at line 3: {stderr}"
    );
    assert!(
        stderr.contains(":7: warning[TEXT005]: fenced code block uses bare `ignore`."),
        "expected bare-ignore warning at line 7: {stderr}"
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.contains("warning[TEXT005]"))
            .count(),
        2,
        "exactly two TEXT005 warnings expected: {stderr}"
    );
    let _ = fs::remove_file(&file);

    let file = temp_file_ext("md");
    fs::write(&file, source).unwrap();
    let out = run_command(&["--include", "lints", "--exclude", "TEXT005"], &file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "excluded TEXT005 run must succeed: {stderr}"
    );
    assert!(
        !stderr.contains("TEXT005"),
        "--exclude TEXT005 must silence the fence warnings: {stderr}"
    );
    let _ = fs::remove_file(&file);
}
