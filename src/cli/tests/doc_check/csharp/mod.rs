//! C# lint tests: the XML doc dialect over the same lint codes as Rust.
//!
//! The lint tests run the built CLI binary with selected lint codes on a
//! fixture in `tests/fixtures/doc/csharp/`. The JSON record tests live
//! in `crate::json_output`; the text-budget tests in
//! `crate::comment_lexicons`.
//!
//! Child modules:
//! - `doc001_missing_docs`: DOC001 undocumented non-private members
//! - `doc002_missing_exception_tag`: DOC002 missing `<exception>` tags
//! - `doc003_vague_exception`: DOC003 vague `<exception>` crefs
//! - `doc004_missing_param_tags`: DOC004 missing `<param>` tags
//! - `doc005_undocumented_param`: DOC005 omitted parameter names
//! - `doc006_placeholder`: DOC006 doc-comment placeholder markers
//! - `doc009_missing_module_docs`: module-header rule silence
//! - `mod003_qualified_path`: MOD003 fully-qualified path hints
//! - `test001_test_naming`: TEST001 discouraged test-method names

use crate::{manifest_dir, run_command};
use std::path::PathBuf;

mod doc001_missing_docs;
mod doc002_missing_exception_tag;
mod doc003_vague_exception;
mod doc004_missing_param_tags;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod doc009_missing_module_docs;
mod mod003_qualified_path;
mod test001_test_naming;

/// Run `rust-llm-tidy --include lints` on a C# fixture and return its
/// (stderr, exit_code).
pub(super) fn run_csharp_fixture(name: &str) -> (String, i32) {
    let path = csharp_fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// The directory holding the C# lint fixtures.
pub(super) fn csharp_fixture_dir() -> PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("doc")
        .join("csharp")
}
