//! Integration tests for the `vis` subcommand of `rust-llm-tidy`.
//!
//! Mirrors the helper pattern from `fix.rs` (`run_command`, `binary`,
//! `manifest_dir`, `fixture_dir`). Each test runs the built CLI binary against
//! fixture files in `tests/fixtures/vis/`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `all` pipeline narrows visibility (vis runs after reorder, before check).
#[test]
fn all_pipeline_runs_vis_after_reorder() {
    // Every item is documented so `check` does not error out and abort `all`.
    let source = "\
/// Module doc.
pub(crate) mod m {
    /// Fn doc.
    pub fn f() {}
}
";
    let tmp = temp_file("rs");
    fs::write(&tmp, source).unwrap();

    let output = run_command(&["all"], &tmp);
    assert!(
        output.status.success(),
        "all should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert!(
        actual.contains("pub(crate) fn f"),
        "vis must narrow bare pub inside all: {actual}"
    );
    assert!(
        !actual.contains("pub fn f"),
        "bare pub fn must be gone after all: {actual}"
    );
}

/// Crate-aware DEFAULT: `pub fn f` in foo.rs narrows to `pub(crate)` because
/// lib.rs declares `pub(crate) mod foo;` (cross-file floor).
#[test]
fn vis_crate_aware_narrows_cross_file() {
    let lib_path = make_temp_crate("pub(crate) mod foo;\n", "pub fn f() {}\n");
    let foo_path = src_sibling(&lib_path, "foo.rs");
    let output = run_command(&["vis"], &foo_path);
    assert!(
        output.status.success(),
        "vis crate-aware should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&foo_path).unwrap();
    assert!(
        actual.contains("pub(crate) fn f"),
        "cross-file floor must narrow foo::f: {actual}"
    );
    let _ = fs::remove_dir_all(lib_path.parent().unwrap().parent().unwrap());
}

/// `vis --dry-run` on `narrow_pub_crate_before.rs` matches `_after.rs`.
#[test]
fn vis_dry_run_matches_after() {
    let before = fixture_dir().join("narrow_pub_crate_before.rs");
    let expected = fs::read_to_string(fixture_dir().join("narrow_pub_crate_after.rs")).unwrap();
    let output = run_command(&["vis", "--dry-run"], &before);

    assert!(
        output.status.success(),
        "vis --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, expected,
        "dry-run stdout must match narrow_pub_crate_after.rs"
    );
}

/// Idempotency: running `vis --dry-run` on an `_after` fixture is a no-op.
#[test]
fn vis_idempotent_on_after_fixtures() {
    for name in ["narrow_pub_crate_after.rs", "reexport_guard_after.rs"] {
        let path = fixture_dir().join(name);
        let expected = fs::read_to_string(&path).unwrap();
        let output = run_command(&["vis", "--dry-run"], &path);
        assert!(
            output.status.success(),
            "vis --dry-run on {name} should succeed"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            &*stdout, &*expected,
            "{name} must be idempotent (output unchanged)"
        );
    }
}

/// In-place write: copy before.rs to a temp file, run `vis`, assert content.
#[test]
fn vis_in_place_write() {
    let expected = fs::read_to_string(fixture_dir().join("narrow_pub_crate_after.rs")).unwrap();
    let tmp = temp_file("rs");
    fs::write(
        &tmp,
        fs::read_to_string(fixture_dir().join("narrow_pub_crate_before.rs")).unwrap(),
    )
    .unwrap();

    let output = run_command(&["vis"], &tmp);
    assert!(
        output.status.success(),
        "vis in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "in-place file must match _after fixture");
}

/// A non-existent path is rejected.
#[test]
fn vis_nonexistent_path_fails() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-vis-missing-{}-{}.rs",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let output = run_command(&["vis"], &nonexistent);
    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
}

/// `vis --dry-run` on `reexport_guard_before.rs` leaves it unchanged.
#[test]
fn vis_reexport_guard_unchanged() {
    let before = fixture_dir().join("reexport_guard_before.rs");
    let expected = fs::read_to_string(fixture_dir().join("reexport_guard_after.rs")).unwrap();
    let output = run_command(&["vis", "--dry-run"], &before);

    assert!(
        output.status.success(),
        "vis --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, expected,
        "re-export guard: dry-run stdout must match reexport_guard_after.rs (unchanged)"
    );
}

/// Standalone path: a file with NO Cargo.toml above it must still run
/// (standalone narrowing) and must not error.
#[test]
fn vis_standalone_without_cargo_toml() {
    let tmp = temp_file("rs");
    fs::write(&tmp, "pub(crate) mod m {\n    pub fn f() {}\n}\n").unwrap();
    let output = run_command(&["vis"], &tmp);
    // May warn about no Cargo.toml on stderr, but must succeed and still narrow.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert!(
        output.status.success(),
        "standalone path must succeed when no Cargo.toml is found: {stderr}"
    );
    assert!(
        actual.contains("pub(crate) fn f"),
        "standalone fallback still narrows inline mod: {actual}"
    );
}

/// Unresolved `mod` diagnostic reaches stderr (REQ-006) without failing vis.
#[test]
fn vis_warns_on_unresolved_mod() {
    let lib_path = make_temp_crate("pub(crate) mod foo;\nmod missing;\n", "pub fn f() {}\n");
    let foo_path = src_sibling(&lib_path, "foo.rs");
    let output = run_command(&["vis"], &foo_path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let foo_actual = fs::read_to_string(src_sibling(&lib_path, "foo.rs")).unwrap();
    let _ = fs::remove_dir_all(lib_path.parent().unwrap().parent().unwrap());
    assert!(
        output.status.success(),
        "vis must still succeed with an unresolved mod (REQ-006): {stderr}"
    );
    assert!(
        stderr.contains("mod missing") && stderr.contains("resolves to no"),
        "unresolved mod warning must name the mod and its empty resolution: {stderr}"
    );
    assert!(
        foo_actual.contains("pub(crate) fn f"),
        "vis must still narrow foo::f despite the unresolved sibling mod: {foo_actual}"
    );
}

// -- Helpers (mirrors fix.rs) ----------------------------------------

/// The directory holding `vis` fixtures.
fn fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("vis")
}

/// Build a minimal temp crate dir with a Cargo.toml + src/lib.rs + src/foo.rs,
/// returning the crate root path (src/lib.rs). `lib_src` declares
/// `pub(crate) mod foo;`; `foo.rs` holds bare-`pub` children.
fn make_temp_crate(lib_src: &str, foo_src: &str) -> PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let root = std::env::temp_dir().join(format!("rlt-vis-crate-{pid}-{seq}"));
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"rlt_vis_crate_{pid}_{seq}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"src/lib.rs\"\n"
        ),
    )
    .unwrap();
    fs::write(src.join("lib.rs"), lib_src).unwrap();
    fs::write(src.join("foo.rs"), foo_src).unwrap();
    src.join("lib.rs")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Resolve a sibling file in the same src/ dir as `lib_path`.
fn src_sibling(lib_path: &std::path::Path, name: &str) -> PathBuf {
    lib_path.parent().unwrap().join(name)
}

/// Create a numbered temporary file path.
fn temp_file(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-vis-{}-{}.{}", pid, seq, ext))
}

/// Return the path to the `rust-llm-tidy` debug binary.
///
/// Prefers `CARGO_BIN_EXE_rust_llm_tidy` (set by `cargo test` at runtime);
/// falls back to the sibling of the test binary under `target/<triple>/debug/`,
/// since the test binary lives in `target/<triple>/debug/deps/`.
fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("current_exe must resolve");
    // Drop `<test-name>-<hash>` and `deps/` to reach `target/<triple>/debug/`.
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
