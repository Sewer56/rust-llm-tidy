//! Classification of `syn` AST items into reordering categories.
//!
//! Maps each top-level [`syn::Item`] to an [`ItemKind`], extracts its name and
//! impl target, derives the visibility tier, captures its leading doc comments,
//! and (for functions) whether it returns `Result`. These classifications feed
//! the parse orchestration that builds source items.

use crate::parse::item::{ItemKind, VisibilityTier};
use quote::ToTokens;

/// Result of classifying a single top-level item.
pub(super) struct Classification {
    pub(super) kind: ItemKind,
    pub(super) name: Option<String>,
    pub(super) impl_target: Option<String>,
    pub(super) is_test_module: bool,
    pub(super) is_trait_impl: bool,
    pub(super) visibility: Option<VisibilityTier>,
    pub(super) doc_comments: Vec<String>,
    pub(super) returns_result: bool,
    /// Named parameter idents of a fn, excluding `self`/`&self`/`&mut self`.
    /// Empty for non-fn items.
    pub(super) params: Vec<String>,
    /// True for fn items carrying a `#[test]` or `#[...::test]` attribute.
    pub(super) is_test_fn: bool,
}

/// Classify a `syn::Item` into a [`Classification`].
pub(super) fn classify_item(item: &syn::Item, source: &str, item_start: usize) -> Classification {
    match item {
        syn::Item::Fn(f) => {
            let name = f.sig.ident.to_string();
            let vis = classify_visibility(&f.vis);
            Classification {
                kind: ItemKind::Fn,
                name: Some(name),
                impl_target: None,
                is_test_module: false,
                is_trait_impl: false,
                visibility: Some(vis),
                doc_comments: extract_doc_from_attrs(&f.attrs),
                returns_result: returns_result(&f.sig),
                params: extract_param_names(&f.sig),
                is_test_fn: is_test_fn(&f.attrs),
            }
        }
        syn::Item::Struct(s) => Classification {
            kind: ItemKind::Struct,
            name: Some(s.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&s.vis)),
            doc_comments: extract_doc_from_attrs(&s.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Enum(e) => Classification {
            kind: ItemKind::Enum,
            name: Some(e.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&e.vis)),
            doc_comments: extract_doc_from_attrs(&e.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Union(u) => Classification {
            kind: ItemKind::Union,
            name: Some(u.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&u.vis)),
            doc_comments: extract_doc_from_attrs(&u.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Type(t) => Classification {
            kind: ItemKind::Type,
            name: Some(t.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&t.vis)),
            doc_comments: extract_doc_from_attrs(&t.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Impl(i) => {
            let impl_target_name = path_type_to_string(&i.self_ty);
            let is_trait = i.trait_.is_some();
            Classification {
                kind: ItemKind::Impl,
                name: None,
                impl_target: Some(impl_target_name),
                is_test_module: false,
                is_trait_impl: is_trait,
                visibility: None,
                doc_comments: extract_doc_from_attrs(&i.attrs),
                returns_result: false,
                params: Vec::new(),
                is_test_fn: false,
            }
        }
        syn::Item::Use(u) => Classification {
            kind: ItemKind::Use,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&u.vis)),
            doc_comments: extract_doc_from_attrs(&u.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Const(c) => Classification {
            kind: ItemKind::Const,
            name: Some(c.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&c.vis)),
            doc_comments: extract_doc_from_attrs(&c.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Static(s) => Classification {
            kind: ItemKind::Static,
            name: Some(s.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&s.vis)),
            doc_comments: extract_doc_from_attrs(&s.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Mod(m) => {
            let is_test = m.attrs.iter().any(|attr| {
                attr.path().is_ident("cfg")
                    && attr
                        .meta
                        .require_list()
                        .ok()
                        .is_some_and(|list| list.tokens.to_string().trim() == "test")
            });
            Classification {
                kind: ItemKind::Mod,
                name: Some(m.ident.to_string()),
                impl_target: None,
                is_test_module: is_test,
                is_trait_impl: false,
                visibility: Some(classify_visibility(&m.vis)),
                doc_comments: extract_doc_from_attrs(&m.attrs),
                returns_result: false,
                params: Vec::new(),
                is_test_fn: false,
            }
        }
        syn::Item::ExternCrate(e) => Classification {
            kind: ItemKind::Extern,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&e.vis)),
            doc_comments: extract_doc_from_attrs(&e.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Trait(t) => Classification {
            kind: ItemKind::Trait,
            name: Some(t.ident.to_string()),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: Some(classify_visibility(&t.vis)),
            doc_comments: extract_doc_from_attrs(&t.attrs),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        syn::Item::Macro(m) if m.mac.path.is_ident("macro_rules") => {
            // syn stores the macro body in `mac.tokens`; the name lives just
            // after `macro_rules!` in the source text.
            let name = extract_macro_rules_name(source, item_start);
            Classification {
                kind: ItemKind::Macro,
                name,
                impl_target: None,
                is_test_module: false,
                is_trait_impl: false,
                visibility: None,
                doc_comments: extract_doc_from_attrs(&m.attrs),
                returns_result: false,
                params: Vec::new(),
                is_test_fn: false,
            }
        }
        syn::Item::Macro(m) => {
            // Any other `Item::Macro` is a macro invocation (e.g.
            // `synthetic_fixture!(x);`, `println!()`, `tokio::main`). The name
            // is the last path segment; the graph stage decides placement
            // based on whether a local `macro_rules!` definition shares it.
            // External macros (no local def) stay in the stable "other" phase.
            let name = m.mac.path.segments.last().map(|s| s.ident.to_string());
            Classification {
                kind: ItemKind::MacroInvocation,
                name,
                impl_target: None,
                is_test_module: false,
                is_trait_impl: false,
                visibility: None,
                doc_comments: extract_doc_from_attrs(&m.attrs),
                returns_result: false,
                params: Vec::new(),
                is_test_fn: false,
            }
        }
        _ => Classification {
            kind: ItemKind::Other,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: None,
            doc_comments: Vec::new(),
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
    }
}

fn classify_visibility(vis: &syn::Visibility) -> VisibilityTier {
    match vis {
        syn::Visibility::Public(_) => VisibilityTier::Pub,
        syn::Visibility::Restricted(restricted) => {
            // `pub(crate)`, `pub(super)`, `pub(in path)`
            // Heuristic: pub(crate), pub(super), and pub(in ...) all map to PubRestricted.
            // String-matching "crate", "super", or "in " covers all practical cases.
            let path_str = restricted.path.to_token_stream().to_string();
            if path_str == "crate" || path_str == "super" || path_str.contains("in ") {
                VisibilityTier::PubRestricted
            } else {
                // Shouldn't happen; treat as restricted
                VisibilityTier::PubRestricted
            }
        }
        syn::Visibility::Inherited => VisibilityTier::Private,
    }
}

/// Extract the text of each `#[doc = "..."]` attribute (one per `///` line).
///
/// syn parses `/// foo` into `#[doc = " foo"]`; the returned strings preserve
/// syn's value verbatim (leading space and all).
fn extract_doc_from_attrs(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            docs.push(s.value());
        }
    }
    docs
}

/// Extract the macro name following `macro_rules!` in the source text.
fn extract_macro_rules_name(source: &str, item_start: usize) -> Option<String> {
    let slice = &source[item_start..];
    let pos = slice.find("macro_rules!")?;
    let rest = &slice[pos + "macro_rules!".len()..];
    let trimmed = rest.trim_start();
    let end = trimmed
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(trimmed.len());
    let name = &trimmed[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Extract named parameter idents from a fn signature, excluding the implicit
/// `self`/`&self`/`&mut self` receiver.
///
/// Only `Pat::Ident` patterns are reported (the common case); destructuring
/// patterns like `(a, b): (u32, u32)` contribute nothing, which is acceptable
/// because doc-check only needs simple name coverage.
fn extract_param_names(sig: &syn::Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => match &*pat_type.pat {
                syn::Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
                _ => None,
            },
            syn::FnArg::Receiver(_) => None,
        })
        .collect()
}

/// True when `attrs` contains a `#[test]` or `#[...::test]` attribute.
///
/// Matching the last path segment covers both `#[test]` and framework variants
/// like `#[tokio::test]`.
fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

fn path_type_to_string(path: &syn::Type) -> String {
    if let syn::Type::Path(type_path) = path {
        type_path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    } else {
        String::new()
    }
}

/// True when `sig` declares a `-> Result<...>` return type.
///
/// Matches by the final path segment name so any `Result` (std, io, a custom
/// error result, etc.) is detected regardless of path prefix or generic args.
fn returns_result(sig: &syn::Signature) -> bool {
    match &sig.output {
        syn::ReturnType::Type(_, ty) => match ty.as_ref() {
            syn::Type::Path(tp) => tp
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "Result"),
            _ => false,
        },
        syn::ReturnType::Default => false,
    }
}
