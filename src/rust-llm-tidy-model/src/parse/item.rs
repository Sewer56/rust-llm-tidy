//! Data model for parsed source items.
//!
//! Holds the value types produced by parsing a Rust source file: the kind,
//! name, visibility, doc comments, and span of each top-level item, plus the
//! container that bundles them with the original source text and
//! preamble/trailer offsets.

use std::fmt;

/// The result of parsing a source file.
pub struct ParseResult {
    /// The parsed items in file order.
    pub items: Vec<SourceItem>,
    /// The original source text.
    pub source: String,
    /// The parsed tree-sitter syntax tree, retained so downstream passes (e.g.
    /// the reorder reference-graph walk) can reuse it instead of re-parsing
    /// `source`.
    /// `pub(crate)`: read it through [`ParseResult::syntax_tree`].
    pub(crate) tree: tree_sitter::Tree,
    /// Byte offset where the preamble ends.
    ///
    /// The preamble is everything before the first top-level item. It may
    /// include:
    ///
    /// - leading comments,
    /// - `//!` module docs,
    /// - license headers,
    /// - inner attributes (`#![...]`).
    ///
    /// Emitted verbatim before any reordered items. `0` when an item starts
    /// at offset 0 (no preamble).
    pub preamble_end: usize,
    /// Byte offset where the trailer begins.
    ///
    /// The trailer is everything after the last top-level item's trailing
    /// newline. It may include:
    ///
    /// - trailing comments,
    /// - the final newline,
    /// - trailing whitespace.
    ///
    /// Emitted verbatim after the reordered items. Equal to `source.len()`
    /// when the file has no trailing content.
    pub trailer_start: usize,
}

/// A single top-level item in the parsed source file.
#[derive(Debug, Clone)]
pub struct SourceItem {
    /// Byte offset of the start of this item (including prefix comments/attrs).
    pub start: usize,
    /// Byte offset of the end of this item.
    pub end: usize,
    /// 1-based source line where this item starts (including prefix
    /// comments/attrs). Precomputed at parse time so lint checks need not rescan
    /// the source for each diagnostic.
    start_line: usize,
    /// The kind of this item.
    kind: ItemKind,
    /// The name of this item (if it has one).
    name: Option<String>,
    /// For impl blocks, the target type name.
    impl_target: Option<String>,
    /// True if this is a `mod` item gated by `#[cfg(test)]`.
    is_test_module: bool,
    /// True only for inline `mod x { ... }` definitions (body present); false
    /// for file-based `mod x;` declarations and every non-mod item.
    is_inline: bool,
    /// True for `impl Trait for Type` (trait impl), false for `impl Type` (inherent).
    is_trait_impl: bool,
    /// Visibility tier used for ordering and doc-coverage checks. `Some` for
    /// every item kind that has a visibility modifier (fn, struct, enum, etc.).
    visibility: Option<VisibilityTier>,
    /// Text of each leading `///` (or `#[doc = "..."]`) line for this item,
    /// in source order. Each entry preserves syn's value (so a `/// foo` line
    /// yields `" foo"`). Empty when the item has no doc comment.
    doc_comments: Vec<String>,
    /// True for fn items whose return type path ends in `Result` (i.e. a
    /// `-> Result<...>` signature). `false` for non-fn items and fns that do
    /// not return `Result`.
    returns_result: bool,
    /// Named parameter idents of a fn, excluding `self`/`&self`/`&mut self`.
    /// Empty for non-fn items.
    params: Vec<String>,
    /// True for fn items carrying a `#[test]` or `#[...::test]` attribute.
    is_test_fn: bool,
}

/// Kind of a parsed source item.
///
/// Each variant is shown with the minimal Rust syntax that produces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A free function.
    ///
    /// ```rust,ignore
    /// fn main() {}
    /// ```
    Fn,
    /// A struct definition.
    ///
    /// ```rust,ignore
    /// struct Foo { x: i32 }
    /// ```
    Struct,
    /// An enum definition.
    ///
    /// ```rust,ignore
    /// enum Color { Red, Green, Blue }
    /// ```
    Enum,
    /// A type alias.
    ///
    /// ```rust,ignore
    /// type Point = (i32, i32);
    /// ```
    Type,
    /// A union definition.
    ///
    /// ```rust,ignore
    /// union Bytes { as_u32: u32, as_bytes: [u8; 4] }
    /// ```
    Union,
    /// An inherent or trait impl block.
    ///
    /// ```rust,ignore
    /// impl Foo {}
    /// impl Default for Foo { fn default() -> Self { Self {} } }
    /// ```
    Impl,
    /// A `use` import.
    ///
    /// ```rust,ignore
    /// use std::io;
    /// ```
    Use,
    /// A `const` item.
    ///
    /// ```rust,ignore
    /// const MAX: u32 = 100;
    /// ```
    Const,
    /// A `static` item.
    ///
    /// ```rust,ignore
    /// static COUNTER: AtomicUsize = AtomicUsize::new(0);
    /// ```
    Static,
    /// A module declaration or inline module.
    ///
    /// ```rust,ignore
    /// mod foo;
    /// mod bar {}
    /// ```
    Mod,
    /// An `extern crate` declaration.
    ///
    /// ```rust,ignore
    /// extern crate serde;
    /// ```
    Extern,
    /// A trait definition.
    ///
    /// ```rust,ignore
    /// trait Draw { fn render(&self); }
    /// ```
    Trait,
    /// A `macro_rules!` definition.
    ///
    /// ```rust,ignore
    /// macro_rules! say_hello { () => { println!("hi"); }; }
    /// ```
    Macro,
    /// A top-level macro invocation (e.g. `foo!();`) that is not a
    /// `macro_rules!` definition. Named after the last path segment so the
    /// graph stage can pair it with its local `macro_rules!` definition.
    ///
    /// ```rust,ignore
    /// println!("x");
    /// ```
    MacroInvocation,
    /// Any other top-level item not covered above (foreign modules, trait
    /// aliases, verbatim items).
    ///
    /// ```rust,ignore
    /// extern "C" { fn f(); }
    /// ```
    Other,
}

/// Visibility classification for ordering items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VisibilityTier {
    /// `pub` - fully public
    Pub,
    /// `pub(crate)`, `pub(super)`, `pub(in path)` - restricted
    PubRestricted,
    /// No visibility modifier (private / inherited)
    Private,
}

impl ParseResult {
    /// The parsed [`tree_sitter::Tree`], reused from parsing so downstream
    /// passes avoid a second parse of [`ParseResult::source`].
    pub fn syntax_tree(&self) -> &tree_sitter::Tree {
        &self.tree
    }
}

impl SourceItem {
    /// The kind of this item.
    #[inline]
    pub fn kind(&self) -> &ItemKind {
        &self.kind
    }

    /// The name of this item, if any.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// True if this item is a function.
    #[inline]
    pub fn is_fn(&self) -> bool {
        self.kind == ItemKind::Fn
    }

    /// The target type name for an impl block, if any.
    #[inline]
    pub fn impl_target_name(&self) -> Option<&str> {
        self.impl_target.as_deref()
    }

    /// True if this is a `mod` item gated by `#[cfg(test)]`.
    #[inline]
    pub fn is_test_module(&self) -> bool {
        self.is_test_module
    }

    /// True only for inline `mod x { ... }` definitions (body present); false
    /// for file-based `mod x;` declarations and every non-mod item.
    #[inline]
    pub fn is_inline(&self) -> bool {
        self.is_inline
    }

    /// True for `impl Trait for Type` (trait impl), false for `impl Type` (inherent).
    #[inline]
    pub fn is_trait_impl(&self) -> bool {
        self.is_trait_impl
    }

    /// Visibility tier for this item.
    ///
    /// Returns `Some` for every item kind that carries a visibility modifier
    /// (fn, struct, enum, union, type, const, static, mod, trait, use, extern
    /// crate), and `None` for kinds without one (impl, macro, macro
    /// invocation, other).
    #[inline]
    pub fn visibility(&self) -> Option<VisibilityTier> {
        self.visibility
    }

    /// The leading doc-comment lines for this item, in source order.
    ///
    /// Each entry is the raw value of a `#[doc = "..."]` attribute (so a
    /// `/// foo` line yields `" foo"`). Empty when the item has no doc
    /// comment.
    pub fn doc_comments(&self) -> &[String] {
        &self.doc_comments
    }

    /// True for fn items whose return type path ends in `Result`
    /// (a `-> Result<...>` signature).
    #[inline]
    pub fn returns_result(&self) -> bool {
        self.returns_result
    }

    /// Named parameter idents of a fn, excluding `self`/`&self`/`&mut self`.
    ///
    /// Empty for non-fn items. For fns with destructuring parameter patterns,
    /// only simple `Pat::Ident` names are reported.
    #[inline]
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// True for fn items carrying a `#[test]` or `#[...::test]` attribute.
    #[inline]
    pub fn is_test_fn(&self) -> bool {
        self.is_test_fn
    }

    /// 1-based source line where this item starts (including prefix
    /// comments/attrs).
    #[inline]
    pub fn start_line(&self) -> usize {
        self.start_line
    }

    #[allow(clippy::too_many_arguments)]
    /// Creates a new `SourceItem`.
    ///
    /// See the [`SourceItem`] struct field docs for parameter descriptions:
    /// `start`, `end`, `start_line`, `kind`, `name`, `impl_target`,
    /// `is_test_module`, `is_inline`, `is_trait_impl`, `visibility`,
    /// `doc_comments`, `returns_result`, `params`, and `is_test_fn`.
    pub fn new(
        start: usize,
        end: usize,
        start_line: usize,
        kind: ItemKind,
        name: Option<String>,
        impl_target: Option<String>,
        is_test_module: bool,
        is_inline: bool,
        is_trait_impl: bool,
        visibility: Option<VisibilityTier>,
        doc_comments: Vec<String>,
        returns_result: bool,
        params: Vec<String>,
        is_test_fn: bool,
    ) -> Self {
        Self {
            start,
            end,
            start_line,
            kind,
            name,
            impl_target,
            is_test_module,
            is_inline,
            is_trait_impl,
            visibility,
            doc_comments,
            returns_result,
            params,
            is_test_fn,
        }
    }
}

impl ItemKind {
    /// The stable `&'static str` form of this kind (e.g. `"fn"`), shared by
    /// [`Display`] and structured change/diagnostic reporting so
    /// records can hold the kind without an owned allocation.
    /// [`Display`]: fmt::Display
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Type => "type",
            ItemKind::Union => "union",
            ItemKind::Impl => "impl",
            ItemKind::Use => "use",
            ItemKind::Const => "const",
            ItemKind::Static => "static",
            ItemKind::Mod => "mod",
            ItemKind::Extern => "extern",
            ItemKind::Trait => "trait",
            ItemKind::Macro => "macro",
            ItemKind::MacroInvocation => "macro-invocation",
            ItemKind::Other => "other",
        }
    }
}

impl fmt::Debug for ParseResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseResult")
            .field("items", &self.items)
            .field("source", &self.source)
            .field("preamble_end", &self.preamble_end)
            .field("trailer_start", &self.trailer_start)
            // `tree` is intentionally omitted: its `Debug` output is verbose and
            // the syntax tree is already represented by `items` + `source`.
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
