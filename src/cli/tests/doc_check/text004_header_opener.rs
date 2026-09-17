//! TEXT004 opener warnings and separate-line reference measurements.
//!
//! Tests run the built CLI on temporary Markdown and Rust inputs, then
//! assert on its exit code and stderr diagnostics.

use crate::{run_command, temp_md, temp_named_file};
use rstest::rstest;

/// Separate reference lines change only the opener's character measurement.
#[rstest]
#[case::markdown("", "md", 160, false)]
#[case::markdown_over("", "md", 161, true)]
#[case::rust_comment("// ", "rs", 160, false)]
#[case::rust_comment_over("// ", "rs", 161, true)]
#[case::rust_item_doc("/// ", "rs", 160, false)]
#[case::rust_item_doc_over("/// ", "rs", 161, true)]
#[case::rust_module_doc("//! ", "rs", 160, false)]
#[case::rust_module_doc_over("//! ", "rs", 161, true)]
fn cli_should_measure_retained_opener_text(
    #[case] prefix: &str,
    #[case] ext: &str,
    #[case] size: usize,
    #[case] warns: bool,
) {
    // Arrange
    let mut source = format!(
        "{prefix}See <https://example.test/{}>\n{prefix}{}\n",
        "x".repeat(180),
        "é".repeat(size),
    );
    if ext == "rs" {
        source.push_str("fn main() {}\n");
    }
    let path = temp_named_file(&format!("opener.{ext}"), &source);

    // Act
    let output = run_command(&["--include", "TEXT004"], &path);

    // Assert
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr.matches("warning[TEXT004]").count(),
        usize::from(warns),
        "{stderr}"
    );
    if warns {
        assert!(stderr.contains(":1: warning[TEXT004]: opener paragraph is 161 chars long;"));
        assert!(stderr.contains(
            "Put links on a separate line. A URL alone, or one word followed by a URL, \
                                 does not count toward the opener's character limit."
        ));
    }
}

/// A three-sentence markdown heading opener warns with TEXT004 at the
/// paragraph's first line.
///
/// The one-sentence heading opener in the same file stays silent, and
/// warnings keep the exit code at 0.
#[test]
fn md_three_sentence_heading_opener_warns_text004_without_failing() {
    let opener = "Apples grow on trees. They are quite tasty. Pick them in fall.";
    let path = temp_md(&format!(
        "# Fruits\n\n{opener}\n\n# Notes\n\nOne sentence about notes.\n"
    ));
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT004 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":3: warning[TEXT004]"),
        "expected a TEXT004 warning at the heading opener's first line, got:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT004").count(),
        1,
        "exactly the three-sentence opener, never the one-sentence one:\n{stderr}"
    );
}
