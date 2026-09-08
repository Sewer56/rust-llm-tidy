//! C# reorder tests for the `rust-llm-tidy` CLI.
//!
//! One synthetic-fixture test per ordering/spacing rule under
//! `tests/fixtures/reorder/csharp/` (declared through `synthetic_fixture!`),
//! plus member-profile reorders over temp copies of the fixture pair.

// The macro invocations lead the file: the reorder profile sorts
// invocations of crate-defined macros before the `use` items.

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

use super::{manifest_dir, reorder_in_place, run_command, run_dry_run, temp_file_ext};
use std::fs;
use std::path::PathBuf;

/// An in-place reorder of `reorder_cs_before.cs` writes the `_after`
/// fixture byte-for-byte: members land in the profile order.
///
/// The caller precedes its callee, and the trailing `using` hoists to the
/// pinned using block.
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

/// A pure-CRLF `.cs` source still reorders.
///
/// - The guard accepts pure CRLF and declines lone-`\r` sources.
/// - The field hoists above the method with every newline still part of
///   a `\r\n` pair.
/// - A second run emits zero records.
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

// ── C# reorder: member profile + pinned usings ────────────────────

/// The directory holding the C# reorder fixture pair.
fn csharp_reorder_fixture_dir() -> PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
        .join("csharp")
}
