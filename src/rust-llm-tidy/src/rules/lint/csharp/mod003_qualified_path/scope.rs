//! The scope data model and its queries.
//!
//! [`ScopeFrame`] holds the imports and names one lexical scope binds.
//! The queries decide whether a name is mentioned or shadowed, and
//! which import covers a path.

/// The imports and names one lexical scope introduces during the walk.
#[derive(Default)]
pub(super) struct ScopeFrame<'a> {
    pub(super) imports: Vec<Import<'a>>,
    pub(super) bindings: Vec<Binding<'a>>,
}

/// One name a lexical scope binds: a local designation (parameter,
/// variable, foreach or catch name) or a same-scope declaration.
///
/// `start == 0` marks a declaration: visible throughout its scope.
/// Any other `start` is the binding's byte offset; a local shadows
/// only from that position onward.
pub(super) struct Binding<'a> {
    pub(super) name: &'a str,
    pub(super) start: usize,
}

/// One explicit `using` import: its full path, its bound name (an
/// alias, else the last segment), and whether an alias binds it.
///
/// A plain, non-aliased directive binds nothing itself: it opens its
/// namespace so the namespace's members read unqualified.
pub(super) struct Import<'a> {
    pub(super) segments: Vec<&'a str>,
    pub(super) short: &'a str,
    pub(super) aliased: bool,
    /// Whether the directive bypasses relative name resolution with `global::`.
    pub(super) absolute: bool,
}

/// The longest in-scope import covering `segments`: its path equals
/// the occurrence path or prefixes it.
///
/// The innermost frame wins ties, since its binding shadows outer
/// ones. Absolute occurrences only reuse absolute imports, avoiding relative
/// targets that happen to have the same spelling.
///
/// Root namespace imports such as `using System;` also cover longer paths.
pub(super) fn covering_import<'frames, 'a>(
    scopes: &'frames [ScopeFrame<'a>],
    segments: &[&str],
    absolute: bool,
) -> Option<&'frames Import<'a>> {
    let mut covering: Option<&Import<'a>> = None;
    for frame in scopes.iter().rev() {
        for import in &frame.imports {
            let covers =
                segments.starts_with(import.segments.as_slice()) && (!absolute || import.absolute);
            if covers
                && covering
                    .as_ref()
                    .is_none_or(|best| import.segments.len() > best.segments.len())
            {
                covering = Some(import);
            }
        }
    }
    covering
}

/// True when `frame` binds `short` at `occurrence_start`.
///
/// Counts an import of that name, plus bindings in scope there:
/// declarations anywhere, a local from its position onward.
pub(super) fn frame_mentions(frame: &ScopeFrame<'_>, short: &str, occurrence_start: usize) -> bool {
    frame.imports.iter().any(|import| import.short == short)
        || frame.bindings.iter().any(|binding| {
            binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
        })
}

/// True when `frame` binds `short` to something other than
/// `segments`: a local designation, or an import of a different path
/// under the same name.
///
/// Either shadows or ambiguates an imported-name suggestion.
pub(super) fn frame_shadows(
    frame: &ScopeFrame<'_>,
    short: &str,
    segments: &[&str],
    occurrence_start: usize,
) -> bool {
    frame.bindings.iter().any(|binding| {
        binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
    }) || frame
        .imports
        .iter()
        .any(|import| import.short == short && import.segments.as_slice() != segments)
}
