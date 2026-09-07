//! Fixture macros shared by the reorder test modules.
//!
//! `run_fixture!` runs a dry-run against a `<name>_before.<ext>` fixture;
//! `synthetic_fixture!` declares one `#[test]` per ordering rule. The
//! macros expand in the invoking module, so the helpers they call must
//! stay in scope there.

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
            "../fixtures/reorder/",
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
