//! Link hoisting through the `fix` subcommand.
//!
//! Covers CRLF preservation, per-comment intra-doc hoisting, threshold
//! tuning, and the indexed-call repro that must stay unchanged. The shared
//! runner helpers live in `mod.rs`.

use super::common::binary;
use super::{fixture_dir, run_command, temp_dir, temp_file};
use std::fs;
use std::process::Command;

/// Byte-exact output of the fix on [`INTRA_DOC_REPRO_SOURCE`]: every link is
/// hoisted.
///
/// A `[text]: url` definition is duplicated inside each comment that uses it,
/// never at EOF, with a blank comment line before the definitions.
const INTRA_DOC_REPRO_FIXED: &str = "\
/// Assembles the final value by driving [the Builder].
///
/// [the Builder]: crate::Builder
pub struct Builder;

impl Builder {
    /// Produces [the Config] and hands it to
    /// [the Builder].
    ///
    /// [the Config]: crate::Config
    /// [the Builder]: crate::Builder
    pub fn build(&self) -> Config {
        Config
    }

    /// Resets the builder before [the build].
    ///
    /// [the build]: Self::build
    pub fn reset(&mut self) {}
}

/// The assembled value; see [the Builder].
///
/// [the Builder]: crate::Builder
pub struct Config;
";
/// Reported multi-comment intra-doc repro with resolvable targets.
///
/// Uses `Self::`/`crate::`-style links across several doc comments, so it is
/// doc-build clean both before and after the fix.
const INTRA_DOC_REPRO_SOURCE: &str = "\
/// Assembles the final value by driving [the Builder](crate::Builder).
pub struct Builder;

impl Builder {
    /// Produces [the Config](crate::Config) and hands it to
    /// [the Builder](crate::Builder).
    pub fn build(&self) -> Config {
        Config
    }

    /// Resets the builder before [the build](Self::build).
    pub fn reset(&mut self) {}
}

/// The assembled value; see [the Builder](crate::Builder).
pub struct Config;
";

/// In-place `fix` on a CRLF markdown file with a repeated inline link
/// preserves `\r\n` in the hoisted `[text]: url` definition.
///
/// CRLF input is built in-memory (committed fixtures would be git-normalized
/// on checkout).
#[test]
fn fix_links_in_place_preserves_crlf() {
    let tmp = temp_file("md");
    let input = "see [A](http://x) and [A](http://x)\r\n";
    fs::write(&tmp, input).unwrap();

    let output = run_command(&["--include", "links"], &tmp);
    assert!(
        output.status.success(),
        "fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);

    // The hoisted definition must be present and use `\r\n`.
    assert!(
        actual.contains("[A]: http://x"),
        "definition hoisted: {actual:?}"
    );
    assert!(
        actual.contains("[A]: http://x\r\n"),
        "hoisted definition must end with CRLF: {actual:?}"
    );
    // No bare LF: every `\n` is part of `\r\n`.
    assert_eq!(
        actual.matches('\n').count(),
        actual.matches("\r\n").count(),
        "every newline must be CRLF after fix: {actual:?}"
    );
}

/// `fix --include links --dry-run` over a `.rs` file with intra-doc links in
/// several doc comments leaves the file untouched.
///
/// It reports one link record per hoisted pair on stderr.
#[test]
fn fix_links_rs_dry_run_reports_intra_doc_records() {
    let tmp = temp_file("rs");
    fs::write(&tmp, INTRA_DOC_REPRO_SOURCE).unwrap();

    let output = run_command(&["--include", "links", "--dry-run"], &tmp);
    let _ = fs::remove_file(&tmp);
    assert!(
        output.status.success(),
        "fix --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        3,
        "one record per hoisted pair: {stderr}"
    );
    assert!(
        stderr.contains("`[the Builder](crate::Builder)` -> `[the Builder]`"),
        "Builder hoist reported: {stderr}"
    );
    assert!(
        stderr.contains("`[the Config](crate::Config)` -> `[the Config]`"),
        "Config hoist reported: {stderr}"
    );
    assert!(
        stderr.contains("`[the build](Self::build)` -> `[the build]`"),
        "Self:: build hoist reported: {stderr}"
    );
}

/// In-place `fix --include links` on the intra-doc repro produces the
/// byte-exact per-comment definitions.
///
/// No definition is emitted at EOF or on a non-doc-comment line.
#[test]
fn fix_links_rs_in_place_produces_per_comment_defs() {
    let tmp = temp_file("rs");
    fs::write(&tmp, INTRA_DOC_REPRO_SOURCE).unwrap();

    let output = run_command(&["--include", "links"], &tmp);
    assert!(
        output.status.success(),
        "fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, INTRA_DOC_REPRO_FIXED,
        "in-place output must match the per-comment reference form"
    );
}

/// A scratch crate embedding the intra-doc repro is doc-build clean after the
/// fix.
///
/// It passes `cargo doc --document-private-items` with
/// `RUSTDOCFLAGS="-D warnings"`, proving the per-comment rewritten output.
#[test]
fn fix_links_rs_output_is_doc_build_clean() {
    let dir = temp_dir();
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"intradoc_repro\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    // Embed the fixed (post-op) output so the scratch crate documents the exact
    // bytes the fix produces.
    fs::write(src.join("lib.rs"), INTRA_DOC_REPRO_FIXED).unwrap();

    let output = Command::new("cargo")
        .current_dir(&dir)
        .env("RUSTDOCFLAGS", "-D warnings")
        .args(["doc", "--document-private-items"])
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn cargo doc: {e}"));

    let _ = fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "cargo doc must be clean on the fixed output:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A JavaScript indexed call `items[i]` reads like an inline link but
/// is call syntax.
///
/// [i]: count
///
/// Even an explicit `--include links` leaves the file byte-unchanged with zero
/// records.
#[test]
fn js_indexed_call_stays_unchanged_even_with_links_included() {
    let original =
        fs::read_to_string(fixture_dir().join("links_js_indexed_call_untouched.js")).unwrap();
    let arg_sets: [&[&str]; 2] = [&[], &["--include", "links"]];

    for args in arg_sets {
        let tmp = temp_file("js");
        fs::write(&tmp, &original).unwrap();

        let output = run_command(args, &tmp);
        assert!(
            output.status.success(),
            "run with {args:?} should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(&tmp).unwrap(),
            original,
            "indexed calls must stay byte-unchanged under {args:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("success["),
            "the links op must not rewrite call syntax: {stderr}"
        );
        let _ = fs::remove_file(&tmp);
    }
}

/// With `links.by_extension: { rs: 2 }`, a single-use doc-comment link in a
/// `.rs` file stays byte-unchanged with no link record (dry-run).
///
/// A `.md` file at the default threshold 1 hoists.
#[test]
fn links_by_extension_rs_two_leaves_single_use_rs_unchanged() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let rs = dir.join("lib.rs");
    let rs_source = "/// see [A](http://x) once\npub fn f() {}\n";
    fs::write(&rs, rs_source).unwrap();
    let md = dir.join("doc.md");
    fs::write(&md, "only [A](http://x) once\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "links:\n  by_extension:\n    rs: 2\n").unwrap();

    // The .rs single use is below the rs threshold of 2: no record, unchanged.
    let output = Command::new(binary())
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "--include",
            "links",
            "--dry-run",
        ])
        .arg(&rs)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("`[A](http://x)` -> `[A]`"),
        "single-use .rs link below threshold must not hoist: {stderr:?}"
    );
    assert_eq!(
        fs::read_to_string(&rs).unwrap(),
        rs_source,
        "single-use .rs file must stay byte-unchanged"
    );

    // The .md file has no rs override, so the default threshold 1 applies and
    // hoists the single use with a trailing definition.
    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "links"])
        .arg(&md)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "md in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&md).unwrap(),
        "only [A] once\n\n[A]: http://x\n",
        ".md at threshold 1 must hoist the single use"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A global `links.min_occurrences: 2` suppresses a single-use `.rs` link.
#[test]
fn links_global_min_two_suppresses_single_use_rs() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let rs = dir.join("lib.rs");
    let rs_source = "/// see [A](http://x) once\npub fn f() {}\n";
    fs::write(&rs, rs_source).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "links:\n  min_occurrences: 2\n").unwrap();

    let output = Command::new(binary())
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "--include",
            "links",
            "--dry-run",
        ])
        .arg(&rs)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("`[A](http://x)` -> `[A]`"),
        "single-use .rs link below threshold 2 must not hoist: {stderr:?}"
    );
    assert_eq!(
        fs::read_to_string(&rs).unwrap(),
        rs_source,
        "single-use .rs file must stay byte-unchanged under min_occurrences: 2"
    );
    let _ = fs::remove_dir_all(&dir);
}
