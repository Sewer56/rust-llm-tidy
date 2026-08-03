//! Shared benchmark fixtures and setup for the `fix_tables` pass.
//!
//! Fixtures live in `fixtures/` and come in two families:
//!
//! - `*_md.md`: plain Markdown files with GFM pipe tables.
//! - `*_rs.rs`: Rust source files whose `///` doc comments contain GFM pipe
//!   tables (the crate strips the `///` prefix before realigning).
//!
//! Each fixture is a real file from an open-source project, embedded verbatim
//! with [`include_str!`] (byte-exact copies, so the benchmarks reflect
//! realistic table shapes). Provenance (repo, path, pinned permalink) is
//! documented in the header comment of each fixture file.

/// Markdown benchmark fixtures: `(name, source)` pairs across three size tiers.
pub const MD_FIXTURES: &[(&str, &str)] = &[
    ("small/md", include_str!("fixtures/small_md.md")),
    ("medium/md", include_str!("fixtures/medium_md.md")),
    ("large/md", include_str!("fixtures/large_md.md")),
];
/// Rust doc-comment benchmark fixtures: `(name, source)` pairs across three
/// size tiers. The tables live inside `///` doc comments.
pub const RS_FIXTURES: &[(&str, &str)] = &[
    ("small/rs", include_str!("fixtures/small_rs.rs")),
    ("medium/rs", include_str!("fixtures/medium_rs.rs")),
    ("large/rs", include_str!("fixtures/large_rs.rs")),
];

/// Produce a misaligned copy of `input` by collapsing each table cell's
/// surrounding padding, so columns no longer line up. Non-table lines and
/// doc-comment prefixes are left untouched.
///
/// Used only in benchmark setup (never inside the measured `iter` closure) to
/// exercise the realignment path against an otherwise-canonical fixture.
///
/// # Arguments
///
/// - `input`: the canonical fixture whose table cells are collapsed so columns
///   no longer line up.
pub fn misalign(input: &str) -> String {
    input.split_inclusive('\n').map(misalign_line).collect()
}

/// Collapse the padding of a single table row, preserving any doc-comment
/// prefix and the line terminator. Non-table lines pass through unchanged.
fn misalign_line(line: &str) -> String {
    let (prefix, body) = strip_doc_prefix(line);
    if body.starts_with('|') {
        // Squeeze the padding around every pipe so cells no longer align.
        let mangled = body.replace(" |", "|").replace("| ", "|");
        format!("{prefix}{mangled}")
    } else {
        line.to_string()
    }
}

/// Strip an optional Rust doc-comment prefix from `line`, mirroring the crate's
/// own logic. Returns `(prefix, rest)` where `prefix` is the leading indent
/// plus the `///` or `//!` marker and one separating space (empty when absent).
fn strip_doc_prefix(line: &str) -> (&str, &str) {
    let indent_end = line.len() - line.trim_start_matches([' ', '\t']).len();
    let core = &line[indent_end..];
    if let Some(rest) = core
        .strip_prefix("///")
        .or_else(|| core.strip_prefix("//!"))
    {
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let prefix_len = line.len() - rest.len();
        (&line[..prefix_len], rest)
    } else {
        ("", line)
    }
}
