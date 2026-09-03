//! The doc-region input shape the measuring core consumes.
//!
//! A [`DocRegion`] is a contiguous run of doc lines sharing one dialect.
//!
//! Producers - the lint crate's own [`line_markers`] or an AST backend's
//! doc-region walk - strip each line's comment marker and indent, keep
//! its original line number, and group the lines into regions.
//!
//! [`line_markers`]: super::line_markers

/// A contiguous run of doc lines sharing one dialect, in source order.
#[derive(Debug, PartialEq, Eq)]
pub struct DocRegion {
    /// The dialect the region's lines are measured with.
    pub dialect: Dialect,
    /// The region's doc lines in source order.
    pub lines: Vec<RegionLine>,
}

/// The dialect a [`DocRegion`] is measured with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Markdown prose: fences, indented code, exempt content, and bullet
    /// segmentation over the stripped text.
    Markdown,
    /// XML doc comments: only the inner text of text nodes is measured,
    /// tags and attribute values vanish, `<code>` and `<example>`
    /// subtrees are exempt, and paragraphs never join across tags.
    XmlDoc,
}

/// One doc line after the producer stripped its comment marker and indent.
#[derive(Debug, PartialEq, Eq)]
pub struct RegionLine {
    /// 1-based original file line number.
    pub number: usize,
    /// The stripped text: line ending, indent, and comment marker removed.
    pub text: String,
    /// Whether the line counts as indented code: a tab or 4-space lead in
    /// the stripped text for marker languages, or a raw indent of at least
    /// 4 spaces in marker-less files.
    pub indented: bool,
}
