//! The doc-region input shape the measuring core consumes.
//!
//! A [`DocRegion`] is a contiguous run of doc lines sharing one dialect.
//! Producers (see [`line_markers`]) strip each line's comment marker and
//! indent, keep its original line number, and group the lines into regions.
//!
//! [`line_markers`]: super::line_markers

/// A contiguous run of doc lines sharing one dialect, in source order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DocRegion {
    /// The dialect the region's lines are measured with.
    pub dialect: Dialect,
    /// The region's doc lines in source order.
    pub lines: Vec<RegionLine>,
}

/// The dialect a [`DocRegion`] is measured with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// Markdown prose: fences, indented code, exempt content, and bullet
    /// segmentation over the stripped text.
    Markdown,
}

/// One doc line after the producer stripped its comment marker and indent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RegionLine {
    /// 1-based original file line number.
    pub number: usize,
    /// The stripped text: line ending, indent, and comment marker removed.
    pub text: String,
    /// Whether the line counts as indented code: a tab or 4-space lead in
    /// the stripped text for marker languages, or a raw indent of at least
    /// 4 spaces in marker-less files.
    pub indented: bool,
}
