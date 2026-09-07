//! C# backend tests over the crate fixtures: parse shape, region
//! interaction, reorder composition, and lint semantics.
//!
//! Everything drives the public backend API ([`backend_for`]) the way the
//! CLI pipeline does, with fixture sources under `tests/fixtures/csharp/`.
//!
//! Child map:
//! - `parsing`: parse-tree shape, item spans, preamble, error no-ops.
//! - `reorder`: reorder compositions that succeed end to end.
//! - `reorder_boundaries`: constructs that freeze or decline a reorder.
//! - `doc001_missing_docs`: DOC001 undocumented non-private members.
//! - `doc002_missing_exception_tag`: same-file DOC002 call resolution.
//! - `doc002_cross_file`: DOC002/DOC003 through the lint index.
//! - `doc003_vague_exception`: DOC003 tags without concrete crefs.
//! - `doc004_missing_param_tags`: DOC004/DOC005 `<param>` tag coverage.
//! - `doc006_placeholder`: DOC006 whole-word placeholder terms.
//! - `test001_test_naming`: TEST001 discouraged test names.

use rust_llm_tidy::languages::LanguageBackend;
use rust_llm_tidy::source::ParseResult;

mod doc001_missing_docs;
mod doc002_cross_file;
mod doc002_missing_exception_tag;
mod doc003_vague_exception;
mod doc004_missing_param_tags;
mod doc006_placeholder;
mod parsing;
mod reorder;
mod reorder_boundaries;
mod test001_test_naming;

// ── Lint semantics ───────────────────────────────────────────────

/// Lint diagnostics for a source, filtered to one code.
fn codes(parsed: &ParseResult, code: &str) -> Vec<String> {
    backend()
        .lint(parsed)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| format!("{}:{}", d.line, d.item_name.as_deref().unwrap_or("")))
        .collect()
}

/// Parse `source` through the backend.
fn parse(source: &str) -> ParseResult {
    backend().parse(source).expect("fixture must parse")
}

/// The registered C# backend.
fn backend() -> &'static dyn LanguageBackend {
    rust_llm_tidy::languages::backend_for("cs").expect("cs must resolve a backend")
}
