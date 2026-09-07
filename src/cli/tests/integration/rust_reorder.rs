//! Rust reorder tests for the `rust-llm-tidy` CLI.
//!
//! One synthetic-fixture test per ordering/spacing rule under
//! `tests/fixtures/reorder/rust/` (declared through `synthetic_fixture!`),
//! plus real-file, in-place, and recursive-directory reorder behavior.

// The macro invocations lead the file: the reorder profile sorts
// invocations of crate-defined macros before the `use` items.

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

use super::{
    manifest_dir, reorder_in_place, run_and_read, run_command, run_dry_run, temp_dir, temp_file,
};
use std::fs;

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
        include_str!("../fixtures/reorder/rust/phase_use_stable_before.rs"),
    )
    .unwrap();
    fs::write(
        &nested_file,
        include_str!("../fixtures/reorder/rust/phase_mod_non_test_stable_before.rs"),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &dir);
    assert!(
        output.status.success(),
        "directory run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected_root = include_str!("../fixtures/reorder/rust/phase_use_stable_after.rs");
    let expected_nested =
        include_str!("../fixtures/reorder/rust/phase_mod_non_test_stable_after.rs");
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

/// In-place reorder of a CRLF source preserves every `\r\n` and reorders
/// callers before callees.
///
/// CRLF input is built in-memory (not from a committed fixture, which git
/// would normalize on checkout).
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
