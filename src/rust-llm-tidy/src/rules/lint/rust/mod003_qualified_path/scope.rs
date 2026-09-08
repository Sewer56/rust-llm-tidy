//! The scope data model and its queries.
//!
//! [`ScopeFrame`] holds the imports and names one lexical scope binds.
//! The queries decide whether a short name is mentioned or shadowed,
//! which import covers a path, and whether a path's root is explicit.

use super::ROOT_SEGMENTS;

/// The imports and names one lexical scope introduces during the walk.
#[derive(Default)]
pub(super) struct ScopeFrame<'a> {
    pub(super) imports: Vec<Import<'a>>,
    pub(super) bindings: Vec<Binding<'a>>,
    /// Explicit external crate names visible throughout this scope.
    pub(super) external_crates: Vec<&'a str>,
    /// A glob may introduce a relative module with a known root's name.
    pub(super) has_glob: bool,
    /// Module boundaries stop lexical lookup of external crate declarations.
    pub(super) module_scope: bool,
}

/// One name a lexical scope binds: a local binding (`let`,
/// parameter, pattern) or a same-scope item.
///
/// `start == 0` marks an item: visible throughout its scope. Any
/// other `start` is the binding's byte offset; a `let` shadows only
/// from that position onward.
pub(super) struct Binding<'a> {
    pub(super) name: &'a str,
    pub(super) start: usize,
}

/// One explicit `use` import: the full imported path plus the name it
/// binds in scope (the alias for `use a::b::C as D`).
pub(super) struct Import<'a> {
    pub(super) segments: Vec<&'a str>,
    pub(super) short: &'a str,
}

/// The longest in-scope import covering `segments`: its path equals
/// the occurrence path or prefixes it.
///
/// The innermost frame wins ties, since its binding shadows outer
/// ones. One-segment imports never cover: their suggestion would
/// repeat the occurrence verbatim (`use std;` covering `std::mem`).
pub(super) fn covering_import<'frames, 'a>(
    scopes: &'frames [ScopeFrame<'a>],
    segments: &[&str],
) -> Option<&'frames Import<'a>> {
    let mut covering: Option<&Import<'a>> = None;
    for frame in scopes.iter().rev() {
        for import in &frame.imports {
            let root_len = usize::from(import.segments.first() == Some(&"")) + 1;
            let covers = import.segments.len() > root_len
                && segments.starts_with(import.segments.as_slice());
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
/// items anywhere, a `let` from its position onward.
pub(super) fn frame_mentions(frame: &ScopeFrame<'_>, short: &str, occurrence_start: usize) -> bool {
    frame.imports.iter().any(|import| import.short == short)
        || frame.bindings.iter().any(|binding| {
            binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
        })
}

/// True when `frame` binds `short` to something other than
/// `segments`: a local binding, or an import of a different path
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

/// Accept explicit roots and unshadowed crate names, never import-relative paths.
pub(super) fn is_full_path(scopes: &[ScopeFrame<'_>], segments: &[&str], start: usize) -> bool {
    let Some(root) = segments.first().copied() else {
        return false;
    };
    if root.is_empty() {
        return segments.len() >= 3;
    }
    if segments.len() < 2 || matches!(root, "self" | "super") {
        return false;
    }
    if root == "crate" {
        return true;
    }

    for frame in scopes.iter().rev() {
        if frame.has_glob
            || frame.bindings.iter().any(|binding| {
                binding.name.trim_start_matches("r#") == root
                    && (binding.start == 0 || binding.start <= start)
            })
            || frame.imports.iter().any(|import| {
                import.short.trim_start_matches("r#") == root
                    && import.segments.as_slice() != [root]
            })
        {
            return false;
        }
        if frame.external_crates.contains(&root) {
            return true;
        }
        if frame.module_scope {
            break;
        }
    }

    ROOT_SEGMENTS.contains(&root)
}
