//! GFM table alignment: split, validate, and re-pad table columns.
//!
//! [`realign_table`] takes the raw (prefix-stripped) lines of a single
//! contiguous table and returns the canonically aligned lines, or [`None`]
//! when the lines are not a table or are already aligned.

use std::iter::repeat_n;

/// Per-column text alignment parsed from a GFM delimiter row.
///
/// A delimiter cell with no colons (`---`) parses as [`Alignment::None`]:
/// it pads like [`Alignment::Left`] but the regenerated delimiter carries no
/// colon, preserving the original marker style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Alignment {
    /// `---` with no colons: the GFM default. Pads right, no delimiter colon.
    None,
    /// `:---`: leading colon only. Pads right.
    Left,
    /// `:---:`: colons on both edges. Pads centered.
    Center,
    /// `---:`: trailing colon only. Pads left.
    Right,
}

/// Realign one contiguous GFM table from its raw lines.
///
/// # Parameters
///
/// - `lines`: the table's content only. Each entry is one row with its
///   doc-comment prefix and line terminator already stripped, and every entry
///   is expected to contain a `|`.
/// - Index 0 is the header row, index 1 the delimiter row (parsed by
///   [`parse_delimiter_row`]), and any further entries are body rows. For
///   source like
///
///   ```text
///   /// | name | value |
///   /// | ---- | ----- |
///   /// | a    | 1     |
///   ```
///
///   the caller passes
///
///   ```text
///   [
///       "| name | value |",
///       "| ---- | ----- |",
///       "| a    | 1     |",
///   ]
///   ```
///
/// # Returns
///
/// [`Some`] realigned lines when the entries form a table with a valid
/// delimiter row and realignment produces a different layout, or [`None`]
/// when they are not a table or are already aligned (idempotent fast path).
///
/// # Allocation strategy
///
/// Cells stay borrowed `&str` slices (see [`split_cells_into`]) through the
/// whole parse/measure/compare phase.
///
/// `body` is one flat grid (`ncols` per row, row-major) filled from a single
/// reused `row_buf`, so the parse cost is a **constant** number of allocations
/// regardless of row count - not one `Vec` per row.
///
/// An already-aligned table is then rejected with **zero** per-row `String`
/// allocation: each canonical row is written into one reused buffer
/// ([`build_row_into`] / [`build_delimiter_into`]) and compared in place.
///
/// Only when a change is detected does [`emit_all`] materialize the owned
/// result rows.
pub(crate) fn realign_table(lines: &[&str]) -> Option<Vec<String>> {
    if lines.len() < 2 {
        return None;
    }
    // Every line of a table must carry a pipe.
    if !lines.iter().all(|l| l.contains('|')) {
        return None;
    }

    let header = split_cells(lines[0]);
    if header.is_empty() {
        return None;
    }
    let ncols = header.len();
    let alignments = parse_delimiter_row(lines[1], ncols)?;

    // Build the `body`, which is just string slices of all of the cells.
    let nrows = lines.len() - 2;
    let mut body: Vec<&str> = Vec::with_capacity(nrows.saturating_mul(ncols));
    let mut row_buf: Vec<&str> = Vec::with_capacity(ncols + 2);
    for raw in &lines[2..] {
        row_buf.clear();
        split_cells_into(raw, &mut row_buf);
        drop_border_empties(&mut row_buf);
        // Normalize to exactly `ncols` cells.
        while row_buf.len() < ncols {
            row_buf.push("");
        }
        row_buf.truncate(ncols);
        body.extend_from_slice(&row_buf);
    }

    // Now figure out the required widths for each column.
    let widths = compute_widths(&header, &body, ncols);

    // Phase 1 - change detection: render each line's canonical form into one
    // reused buffer and compare it against the original.
    //
    // The first mismatch drops to Phase 2 ([`emit_all`]); if every line
    // matches, the table is already aligned and we return `None` with no
    // result allocation.
    let rows = body.chunks_exact(ncols);
    let row_len = lines.first().map_or(0, |l| l.len()) + 4;
    let mut buf = String::with_capacity(row_len);
    build_row_into(&header, &widths, &alignments, &mut buf);

    // Did header match?
    if buf != lines[0] {
        return Some(emit_all(&header, &body, ncols, &widths, &alignments));
    }
    buf.clear();

    // Did delimiter match?
    build_delimiter_into(&widths, &alignments, &mut buf);
    if buf != lines[1] {
        return Some(emit_all(&header, &body, ncols, &widths, &alignments));
    }

    // Now check if rows match
    for (row, raw) in rows.zip(&lines[2..]) {
        buf.clear();
        build_row_into(row, &widths, &alignments, &mut buf);
        if buf != *raw {
            return Some(emit_all(&header, &body, ncols, &widths, &alignments));
        }
    }

    // All lines already canonical - nothing to realign.
    None
}

/// Per-column maximum character count across header and all body rows.
fn compute_widths(header: &[&str], body: &[&str], ncols: usize) -> Vec<usize> {
    // `widths[col_index]` is the target width every cell in column `col_index`
    // pads to: the widest cell across the header and every body row.

    // Seed it from the header first so a wide header cell can't be beaten by narrower
    // body cells (which would leave the header narrower than its own column).
    let mut widths = vec![0usize; ncols];
    for (col_index, cell) in header.iter().enumerate() {
        widths[col_index] = widths[col_index].max(char_width(cell));
    }

    // Now seed it from the body.
    // The `body.chunks_exact(ncols)` just creates a row iterator; as the body
    // is a flat grid (`ncols` per row).
    for row in body.chunks_exact(ncols) {
        for (col_index, cell) in row.iter().enumerate() {
            widths[col_index] = widths[col_index].max(char_width(cell));
        }
    }
    widths
}

/// Phase 2 - materialize every realigned row once a change has been detected.
///
/// Reached only when Phase 1 found a mismatch. Each row is written into a
/// fresh, pre-sized `String` by the same builders used for detection, so there
/// is no logic duplication and no intermediate `Vec<String>`/`join` churn.
fn emit_all(
    header: &[&str],
    body: &[&str],
    ncols: usize,
    widths: &[usize],
    alignments: &[Alignment],
) -> Vec<String> {
    let mut out = Vec::with_capacity(2 + body.len() / ncols);
    let cap = row_capacity(widths);
    let mut s = String::with_capacity(cap);
    build_row_into(header, widths, alignments, &mut s);
    out.push(s);
    let mut s = String::with_capacity(cap);
    build_delimiter_into(widths, alignments, &mut s);
    out.push(s);
    for row in body.chunks_exact(ncols) {
        let mut s = String::with_capacity(cap);
        build_row_into(row, widths, alignments, &mut s);
        out.push(s);
    }
    out
}

/// Parse the delimiter row into per-column alignments.
fn parse_delimiter_row(line: &str, ncols: usize) -> Option<Vec<Alignment>> {
    let cells = split_cells(line);
    if cells.len() != ncols {
        return None;
    }
    cells.iter().map(|c| parse_delimiter(c)).collect()
}

/// Append the regenerated delimiter row to `out`.
fn build_delimiter_into(widths: &[usize], alignments: &[Alignment], out: &mut String) {
    out.push_str("| ");
    let mut sep = "";
    for (&w, &a) in widths.iter().zip(alignments) {
        out.push_str(sep);
        write_delimiter_cell(w, a, out);
        sep = " | ";
    }
    out.push_str(" |");
}

/// Append one realigned data row (`| a | b |`) to `out`.
fn build_row_into(cells: &[&str], widths: &[usize], alignments: &[Alignment], out: &mut String) {
    out.push_str("| ");
    let mut sep = "";
    for ((&cell, &w), &a) in cells.iter().zip(widths).zip(alignments) {
        out.push_str(sep);
        write_padded(cell, w, a, out);
        sep = " | ";
    }
    out.push_str(" |");
}

/// Validate `cell` as a GFM delimiter (`:?-+:?`) and return its alignment.
///
/// Returns [`None`] when the cell is not a valid delimiter.
fn parse_delimiter(cell: &str) -> Option<Alignment> {
    let cell = cell.trim();
    let after_leading = cell.strip_prefix(':');
    let has_leading = after_leading.is_some();
    let rest = after_leading.unwrap_or(cell);
    let (core, has_trailing) = match rest.strip_suffix(':') {
        Some(c) => (c, true),
        None => (rest, false),
    };
    if core.is_empty() || !core.bytes().all(|b| b == b'-') {
        return None;
    }
    Some(match (has_leading, has_trailing) {
        (true, true) => Alignment::Center,
        (true, false) => Alignment::Left,
        (false, true) => Alignment::Right,
        (false, false) => Alignment::None,
    })
}

/// Upper bound on a rebuilt row's byte length, for pre-allocation.
fn row_capacity(widths: &[usize]) -> usize {
    // "| " + cells joined with " | " + " |" => 3 chars framing per cell + 1.
    let cells: usize = widths.iter().sum();
    cells + widths.len() * 3 + 1
}

/// Split a single table line into its cells.
///
/// Splits on unescaped `|`, trims surrounding whitespace from each cell, and
/// drops the empty cells produced by the outer pipes. A few examples:
///
/// ```text
/// "| a | b |"   -> ["a", "b"]
/// "|a|b|"       -> ["a", "b"]
/// "  |  a  |  " -> ["a"]
/// "no pipes"    -> ["no pipes"]
/// r"| a\|b | c |" -> ["a\\|b", "c"]   // escaped pipe stays in the cell
/// ```
///
/// Returns borrowed `&str` slices into `line`, so parsing a table allocates
/// only the single outer `Vec` (one pointer per cell) - no per-cell `String`.
fn split_cells(line: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    split_cells_into(line, &mut cells);
    drop_border_empties(&mut cells);
    cells
}

/// Drop the empty cells produced by leading/trailing border pipes, if present.
///
/// For a row like `| a | b |`, [`split_cells_into`] leaves the border pipes as
/// empty cells at both ends:
///
/// ```text
/// ["", "a", "b", ""]
/// ```
///
/// Border cells popped:
///
/// ```text
/// ["a", "b"]
/// ```
///
/// Rows with no outer pipes (`a | b`) have no border empties and are left
/// unchanged. At most one cell is dropped per end; inner empties (e.g. the
/// gap in `a | | c`) stay put.
fn drop_border_empties(cells: &mut Vec<&str>) {
    if cells.first().is_some_and(|c| c.is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|c| c.is_empty()) {
        cells.pop();
    }
}

/// Append every pipe-delimited cell of `line` to `out` (without dropping the
/// border empty cells produced by leading/trailing pipes). Borrows from
/// `line`, so it allocates nothing itself.
///
/// Each cell is trimmed and escaped pipes (`\|`) stay inside their cell rather
/// than acting as a separator. For input `| a | b\|c |`:
///
/// ```text
/// ["", "a", "b\\|c", ""]
/// ```
///
/// The leading/trailing empties come from the outer pipes; pass the result to
/// [`drop_border_empties`] to strip them, as [`split_cells`] does.
fn split_cells_into<'a>(line: &'a str, out: &mut Vec<&'a str>) {
    // `|` (0x7C) and `\` (0x5C) are ASCII, so they never appear inside a
    // multibyte UTF-8 sequence; iterating bytes is therefore sound and lets
    // us slice at every pipe without decoding characters.
    let bytes = line.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            // Skip the backslash and the character it escapes (e.g. `\|`),
            // mirroring the original char-pair consumption without allocating.
            i += 2;
            continue;
        }
        if b == b'|' {
            // SAFETY: `i` sits on an ASCII `|`, which is always a char boundary.
            out.push(line[start..i].trim());
            start = i + 1;
        }
        i += 1;
    }
    out.push(line[start..].trim());
}

/// Write a single delimiter cell of visual width `width` into `out`.
#[inline]
fn write_delimiter_cell(width: usize, alignment: Alignment, out: &mut String) {
    let (leading, trailing) = match alignment {
        Alignment::None => (false, false),
        Alignment::Left => (true, false),
        Alignment::Center => (true, true),
        Alignment::Right => (false, true),
    };
    let mut dashes = width;
    if leading {
        dashes = dashes.saturating_sub(1);
    }
    if trailing {
        dashes = dashes.saturating_sub(1);
    }
    let dashes = dashes.max(1);
    if leading {
        out.push(':');
    }
    out.extend(repeat_n('-', dashes));
    if trailing {
        out.push(':');
    }
}

/// Write a single data cell, padded to `width` per `alignment`, into `out`.
///
/// Uses [`repeat_n`] so padding never allocates a temporary `String`.
#[inline]
fn write_padded(cell: &str, width: usize, alignment: Alignment, out: &mut String) {
    let len = char_width(cell);
    if len >= width {
        out.push_str(cell);
        return;
    }
    let pad = width - len;
    match alignment {
        Alignment::Right => {
            out.extend(repeat_n(' ', pad));
            out.push_str(cell);
        }
        Alignment::Center => {
            // Match the original split: the left side absorbs the odd space.
            let left = pad / 2 + pad % 2;
            let right = pad - left;
            out.extend(repeat_n(' ', left));
            out.push_str(cell);
            out.extend(repeat_n(' ', right));
        }
        Alignment::None | Alignment::Left => {
            out.push_str(cell);
            out.extend(repeat_n(' ', pad));
        }
    }
}

/// Character count of `s`, using the byte length directly when `s` is ASCII
/// (the overwhelmingly common case for table cells), which lets the optimizer
/// avoid decoding UTF-8.
#[inline]
fn char_width(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_for_non_table_missing_delimiter() {
        let lines = ["| a | b |", "| c | d |"];
        assert!(
            realign_table(&lines).is_none(),
            "no delimiter row -> not a table"
        );
    }

    #[test]
    fn none_for_single_line() {
        assert!(realign_table(&["| a | b |"]).is_none());
    }

    #[test]
    fn none_for_line_without_pipe() {
        let lines = ["| a | b |", "no pipe here", "| c | d |"];
        assert!(realign_table(&lines).is_none());
    }

    #[test]
    fn none_for_invalid_delimiter() {
        let lines = ["| a | b |", "| abc | def |", "| g | h |"];
        assert!(
            realign_table(&lines).is_none(),
            "second row not a delimiter -> not a table"
        );
    }

    #[test]
    fn left_aligned_padding() {
        let lines = ["| a | bb |", "| --- | --- |", "| ccc | d |"];
        let out = realign_table(&lines).expect("is a table");
        assert_eq!(out[0], "| a   | bb |");
        assert_eq!(out[1], "| --- | -- |");
        assert_eq!(out[2], "| ccc | d  |");
    }

    #[test]
    fn left_delimiter_keeps_leading_colon() {
        let lines = ["| a |", "| :--- |", "| bcd |"];
        let out = realign_table(&lines).expect("is a table");
        assert_eq!(out[0], "| a   |");
        assert_eq!(out[1], "| :-- |");
        assert_eq!(out[2], "| bcd |");
    }

    #[test]
    fn right_aligned_padding() {
        let lines = ["| a | b |", "| ---: | ---: |", "| cc | dd |"];
        let out = realign_table(&lines).expect("is a table");
        assert_eq!(out[0], "|  a |  b |");
        assert_eq!(out[1], "| -: | -: |");
        assert_eq!(out[2], "| cc | dd |");
    }

    #[test]
    fn center_aligned_padding() {
        let lines = ["| a |", "| :---: |", "| bcde |"];
        let out = realign_table(&lines).expect("is a table");
        assert_eq!(out[0], "|   a  |");
        assert_eq!(out[1], "| :--: |");
        assert_eq!(out[2], "| bcde |");
    }

    #[test]
    fn escaped_pipe_not_split() {
        // `a\|b` is one cell; column count stays 1.
        let lines = ["| a\\|b |", "| --- |", "| c |"];
        let out = realign_table(&lines).expect("is a table");
        assert_eq!(out[0], "| a\\|b |");
        assert_eq!(out[1], "| ---- |");
        assert_eq!(out[2], "| c    |");
    }

    #[test]
    fn ragged_rows_normalized() {
        let lines = ["| a | b |", "| --- | --- |", "| c | d | e |", "| f |"];
        let out = realign_table(&lines).expect("is a table");
        // header has 2 cols; row 3 drops excess, row 4 pads with empty.
        assert_eq!(out.len(), 4);
        assert_eq!(out[2], "| c | d |");
        assert_eq!(out[3], "| f |   |");
    }

    #[test]
    fn realigned_table_is_idempotent() {
        let lines = ["| a | bb |", "| --- | --- |", "| ccc | d |"];
        let once = realign_table(&lines).expect("first pass realigns");
        let once_refs: Vec<&str> = once.iter().map(String::as_str).collect();
        assert!(
            realign_table(&once_refs).is_none(),
            "already-aligned table should return None (idempotent)"
        );
    }

    #[test]
    fn split_cells_basics() {
        assert_eq!(split_cells("| a | bb |"), ["a", "bb"]);
        assert_eq!(split_cells("|a|b|"), ["a", "b"]);
        assert_eq!(split_cells("  |  a  |  "), ["a"]);
        assert_eq!(split_cells("no pipes"), ["no pipes"]);
        // escaped pipe stays in the cell
        assert_eq!(split_cells(r"| a\|b | c |"), ["a\\|b", "c"]);
    }

    #[test]
    fn split_cells_trailing_backslash() {
        // A lone trailing backslash is preserved in the final cell.
        assert_eq!(split_cells("| a |\\"), ["a", "\\"]);
    }

    #[test]
    fn split_cells_into_appends_all_cells() {
        // split_cells_into keeps border empties; split_cells drops them.
        let mut cells = Vec::new();
        split_cells_into("| a | b |", &mut cells);
        assert_eq!(cells, ["", "a", "b", ""]);
    }

    #[test]
    fn none_for_ragged_delimiter_column_count() {
        // header has 2 cols, delimiter claims 1 -> not a (canonical) table.
        let lines = ["| a | b |", "| --- |", "| c | d |"];
        assert!(realign_table(&lines).is_none());
    }

    #[test]
    fn char_width_ascii_fast_path() {
        assert_eq!(char_width("abc"), 3);
        // non-ASCII: 'é' is two bytes, one char.
        assert_eq!(char_width("aé"), 2);
    }
}
