//! Language-selection tests for the `rust-llm-tidy` CLI: `--extension`
//! flags, markdown-family parity, and case-insensitive extensions.

use super::{run_command, strip_path_prefix, temp_file_ext};
use std::fs;

/// Malformed `--extension` values fail the run with a non-zero exit.
#[test]
fn extension_flag_rejects_malformed_values() {
    let file = temp_file_ext("rs");
    fs::write(&file, "fn a() {}\n").unwrap();
    for bad in [".rs", "a.md", "src/rs", ""] {
        let out = run_command(&["--extension", bad], &file);
        assert!(
            !out.status.success(),
            "--extension `{bad}` must fail the run"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("invalid extension"),
            "stderr should name the bad extension: {stderr}"
        );
    }
    let _ = fs::remove_file(&file);
}

/// Unknown extensions remain unchanged with or without explicit selection.
#[rstest::rstest]
#[case::unselected(&[])]
#[case::extension_selected(&["--extension", "org"])]
#[case::tables_selected(&["--extension", "org", "--include", "tables"])]
fn extension_flag_should_preserve_unknown_source(#[case] args: &[&str]) {
    let source = "| a | b |\n| --- | --- |\n| 1 | 22 |\n";
    let file = temp_file_ext("org");
    fs::write(&file, source).unwrap();

    let mut full_args = vec!["--json"];
    full_args.extend_from_slice(args);
    let out = run_command(&full_args, &file);

    assert!(
        out.status.success(),
        "unknown extension should be a successful no-op: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[]");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let _ = fs::remove_file(&file);
}

/// Markdown-family siblings behave exactly like `.md` on identical input.
///
/// Same fixed bytes, same stderr records and lint findings, same exit code.
/// Siblings: `.markdown`, `.txt`, `.text`, `.mdx`, and uppercase `.TXT`.
#[test]
fn markdown_family_siblings_match_md_behavior() {
    // Exercises every markdown-family op plus a text lint: a misaligned
    // table, a nested fence, a repeated inline link, and an over-limit
    // line.
    let source = "\
| Name | Value |
| --- | --- |
| a | 1 |
| longname | 200 |

```text
```rust
inner
```
```

see [A](http://example.com/long) and [A](http://example.com/long)

this line is deliberately made far longer than eighty characters so the line-length lint fires
";

    let md = temp_file_ext("md");
    fs::write(&md, source).unwrap();
    let md_out = run_command(&[], &md);
    let md_bytes = fs::read_to_string(&md).unwrap();
    let md_exit = md_out.status.code().unwrap_or(-1);
    let md_stderr = strip_path_prefix(&String::from_utf8_lossy(&md_out.stderr), &md);

    // The .md baseline itself must be non-trivial: fixes applied and the
    // line-length finding reported, or the parity check below proves
    // nothing.
    assert_eq!(md_exit, 0, "md baseline should succeed");
    assert!(
        md_stderr.contains("success[FIX]"),
        "md baseline: {md_stderr}"
    );
    assert!(
        md_stderr.contains("TEXT002"),
        "md baseline lints: {md_stderr}"
    );
    assert_ne!(md_bytes, source, "md baseline must change the file");

    for ext in ["markdown", "txt", "TXT", "text", "mdx"] {
        let file = temp_file_ext(ext);
        fs::write(&file, source).unwrap();
        let out = run_command(&[], &file);

        assert_eq!(
            out.status.code().unwrap_or(-1),
            md_exit,
            ".{ext} exit must match .md"
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            md_bytes,
            ".{ext} fixed bytes must match .md"
        );
        let stderr = strip_path_prefix(&String::from_utf8_lossy(&out.stderr), &file);
        assert_eq!(stderr, md_stderr, ".{ext} stderr must match .md");
        let _ = fs::remove_file(&file);
    }
    let _ = fs::remove_file(&md);
}

/// An explicit `Note.MD` file is allowed, runs markdown fix ops, and
/// never runs the Rust-only reorder op.
#[test]
fn uppercase_md_explicit_file_runs_fix_not_rust_ops() {
    let file = temp_file_ext("MD");
    fs::write(
        &file,
        "| Name | Value | Description |\n| --- | --- | --- |\n| a | 1 | first |\n| longname | 200 | second item |\n",
    )
    .unwrap();

    // Tables (a markdown fix op) run on the `.MD` file.
    let output = run_command(&["--include", "tables"], &file);
    assert!(
        output.status.success(),
        ".MD file should be allowed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        ".MD must run markdown fix ops: {stderr}"
    );

    // Reorder (a Rust-only op) never runs on a `.MD` file, even when the bytes
    // would reorder as Rust.
    let output = run_command(&["--include", "reorder", "--dry-run"], &file);
    assert!(
        output.status.success(),
        ".MD reorder dry-run should succeed without Rust ops: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("success[REORDER]"),
        ".MD must never run the Rust reorder op"
    );
    let _ = fs::remove_file(&file);
}

// ── Case-insensitive allowed extensions ───────

/// An explicit `Foo.RS` file is allowed and runs the Rust reorder op,
/// matching the lowercase `.rs` behavior.
#[test]
fn uppercase_rs_explicit_file_runs_reorder() {
    let file = temp_file_ext("RS");
    fs::write(&file, "fn a() {}\nfn b() { a(); }\n").unwrap();

    let output = run_command(&["--include", "reorder"], &file);
    let _ = fs::remove_file(&file);

    assert!(
        output.status.success(),
        ".RS file should be allowed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[REORDER]"),
        ".RS must run the Rust reorder op: {stderr}"
    );
}
