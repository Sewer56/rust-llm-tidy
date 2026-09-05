//! Integration tests for `rust-llm-tidy` CLI.
//!
//! Tests are split into two groups:
//!
//! 1. Synthetic fixture tests (`tests/fixtures/reorder/<lang>/*_before.<ext>`
//!    → `*_after.<ext>`): one test per ordering/spacing rule, for `rust`
//!    and `csharp`.  Each fixture's header comment documents the rule and
//!    the expected before/after state.
//!
//! 2. CLI behavior tests: dry-run, in-place writes, directory traversal,
//!    error handling, and idempotency.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Run `rust-llm-tidy --include reorder --dry-run` against
/// `<name>_before.<ext>` in `tests/fixtures/reorder/<lang>/`.
///
/// Returns `(stdout, stderr, exit, before_path, expected_after_content)`.
macro_rules! run_fixture {
    ($lang:ident, $ext:literal, $name:ident) => {{
        let fixture_dir = manifest_dir()
            .join("tests")
            .join("fixtures")
            .join("reorder")
            .join(stringify!($lang));
        let before_path = fixture_dir.join(concat!(stringify!($name), "_before.", $ext));
        let expected_after = include_str!(concat!(
            "fixtures/reorder/",
            stringify!($lang),
            "/",
            stringify!($name),
            "_after.",
            $ext
        ))
        .to_string();

        let (stdout, stderr, exit) = run_dry_run(&before_path);

        (stdout, stderr, exit, before_path, expected_after)
    }};
}

/// Declare a fixture test.  The test name is the fixture rule name.
///
/// Dry-run reports change records on stderr (allowing zero records for an
/// already-tidy fixture) and keeps stdout empty. Byte-for-byte "produces
/// _after" coverage is preserved by re-ordering a temp copy in place and
/// comparing its content.
macro_rules! synthetic_fixture {
    ($lang:ident, $ext:literal, $name:ident) => {
        #[test]
        fn $name() {
            let (stdout, stderr, exit, before_path, expected_after) =
                run_fixture!($lang, $ext, $name);
            assert_eq!(
                exit, 0,
                concat!(stringify!($name), " dry-run should succeed")
            );
            assert!(
                stdout.is_empty(),
                concat!(
                    stringify!($name),
                    " dry-run must not print reconstructed source to stdout"
                )
            );
            // Any stderr output from a reorder dry-run must be change records,
            // never reconstructed source (which would lack the op marker).
            for line in stderr.lines() {
                assert!(
                    line.contains("success[REORDER]"),
                    "{} dry-run stderr must only carry change records: {}",
                    stringify!($name),
                    line
                );
            }
            // In-place reorder still produces the _after fixture byte-for-byte.
            assert_eq!(
                reorder_in_place(&before_path, $ext),
                expected_after,
                concat!(
                    stringify!($name),
                    " fixture: in-place reorder must match _after.",
                    $ext
                )
            );
        }
    };
}

// ── Synthetic fixture tests: one per rule ─────────────────────────

synthetic_fixture!(rust, "rs", phase_extern_crate_stable);

synthetic_fixture!(rust, "rs", phase_other_stable);

synthetic_fixture!(rust, "rs", phase_use_stable);

synthetic_fixture!(rust, "rs", phase_mod_non_test_stable);

synthetic_fixture!(rust, "rs", phase_macro_alphabetical);

synthetic_fixture!(rust, "rs", phase_macro_dependency);

synthetic_fixture!(rust, "rs", phase_macro_invocation_after_def);

synthetic_fixture!(rust, "rs", phase_const_static_alphabetical);

synthetic_fixture!(rust, "rs", phase_const_static_dependency);

synthetic_fixture!(rust, "rs", phase_type_alphabetical);

synthetic_fixture!(rust, "rs", phase_type_dependency);

synthetic_fixture!(rust, "rs", phase_trait_alphabetical);

synthetic_fixture!(rust, "rs", phase_trait_dependency);

synthetic_fixture!(rust, "rs", phase_impl_inherent_before_trait);

synthetic_fixture!(rust, "rs", phase_impl_after_matching_type);

synthetic_fixture!(rust, "rs", phase_impl_orphan_stable);

synthetic_fixture!(rust, "rs", fn_visibility_groups);

synthetic_fixture!(rust, "rs", fn_main_first);

synthetic_fixture!(rust, "rs", fn_callers_before_callees);

synthetic_fixture!(rust, "rs", fn_alphabetical_tie_break);

synthetic_fixture!(rust, "rs", fn_mutual_recursion_contiguous);

synthetic_fixture!(rust, "rs", cfg_test_mod_last_stable);

synthetic_fixture!(rust, "rs", mod_file_decl_stays_in_phase);

synthetic_fixture!(rust, "rs", preamble_preserved);

synthetic_fixture!(rust, "rs", trailer_preserved);

synthetic_fixture!(rust, "rs", fn_interstitial_comment_travels_with_next);

synthetic_fixture!(rust, "rs", docs_attrs_travel);

synthetic_fixture!(rust, "rs", spacing_compact_use_mod_const_static);

synthetic_fixture!(rust, "rs", spacing_blank_line_between_phases);

synthetic_fixture!(rust, "rs", spacing_blank_line_fn_visibility);

synthetic_fixture!(rust, "rs", safety_line_preservation);

// ── Synthetic C# fixture tests: one per rule ──────────────────────

synthetic_fixture!(csharp, "cs", usings_hoist_above_types);

synthetic_fixture!(csharp, "cs", usings_keep_source_order);

synthetic_fixture!(csharp, "cs", top_level_types_keep_source_order);

synthetic_fixture!(csharp, "cs", member_buckets_reorder);

synthetic_fixture!(csharp, "cs", member_fields_keep_source_order);

synthetic_fixture!(csharp, "cs", member_delegates_events_keep_source_order);

synthetic_fixture!(csharp, "cs", member_enums_nested_types_keep_source_order);

synthetic_fixture!(csharp, "cs", member_properties_indexers_keep_source_order);

synthetic_fixture!(csharp, "cs", member_callers_before_callees);

synthetic_fixture!(csharp, "cs", member_methods_keep_source_order);

synthetic_fixture!(csharp, "cs", member_mutual_recursion_contiguous);

synthetic_fixture!(csharp, "cs", member_docs_attributes_travel);

synthetic_fixture!(csharp, "cs", member_leading_comment_travels);

synthetic_fixture!(csharp, "cs", member_compact_spacing_stays_compact);

synthetic_fixture!(csharp, "cs", nested_type_moves_whole);

synthetic_fixture!(csharp, "cs", namespace_usings_pin_first);

synthetic_fixture!(csharp, "cs", header_comment_preserved);

synthetic_fixture!(csharp, "cs", footer_comment_preserved);

synthetic_fixture!(csharp, "cs", spacing_usings_compact_types_separated);

// ── Idempotency: every _after fixture must be unchanged ───────────

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Idempotency: every `_after.*` fixture, in every language directory
/// under `tests/fixtures/reorder/`, must be unchanged by a second run.
#[test]
fn all_after_fixtures_should_be_idempotent_on_rerun() {
    let fixture_root = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder");
    // Each language directory under the fixture root holds its own
    // `<name>_after.<ext>` fixture pairs.
    let mut after_files: Vec<std::path::PathBuf> = Vec::new();
    for lang in fs::read_dir(&fixture_root).unwrap() {
        let lang_dir = lang.unwrap().path().read_dir().unwrap();
        for entry in lang_dir {
            let path = entry.unwrap().path();
            let is_after = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name.contains("_after."));
            if is_after {
                after_files.push(path);
            }
        }
    }
    after_files.sort();

    assert!(!after_files.is_empty(), "no _after fixtures found");

    for after_path in &after_files {
        let (stdout, stderr, exit) = run_dry_run(after_path);

        assert_eq!(exit, 0, "{} dry-run should succeed", after_path.display());
        assert!(
            stdout.is_empty(),
            "{} dry-run must not print source to stdout",
            after_path.display()
        );
        assert!(
            stderr.is_empty(),
            "{} is already tidy: dry-run must emit zero change records",
            after_path.display()
        );
    }
}

/// An in-place reorder of `reorder_cs_before.cs` writes the `_after`
/// fixture byte-for-byte: members land in the profile order, the caller
/// precedes its callee, and the trailing `using` hoists to the pinned
/// using block.
#[test]
fn csharp_member_reorder_matches_after_fixture() {
    let before = csharp_reorder_fixture_dir().join("reorder_cs_before.cs");
    let expected_after =
        fs::read_to_string(csharp_reorder_fixture_dir().join("reorder_cs_after.cs")).unwrap();
    let tmp = temp_file_ext("cs");
    fs::write(&tmp, fs::read_to_string(&before).unwrap()).unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "C# reorder should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected_after,
        "in-place C# reorder must match reorder_cs_after.cs"
    );
}

/// A pure-CRLF `.cs` source still reorders - the accept side of the guard
/// that declines lone-`\r` sources: the field hoists above the method with
/// every newline still part of a `\r\n` pair, and a second run emits zero
/// records.
#[test]
fn csharp_reorder_on_pure_crlf_source_preserves_the_endings() {
    let source = "class C\r\n{\r\n    void M() {}\r\n    int F;\r\n}\r\n";
    let tmp = temp_file_ext("cs");
    fs::write(&tmp, source).unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "CRLF C# reorder should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&tmp).unwrap();
    assert!(
        after.find("int F;").unwrap() < after.find("void M()").unwrap(),
        "fields must precede methods under the C# profile: {after:?}"
    );
    assert_eq!(
        after.matches('\n').count(),
        after.matches("\r\n").count(),
        "every newline must stay CRLF after the reorder: {after:?}"
    );

    let dry = run_command(&["--include", "reorder", "--dry-run"], &tmp);
    let _ = fs::remove_file(&tmp);
    assert!(dry.status.success(), "second run should succeed");
    assert!(
        String::from_utf8_lossy(&dry.stderr).is_empty(),
        "second run on the CRLF rewrite must emit zero records"
    );
}

// ── CLI behavior tests ────────────────────────────────────────────

/// `--dry-run` reports the would-be reorder move on stderr without printing
/// the reconstructed source to stdout or modifying the file on disk.
#[test]
fn dry_run_should_not_write_files() {
    let source = "fn a() {}\nfn b() { a(); }\n";

    let (stdout, stderr, exit) = run(source, &["--dry-run"]);

    assert_eq!(exit, 0, "dry-run should succeed");
    assert!(stdout.is_empty(), "stdout must be empty on dry-run success");
    assert!(
        stderr.contains("success[REORDER]"),
        "stderr should report the reorder move as a change line: {stderr}"
    );
    assert!(
        !stderr.contains("fn a()"),
        "stderr must not echo reconstructed source: {stderr}"
    );
}

/// An empty directory is accepted and produces no output.
#[test]
fn empty_directory_should_run_cleanly() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let (stdout, stderr, exit) = run_dir(&dir, &[]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit, 0, "empty directory should exit successfully");
    assert!(
        stdout.is_empty(),
        "stdout should be empty for empty directory"
    );
    assert!(stderr.is_empty(), "stderr should be empty on success");
}

/// `--extension` allows an extra extension additively: the file runs the
/// tables-only unmapped profile, and without the flag the
/// same file is a silent skip.
#[test]
fn extension_flag_allows_unmapped_extension_tables_only() {
    let source = "| a | b |\n| --- | --- |\n| 1 | 22 |\n";

    // Without the flag the explicit file is a silent skip.
    let file = temp_file_ext("org");
    fs::write(&file, source).unwrap();
    let out = run_command(&["--json"], &file);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[]");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let _ = fs::remove_file(&file);

    // With the flag the file is allowed and its GFM table aligns.
    let file = temp_file_ext("org");
    fs::write(&file, source).unwrap();
    let out = run_command(&["--extension", "org"], &file);
    assert!(
        out.status.success(),
        "--extension org should allow the file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tables were aligned"),
        "the unmapped profile must run tables: {stderr}"
    );
    assert_ne!(fs::read_to_string(&file).unwrap(), source);
    let _ = fs::remove_file(&file);
}

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

/// In-place write: copy a synthetic before fixture to a temp file, run without
/// `--dry-run`, and verify the file content matches the after fixture.
#[test]
fn in_place_write_should_match_after_fixture() {
    let expected = include_str!("fixtures/reorder/rust/phase_use_stable_after.rs");

    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("rust-llm-tidy-write-test-{}-{}.rs", pid, seq));

    fs::write(
        &tmp,
        include_str!("fixtures/reorder/rust/phase_use_stable_before.rs"),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "rust-llm-tidy (no --dry-run) failed"
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);

    assert_eq!(
        actual, expected,
        "in-place write: temp file content must match phase_use_stable_after.rs"
    );
}

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

/// Safety check: parse-invalid source must cause an error exit.
/// We verify that rust-llm-tidy exits non-zero when given a non-Rust file.
#[test]
fn invalid_source_should_abort_with_error() {
    let source = "not valid rust {{{";

    let (_stdout, stderr, exit) = run(source, &[]);

    assert_ne!(exit, 0, "rust-llm-tidy should exit non-zero on parse error");
    assert!(!stderr.is_empty(), "stderr should contain error message");
}

// ── Language tiers ────────────────────────────────────────────────

/// Markdown-family siblings (`.markdown`, `.txt`, `.text`, `.mdx`, and the
/// uppercase `.TXT` variant) behave exactly like `.md` on identical input:
/// same fixed bytes, same stderr records and lint findings, same exit code.
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

/// A non-existent path is rejected with an error exit.
#[test]
fn nonexistent_path_should_fail_with_error() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-missing-{}-{}-{}-{}-{}-{}-{}-{}-{}.rs",
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id()
    ));

    let output = run_command(&["--include", "reorder"], &nonexistent);
    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).is_empty(),
        "stderr should report the missing path"
    );
}

/// Directory recursion collects `README.MD`/`lib.RS` case variants and
/// excludes extensions outside the default set (like `notes.org`).
#[test]
fn recursive_dir_collects_uppercase_variants_excludes_others() {
    let dir = temp_dir();
    let nested = dir.join("src");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("lib.RS"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(
        dir.join("README.MD"),
        "| Name | Value | Description |\n| --- | --- | --- |\n| a | 1 | first |\n| longname | 200 | second item |\n",
    )
    .unwrap();
    fs::write(dir.join("notes.org"), "not allowed by default\n").unwrap();

    // Rust-only reorder runs on the nested `.RS` and reports it by path.
    let (_stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    assert_eq!(exit, 0, "dir with .RS/.MD/.org should succeed");
    assert!(
        stderr.contains("lib.RS"),
        "recursion must collect and process lib.RS: {stderr}"
    );

    // Markdown table fix runs on the `.MD` and reports it by path.
    let (_stdout, md_stderr, md_exit) = run_dir(&dir, &["--include", "tables", "--dry-run"]);
    assert_eq!(md_exit, 0, "tables dry-run on dir should succeed");
    assert!(
        md_stderr.contains("README.MD") && md_stderr.contains("success[FIX]"),
        "recursion must collect and process README.MD: {md_stderr}"
    );
    assert!(
        !md_stderr.contains("notes.org") && !stderr.contains("notes.org"),
        "notes.org must be excluded silently"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--dry-run` on a directory reports each file's moves as path-labeled change
/// lines on stderr, leaving stdout empty and the files unmodified.
#[test]
fn recursive_directory_dry_run_should_label_each_move_with_path() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let file_a = dir.join("a.rs");
    let file_b = dir.join("b.rs");

    fs::write(&file_a, "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(&file_b, "fn c() {}\nfn d() { c(); }\n").unwrap();

    let (stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit, 0, "dry-run on directory should succeed");
    assert!(stdout.is_empty(), "stdout should be empty on success");
    assert!(
        stderr.contains("a.rs:") && stderr.contains("b.rs:"),
        "multi-file dry-run must label each change line with its path: {stderr}"
    );
    assert!(
        stderr.contains("rearrange fn b from pos 2 to pos 1")
            && stderr.contains("rearrange fn d from pos 2 to pos 1"),
        "directory dry-run should report each file's move on stderr: {stderr}"
    );
}

/// If a directory contains a valid file and an invalid file, the valid file is
/// still reordered and the operation exits non-zero.
#[test]
fn recursive_directory_error_should_still_reorder_valid_file() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let good = dir.join("good.rs");
    let bad = dir.join("bad.rs");

    fs::write(&good, "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(&bad, "not valid rust {{{").unwrap();

    let (_stdout, stderr, exit) = run_dir(&dir, &[]);

    let actual_good = fs::read_to_string(&good).unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert_ne!(exit, 0, "directory with invalid file should exit non-zero");
    assert!(
        !stderr.is_empty(),
        "stderr should contain error message for invalid file"
    );

    let a_pos = actual_good.find("fn a").expect("fn a missing");
    let b_pos = actual_good.find("fn b").expect("fn b missing");
    assert!(
        b_pos < a_pos,
        "valid file should still be reordered despite sibling error"
    );
}

/// A directory is processed recursively, reordering every `.rs` file.
#[test]
fn recursive_directory_should_reorder_every_rs_file() {
    let dir = temp_dir();
    let root_file = dir.join("phase_use.rs");
    let nested_dir = dir.join("utils");
    let nested_file = nested_dir.join("phase_mod.rs");

    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(
        &root_file,
        include_str!("fixtures/reorder/rust/phase_use_stable_before.rs"),
    )
    .unwrap();
    fs::write(
        &nested_file,
        include_str!("fixtures/reorder/rust/phase_mod_non_test_stable_before.rs"),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &dir);
    assert!(
        output.status.success(),
        "directory run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected_root = include_str!("fixtures/reorder/rust/phase_use_stable_after.rs");
    let expected_nested = include_str!("fixtures/reorder/rust/phase_mod_non_test_stable_after.rs");
    let actual_root = fs::read_to_string(&root_file).unwrap();
    let actual_nested = fs::read_to_string(&nested_file).unwrap();

    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        actual_root, expected_root,
        "phase_use.rs should be reordered in place"
    );
    assert_eq!(
        actual_nested, expected_nested,
        "utils/phase_mod.rs should be reordered in place"
    );
}

/// `reorder --dry-run` on a CRLF reordering source reports its move on stderr
/// and leaves stdout empty (no reconstructed source).
#[test]
fn reorder_dry_run_reports_change_with_empty_stdout() {
    let source = "fn a() {}\r\nfn b() { a(); }\r\n";
    let (stdout, stderr, exit) = run(source, &["--dry-run"]);
    assert_eq!(exit, 0, "dry-run should succeed");
    assert!(stdout.is_empty(), "dry-run must not print source to stdout");
    assert!(
        stderr.contains("success[REORDER]"),
        "dry-run must report a reorder change on stderr: {stderr:?}"
    );
}

/// Reorder a temp copy of `path` (keeping the given extension, which
/// selects the language backend) in place and return the rewritten content.
///
/// Preserves the byte-for-byte "produces the _after fixture" coverage while
/// dry-run keeps stdout empty.
fn reorder_in_place(path: &std::path::Path, ext: &str) -> String {
    let tmp = temp_file_ext(ext);
    fs::copy(path, &tmp).unwrap();
    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "in-place reorder failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    result
}

/// In-place reorder of a CRLF source preserves every `\r\n` and reorders
/// callers before callees. CRLF input is built in-memory (not from a
/// committed fixture, which git would normalize on checkout).
#[test]
fn reorder_in_place_preserves_crlf() {
    let source = "fn b() { a(); }\r\nfn a() {}\r\n";
    let result = run_and_read(source);

    // Caller (b) before callee (a).
    let a_pos = result.find("fn a").expect("fn a missing");
    let b_pos = result.find("fn b").expect("fn b missing");
    assert!(b_pos < a_pos, "b (caller) before a (callee)");

    // Every `\n` must be part of `\r\n` (no CRLF -> LF flip).
    assert_eq!(
        result.matches('\n').count(),
        result.matches("\r\n").count(),
        "every newline must be CRLF after reorder: {result:?}"
    );
}

/// In-place reorder both writes the reordered file and reports the move as a
/// change line on stderr, mirroring the records a dry-run would have previewed.
#[test]
fn reorder_in_place_reports_change_and_writes() {
    let fixture = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
        .join("rust");
    let expected =
        fs::read_to_string(fixture.join("fn_interstitial_comment_travels_with_next_after.rs"))
            .unwrap();
    let tmp = temp_file();
    fs::copy(
        fixture.join("fn_interstitial_comment_travels_with_next_before.rs"),
        &tmp,
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "in-place reorder on a moving fixture should succeed"
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected,
        "in-place reorder must write the after fixture"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[REORDER]"),
        "in-place reorder must also report its change line on stderr: {stderr}"
    );
}

/// Realistic file: struct, impl, use, and multiple fns.
/// Tests that multi-phase ordering keeps use first, struct+impl together, and
/// callers before callees.
#[test]
fn reorder_real_file_should_keep_phase_and_caller_order() {
    let source = "\
use std::fmt;\n\n\
pub struct Config {\n\
    pub name: String,\n}\n\n\
impl Config {\n\
    pub fn new(name: &str) -> Self {\n\
        Config {\n\
            name: name.to_string(),\n\
        }\n\
    }\n}\n\n\
fn validate(c: &Config) -> bool {\n\
    !c.name.is_empty()\n}\n\n\
pub fn build(name: &str) -> Option<Config> {\n\
    let c = Config::new(name);\n\
    if validate(&c) {\n\
        Some(c)\n\
    } else {\n\
        None\n\
    }\n}\n";

    let result = run_and_read(source);

    let use_pos = result.find("use std::fmt").unwrap();
    let struct_pos = result.find("pub struct Config").unwrap();
    let impl_pos = result.find("impl Config").unwrap();
    let build_pos = result.find("pub fn build").unwrap();
    let validate_pos = result.find("fn validate").unwrap();

    assert!(use_pos < struct_pos, "use before struct");
    assert!(struct_pos < impl_pos, "struct before its impl");
    assert!(
        build_pos < validate_pos,
        "build (caller) before validate (callee)"
    );
}

// ── Corpus gate ────────────────────────────────────────────────────

/// The repository corpus gate: a `--dry-run` over this repository's root
/// (repo config active, the same invocation CI's tidy job makes) exits 0
/// and emits zero change records - every tracked file is already tidy.
#[test]
fn repo_corpus_dry_run_emits_zero_change_records() {
    let root = manifest_dir()
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve");
    // Guard against a vacuous pass: only this repository's root holds both
    // the workspace manifest and the tidy config, so the walk below covers
    // real files.
    assert!(root.join("src").join("Cargo.toml").is_file());
    assert!(root.join(".rust-llm-tidy.yml").is_file());

    let output = Command::new(binary())
        .current_dir(&root)
        .args(["--dry-run", "."])
        .output()
        .expect("failed to spawn rust-llm-tidy over the repo root");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "corpus dry-run must exit 0: {stderr}"
    );
    assert!(stdout.is_empty(), "dry-run prints nothing to stdout");
    assert!(
        !stderr.contains("success["),
        "the whole repository must emit zero change records: {stderr}"
    );
}

/// An already-sorted file (callers before callees) should be unchanged.
#[test]
fn sorted_file_should_roundtrip_unchanged() {
    let source = "\
fn main() {\n\
    a();\n\
    b();\n}\n\n\
fn a() {\n\
    helper();\n}\n\n\
fn b() {}\n\n\
fn helper() {}\n";

    let result = run_and_read(source);
    let main_pos = result.find("fn main").unwrap();
    let a_pos = result.find("fn a").unwrap();
    let b_pos = result.find("fn b").unwrap();
    let helper_pos = result.find("fn helper").unwrap();

    assert!(main_pos < a_pos, "main before a");
    assert!(a_pos < helper_pos, "a before helper (a calls helper)");
    assert!(b_pos < helper_pos, "b before helper (original order)");
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

// ── C# reorder: member profile + pinned usings ────────────────────

/// The directory holding the C# reorder fixture pair.
fn csharp_reorder_fixture_dir() -> std::path::PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
        .join("csharp")
}

/// Run rust-llm-tidy on `content` (written to a tempfile) with optional
/// `--dry-run`.
/// Returns (stdout, stderr, exit_code).
fn run(content: &str, args: &[&str]) -> (String, String, i32) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = dir.join(format!("rust-llm-tidy-test-{}-{}.rs", pid, seq));
    fs::write(&file, content).unwrap();

    let mut full_args = vec!["--include", "reorder"];
    full_args.extend(args);
    let output = run_command(&full_args, &file);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit = output.status.code().unwrap_or(-1);

    let _ = fs::remove_file(&file);

    (stdout, stderr, exit)
}

/// Read a tempfile after rust-llm-tidy has modified it.
fn run_and_read(content: &str) -> String {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = dir.join(format!("rust-llm-tidy-test-{}-{}.rs", pid, seq));
    fs::write(&file, content).unwrap();

    let output = run_command(&["--include", "reorder"], &file);
    assert!(
        output.status.success(),
        "rust-llm-tidy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result = fs::read_to_string(&file).unwrap();
    let _ = fs::remove_file(&file);
    result
}

/// Run `rust-llm-tidy` on a directory with optional arguments.
fn run_dir(dir: &std::path::Path, args: &[&str]) -> (String, String, i32) {
    let mut full_args = vec!["--include", "reorder"];
    full_args.extend(args);
    let output = run_command(&full_args, dir);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit)
}

/// Run `rust-llm-tidy --include reorder --dry-run` on `path` and return
/// `(stdout, stderr, exit)`.
fn run_dry_run(path: &std::path::Path) -> (String, String, i32) {
    let output = run_command(&["--include", "reorder", "--dry-run"], path);

    assert!(
        output.status.success(),
        "rust-llm-tidy --dry-run failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Strip the `path:` label from every stderr line so outputs for the same
/// content under different file names compare equal.
fn strip_path_prefix(stderr: &str, path: &std::path::Path) -> String {
    let prefix = format!("{}:", path.display());
    stderr
        .lines()
        .map(|line| line.strip_prefix(&prefix).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary `.rs` file path for fixture copies that
/// reorder in place.
fn temp_file() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-file-{}-{}.rs", pid, seq))
}

/// Create a numbered temporary file path with the given extension, for
/// fixture copies whose language the extension selects (`.cs`) and
/// case-sensitivity tests that need `.RS`/`.MD`/`.TXT` (the local
/// `temp_file` is fixed to `.rs`).
fn temp_file_ext(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-ext-{}-{}.{}", pid, seq, ext))
}

// ── Helpers ───────────────────────────────────────────────────────

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Returns the path to the `rust-llm-tidy` binary for spawning in tests.
///
/// Resolution order:
/// 1. `CARGO_BIN_EXE_rust-llm-tidy`; modern Cargo keeps the hyphen.
/// 2. `CARGO_BIN_EXE_rust_llm_tidy`; older Cargo normalized it.
/// 3. Walk up from the test executable to the `target/<profile>` dir that
///    holds the peer binary.
///
/// Panics when none resolve.
fn binary() -> std::path::PathBuf {
    for var in ["CARGO_BIN_EXE_rust-llm-tidy", "CARGO_BIN_EXE_rust_llm_tidy"] {
        if let Some(path) = std::env::var_os(var) {
            return std::path::PathBuf::from(path);
        }
    }

    // Fallback for direct runs: the test binary lives in `<profile>/deps/`
    // (stable) or the build-out dir (newer Cargo); both sit under the
    // `<profile>` dir that holds the peer binary.
    let mut dir = std::env::current_exe()
        .expect("current_exe must resolve")
        .parent()
        .expect("current_exe must have a parent")
        .to_path_buf();
    loop {
        for bin in ["rust-llm-tidy", "rust-llm-tidy.exe"] {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                return candidate;
            }
        }
        if !dir.pop() {
            break;
        }
    }
    panic!("could not locate the rust-llm-tidy binary next to the test executable");
}
