//! GFM table alignment: split, validate, and re-pad table columns.
//!
//! [`realign_table`] takes the raw (prefix-stripped) lines of a single
//! contiguous table and returns the canonically aligned lines, or [`None`]
//! when the lines are not a table or are already aligned.

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

/// Realign one contiguous GFM table from its raw (prefix-stripped) lines.
///
/// Returns [`Some`] realigned lines when the lines form a table with a valid
/// delimiter row and realignment produces a different layout, or [`None`]
/// when the lines are not a table or are already aligned (idempotent fast
/// path).
pub(crate) fn realign_table(lines: &[&str]) -> Option<Vec<String>> {
    if lines.len() < 2 {
        return None;
    }
    if !lines.iter().all(|l| l.contains('|')) {
        return None;
    }

    let header = split_cells(lines[0]);
    if header.is_empty() {
        return None;
    }
    let ncols = header.len();
    let alignments = parse_delimiter_row(lines[1], ncols)?;

    let mut body: Vec<Vec<String>> = Vec::with_capacity(lines.len().saturating_sub(2));
    for raw in &lines[2..] {
        let mut row = split_cells(raw);
        normalize_row(&mut row, ncols);
        body.push(row);
    }

    let widths = compute_widths(&header, &body, ncols);
    let delimiter = build_delimiter_row(&widths, &alignments);
    let out_header = build_row(&header, &widths, &alignments);

    let mut out = Vec::with_capacity(lines.len());
    out.push(out_header);
    out.push(delimiter);
    for row in &body {
        out.push(build_row(row, &widths, &alignments));
    }

    if out.iter().map(String::as_str).eq(lines.iter().copied()) {
        return None;
    }
    Some(out)
}

/// Build the regenerated delimiter row.
fn build_delimiter_row(widths: &[usize], alignments: &[Alignment]) -> String {
    let cells: Vec<String> = widths
        .iter()
        .zip(alignments.iter())
        .map(|(&w, &a)| build_delimiter_cell(w, a))
        .collect();
    format!("| {} |", cells.join(" | "))
}

/// Reassemble `cells` into a `| a | b |` row.
fn build_row(cells: &[String], widths: &[usize], alignments: &[Alignment]) -> String {
    let padded: Vec<String> = cells
        .iter()
        .zip(widths.iter())
        .zip(alignments.iter())
        .map(|((cell, &w), &a)| pad_cell(cell, w, a))
        .collect();
    format!("| {} |", padded.join(" | "))
}

/// Per-column maximum character count across header and all body rows.
fn compute_widths(header: &[String], body: &[Vec<String>], ncols: usize) -> Vec<usize> {
    let mut widths = vec![0usize; ncols];
    for (i, cell) in header.iter().enumerate() {
        widths[i] = widths[i].max(cell.chars().count());
    }
    for row in body {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    widths
}

/// Pad or truncate `row` to exactly `ncols` cells.
fn normalize_row(row: &mut Vec<String>, ncols: usize) {
    while row.len() < ncols {
        row.push(String::new());
    }
    row.truncate(ncols);
}

/// Parse the delimiter row into per-column alignments.
fn parse_delimiter_row(line: &str, ncols: usize) -> Option<Vec<Alignment>> {
    let cells = split_cells(line);
    if cells.len() != ncols {
        return None;
    }
    cells.iter().map(|c| parse_delimiter(c)).collect()
}

/// Build a single delimiter cell of visual width `width`.
fn build_delimiter_cell(width: usize, alignment: Alignment) -> String {
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
    let mut cell = String::with_capacity(width);
    if leading {
        cell.push(':');
    }
    cell.push_str(&"-".repeat(dashes));
    if trailing {
        cell.push(':');
    }
    cell
}

/// Pad `cell` to `width` characters according to its alignment.
fn pad_cell(cell: &str, width: usize, alignment: Alignment) -> String {
    let len = cell.chars().count();
    if len >= width {
        return cell.to_string();
    }
    let pad = width - len;
    match alignment {
        Alignment::Right => format!("{}{}", " ".repeat(pad), cell),
        Alignment::Center => {
            let left = " ".repeat(pad / 2 + pad % 2);
            let right = " ".repeat(pad / 2);
            format!("{left}{cell}{right}")
        }
        Alignment::None | Alignment::Left => format!("{cell}{}", " ".repeat(pad)),
    }
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
    if core.is_empty() || !core.chars().all(|c| c == '-') {
        return None;
    }
    Some(match (has_leading, has_trailing) {
        (true, true) => Alignment::Center,
        (true, false) => Alignment::Left,
        (false, true) => Alignment::Right,
        (false, false) => Alignment::None,
    })
}

/// Split a single table line into its cells.
///
/// Splits on unescaped `|` (a `\|` stays inside the cell), trims surrounding
/// whitespace from each cell, and drops the empty cells produced by the outer
/// pipes (`| a | b |` -> `["a", "b"]`).
fn split_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            current.push(c);
            if let Some(next) = chars.next() {
                current.push(next);
            }
            continue;
        }
        if c == '|' {
            cells.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(c);
        }
    }
    cells.push(current.trim().to_string());

    if cells.first().is_some_and(|c| c.is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|c| c.is_empty()) {
        cells.pop();
    }
    cells
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
}
