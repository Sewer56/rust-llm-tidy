//! TEXT006 verbose-synonym wording hints in markdown.
//!
//! The test writes a temp markdown file, runs the built CLI binary, and
//! asserts on the hint rendering and the `--include`/`--exclude` gating.

use crate::{run_command, temp_md};
use std::fs;

/// Words, phrases, and filler produce before/after hints without changing the file.
///
/// Code-span occurrences stay quiet, and `--include TEXT006` /
/// `--exclude TEXT006` gate the finding.
#[test]
fn md_should_show_wording_hints_when_text006_is_enabled() {
    // Arrange: include direct alternatives and context-sensitive guidance.
    let source = "# Title\n\nWe utilize this.\nUse `utilize` inside code spans.\n\
                  Due to the fact that it failed, retry.\n\
                  It is worth noting that this works.\n\
                  Use the canonical form.\n\
                  The tool is sophisticated.\n";
    let path = temp_md(source);
    let hints = [
        (3, "utilize", "`use`"),
        (5, "due to the fact that", "`because`"),
        (
            6,
            "it is worth noting that",
            "omit this opener; state the point directly",
        ),
        (
            7,
            "canonical",
            "`standard` or `usual`, depending on meaning; keep and explain precise technical uses such as `canonical form`",
        ),
        (
            8,
            "sophisticated",
            "`complex` or `advanced`, depending on meaning; use `hard` only for difficulty, or explain the relevant features",
        ),
    ];

    // Act: render all lint findings through the CLI.
    let output = run_command(&["--include", "lints"], &path);

    // Assert: hints preserve the full guidance and never rewrite the source.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT006 hints must not fail the run: {stderr}"
    );
    for (line, before, after) in hints {
        let expected = format!(
            ":{line}: hint[TEXT006]: wording has a simpler alternative: `{before}`.\n\
             Why: Unnecessary formal wording and framing can make the point harder to understand.\n\
             Suggestions:\n  \
             - Before: `{before}`\n  - After: {after}\n  \
             - Preserve meaning and adjust grammar to fit.\n  \
             - Use the alternative only if it preserves technical meaning, uncertainty, and required wording. (file)"
        );

        assert!(
            stderr.contains(&expected),
            "expected {expected:?}, got:\n{stderr}"
        );
    }
    assert_eq!(
        stderr.matches("TEXT006").count(),
        hints.len(),
        "the prose occurrences only, never the code span:\n{stderr}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);

    // Act: select TEXT006 alone.
    let included = run_command(&["--include", "TEXT006"], &path);

    // Assert: selecting only the rule preserves all its findings.
    let include_stderr = String::from_utf8_lossy(&included.stderr);
    assert!(included.status.success(), "{include_stderr}");
    assert_eq!(
        include_stderr.matches("TEXT006").count(),
        hints.len(),
        "--include TEXT006 must report the finding:\n{include_stderr}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);

    // Act: exclude TEXT006 from the lint run.
    let excluded = run_command(&["--include", "lints", "--exclude", "TEXT006"], &path);

    // Assert: exclusion hides the hints without modifying the document.
    let exclude_stderr = String::from_utf8_lossy(&excluded.stderr);
    assert!(
        excluded.status.success() && !exclude_stderr.contains("TEXT006"),
        "excluding TEXT006 must suppress the finding:\n{exclude_stderr}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}
