//! C# lint checks: the XML doc-comment dialect over the same codes and
//! [`Diagnostic`] shape the Rust checks emit.
//!
//! One module per rule, named by lint code: `doc001_missing_docs` through
//! `test001_test_naming`.
//!
//! [`run`] walks the compilation unit and every declaration list in
//! document order, collecting one [`Declaration`] fact set per
//! declaration.
//!
//! Between collection and emission it computes the can-throw closure
//! and stamps the `<exception>` facts (`exception_scan`) of the
//! non-private throwing methods and constructors the closure flags.
//!
//! Every rule then runs over the collected facts in the same code order
//! the Rust backend emits (DOC001 through DOC006, then TEST001).
//!
//! The text checks (TEXT001, TEXT002) follow from the same parse's doc
//! regions.
//!
//! # Semantics
//!
//! - DOC001: non-private documentable declarations (`public`, `internal`,
//!   `protected`-family modifiers) need a `///` doc comment.
//! - DOC002: a non-private method or constructor that can throw needs
//!   an `<exception>` tag (error severity); throwing includes calls to
//!   same-file members that throw.
//! - DOC003: non-private can-throw members whose `<exception>` tags
//!   all lack a concrete `cref` type.
//! - DOC004: non-private methods, constructors, and indexers with
//!   parameters need `<param name="...">` tags.
//! - DOC005: `<param>` tags must name every declared parameter.
//! - DOC006: placeholder markers (`TODO`/`FIXME`/`TBD`) in doc comments.
//! - TEST001: `TestMethod`/`Test`/`Fact`/`Theory`-marked methods with
//!   discouraged (`test_*`, `case_*`, `test` + digits) names.
//! - TEXT001/TEXT002: `///` doc-comment prose measured with the XML doc
//!   dialect; findings carry original file lines. The dialect rules live
//!   with the lint crate's measuring core; see [`text_regions`]
//!   producer.
//!
//! [`text_regions`]: super::text_regions

use super::parse::{
    declaration_name, doc_comment_texts, doc_run_start_line, member_kind, parameter_names,
    visibility_of,
};
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, VisibilityTier};
use std::collections::{HashMap, HashSet};

mod doc001_missing_docs;
mod doc002_missing_exception_tag;
mod doc003_vague_exception;
mod doc004_missing_param_tags;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod test001_test_naming;

/// Kinds whose non-private declarations need doc comments.
const DOCUMENTABLE: &[ItemKind] = &[
    ItemKind::Class,
    ItemKind::Struct,
    ItemKind::Interface,
    ItemKind::Record,
    ItemKind::Enum,
    ItemKind::Delegate,
    ItemKind::Fn,
    ItemKind::Property,
    ItemKind::Event,
    ItemKind::Const,
    ItemKind::Static,
    ItemKind::Constructor,
];
/// Kinds checked for parameter documentation (DOC004/DOC005); properties
/// cover indexers, whose parameter lists hold real parameters.
const PARAMETERIZED: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor, ItemKind::Property];
/// Kinds checked for throwing, directly or through same-file calls
/// (DOC002/DOC003).
const THROWING: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor];

/// One declaration's lint context: the shared facts every rule reads,
/// built once per declaration by the walker.
struct Declaration<'a> {
    /// The declaration's syntax node, for rules that walk attributes.
    node: tree_sitter::Node<'a>,
    /// The full source text, slicing companion to `node`.
    source: &'a str,
    /// The declaration's model kind.
    kind: ItemKind,
    /// The declaration's name, when it has a meaningful one.
    name: Option<String>,
    /// The declaration's `///` doc-comment lines.
    docs: Vec<String>,
    /// True when the visibility modifier is not `private`.
    non_private: bool,
    /// The 1-based diagnostic line: the `///` doc run's start when
    /// present, else the declaration's own row.
    line: usize,
    /// The `<exception>` tag facts for a non-private member that can
    /// throw, directly or through same-file calls: tag count plus every
    /// `cref` value.
    ///
    /// `None` for members that cannot or do not throw, so DOC002 and
    /// DOC003 share one answer; [`run`] stamps it after the can-throw
    /// closure.
    exception_scan: Option<(usize, Vec<String>)>,
    /// The declared parameter names paired with their `<param>` tag names,
    /// for a non-private parameterized member that declares parameters.
    ///
    /// `None` otherwise, so DOC004 and DOC005 share one parameter walk.
    param_scan: Option<(Vec<String>, Vec<String>)>,
}

/// The facts the can-throw closure is computed from, filled in one
/// subtree walk per method and constructor.
struct ThrowFacts<'a> {
    /// Names of every method and constructor in the file: the targets a
    /// call site can resolve to.
    member_names: HashSet<&'a str>,
    /// Names proven to can throw: direct throws seed the set, reverse
    /// call edges grow it.
    can_throw: HashSet<&'a str>,
    /// Reverse call edges: callee name -> names of the members calling
    /// it.
    callers_of: HashMap<&'a str, Vec<&'a str>>,
    /// Direct-subtree `throw` flags, indexed like the declaration list.
    direct_throw: Vec<bool>,
}

impl Declaration<'_> {
    /// One diagnostic stamped with this declaration's line, kind, and
    /// name.
    fn diagnostic(&self, severity: Severity, code: &'static str, message: String) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message,
            line: self.line,
            item_kind: self.kind.as_str().to_string(),
            item_name: self.name.clone(),
        }
    }
}

impl<'a> ThrowFacts<'a> {
    /// Empty facts for `declaration_count` declarations.
    fn new(declaration_count: usize) -> Self {
        Self {
            member_names: HashSet::with_capacity(declaration_count),
            can_throw: HashSet::new(),
            callers_of: HashMap::new(),
            direct_throw: vec![false; declaration_count],
        }
    }

    /// Index the names calls can resolve to: methods and constructors
    /// of any visibility.
    fn index_member_names(&mut self, declarations: &'a [Declaration<'a>]) {
        for decl in declarations {
            if THROWING.contains(&decl.kind)
                && let Some(name) = decl.name.as_deref()
            {
                self.member_names.insert(name);
            }
        }
    }

    /// Walk one member's own body depth-first, reusing `cursor`: a
    /// `throw_statement` flags the member and seeds its name, and each
    /// call site that names a same-file member records a reverse edge.
    ///
    /// Nested callables (`lambda_expression`,
    /// `anonymous_method_expression`, `local_function_statement`) are
    /// boundaries, not body.
    ///
    /// Their throws and calls belong to the nested scope, so the walk
    /// skips their subtrees instead of attributing them to the
    /// enclosing member.
    fn scan_member(
        &mut self,
        idx: usize,
        decl: &'a Declaration<'a>,
        cursor: &mut tree_sitter::TreeCursor<'a>,
    ) {
        let node = decl.node;
        let caller = decl.name.as_deref();
        cursor.reset(node);
        'walk: loop {
            let current = cursor.node();
            let nested_callable = matches!(
                current.kind(),
                "lambda_expression" | "anonymous_method_expression" | "local_function_statement"
            );
            if !nested_callable {
                match current.kind() {
                    "throw_statement" => {
                        self.direct_throw[idx] = true;
                        if let Some(name) = caller {
                            self.can_throw.insert(name);
                        }
                    }
                    "invocation_expression" => {
                        self.record_call(current, "function", decl.source, caller)
                    }
                    "object_creation_expression" => {
                        self.record_call(current, "type", decl.source, caller)
                    }
                    _ => {}
                }
                if cursor.goto_first_child() {
                    continue 'walk;
                }
            }
            loop {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
                if !cursor.goto_parent() || cursor.node() == node {
                    break 'walk;
                }
            }
        }
    }

    /// Record one call edge when its target names a same-file method or
    /// constructor.
    ///
    /// A member with no name cannot be flagged, and no call can reach
    /// it, so its edges are dropped.
    fn record_call(
        &mut self,
        call: tree_sitter::Node<'a>,
        target_field: &'static str,
        source: &'a str,
        caller: Option<&'a str>,
    ) {
        let Some(caller) = caller else {
            return;
        };
        let Some(target) = call.child_by_field_name(target_field) else {
            return;
        };
        let Some(name) = call_target_name(target, source) else {
            return;
        };
        // `nameof(X)` mentions a member; it never calls it.
        if name != "nameof" && self.member_names.contains(name) {
            self.callers_of.entry(name).or_default().push(caller);
        }
    }
}

/// Run every C# check over `parsed` and return all diagnostics in document
/// order: the declaration checks first, then the text checks (TEXT001,
/// TEXT002) over the same parse's doc regions.
///
/// Returns no diagnostics when the parse tree carries error nodes: a
/// broken tree would report findings against misread declarations, so the
/// whole pass degrades to silence.
pub(super) fn run(parsed: &ParseResult) -> Vec<Diagnostic> {
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }

    let source = parsed.source.as_str();
    let mut declarations = Vec::with_capacity(parsed.items.len());
    collect_children(parsed.syntax_tree().root_node(), source, &mut declarations);

    // The can-throw closure spans the whole file (a caller may sit
    // before its callee), so it runs between collection and the rules;
    // stamping from its answers keeps diagnostics in document order.
    let can_throw = can_throw_closure(&declarations);
    for (decl, throws) in declarations.iter_mut().zip(can_throw) {
        if decl.non_private && THROWING.contains(&decl.kind) && throws {
            decl.exception_scan = Some(exception_tags(&decl.docs));
        }
    }

    let mut diagnostics = Vec::with_capacity(parsed.items.len());
    for decl in &declarations {
        check_declaration(decl, &mut diagnostics);
    }

    diagnostics.extend(super::text_regions::text_checks(parsed));
    diagnostics
}

/// The simple call-target name of `target`.
///
/// - A bare name yields the identifier itself: `Helper()`.
/// - A qualified target yields its rightmost segment:
///   `this.Helper()`, `obj.Helper()`, `new Cfg.Exception()`.
/// - A generic target yields its bare name: `Helper<T>()`.
fn call_target_name<'a>(target: tree_sitter::Node<'a>, source: &'a str) -> Option<&'a str> {
    let mut node = target;
    loop {
        if node.kind() == "identifier" {
            return node.utf8_text(source.as_bytes()).ok();
        }
        // member_access_expression and qualified_name hold their
        // rightmost segment in `name`; generic_name holds its bare name
        // as the leading identifier child.
        let next = node
            .child_by_field_name("name")
            .or_else(|| node.named_child(0))?;
        node = next;
    }
}

/// The can-throw answers for one file's declarations: declaration `i`
/// can throw when its subtree holds a `throw` or one of its calls
/// reaches, transitively, a same-file member that throws.
///
/// Resolution is same-file and name-keyed, so it stays conservative:
///
/// - `Helper()`, `this.Helper()`, `obj.Helper()`, and `Helper<T>()`
///   all resolve to a member named `Helper`; `new C()` resolves to a
///   constructor named `C`.
/// - Overloads and same-name members of other same-file types match
///   too: accepted false positives over missed throws.
/// - Calls the file cannot resolve (framework members, other-file
///   helpers) never flag the caller; `nameof(X)` references a name
///   without calling it.
/// - Private members count as throw sources; the checks themselves
///   still skip them.
/// - Calls and throws inside nested lambdas, anonymous methods, and
///   local functions belong to the nested scope, never the enclosing
///   member.
///
/// The fixpoint over reverse call edges is cycle-safe: names enter the
/// can-throw set at most once, so self- and mutual recursion
/// terminate, and a cycle can throw when any member reachable in it
/// can.
fn can_throw_closure<'a>(declarations: &'a [Declaration<'a>]) -> Vec<bool> {
    let mut facts = ThrowFacts::new(declarations.len());
    facts.index_member_names(declarations);
    if let Some(first) = declarations.first() {
        // One cursor is reused across every member's subtree; a fresh
        // cursor per member would allocate behind every step.
        let mut cursor = first.node.walk();
        for (idx, decl) in declarations.iter().enumerate() {
            if THROWING.contains(&decl.kind) {
                facts.scan_member(idx, decl, &mut cursor);
            }
        }
    }

    // Propagate along reverse edges until stable: a caller of a
    // can-throw name can throw.
    let mut work: Vec<&str> = facts.can_throw.iter().copied().collect();
    while let Some(name) = work.pop() {
        if let Some(callers) = facts.callers_of.get_mut(name) {
            for caller in callers.iter().copied() {
                if facts.can_throw.insert(caller) {
                    work.push(caller);
                }
            }
        }
    }

    declarations
        .iter()
        .enumerate()
        .map(|(idx, decl)| {
            // A nameless member can still throw directly; it just never
            // propagates, because no call can name it.
            facts.direct_throw[idx]
                || decl
                    .name
                    .as_deref()
                    .is_some_and(|name| facts.can_throw.contains(name))
        })
        .collect()
}

/// Run every rule over one collected declaration.
fn check_declaration(decl: &Declaration<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(doc001_missing_docs::check(decl));
    diagnostics.extend(doc002_missing_exception_tag::check(decl));
    diagnostics.extend(doc003_vague_exception::check(decl));
    diagnostics.extend(doc004_missing_param_tags::check(decl));
    diagnostics.extend(doc005_undocumented_param::check(decl));
    diagnostics.extend(doc006_placeholder::check(decl));
    diagnostics.extend(test001_test_naming::check(decl));
}

/// Collect the facts of every declaration under `list` (a
/// `compilation_unit` or `declaration_list`) in document order, recursing
/// into nested bodies and preprocessor branches.
fn collect_children<'a>(
    list: tree_sitter::Node<'a>,
    source: &'a str,
    declarations: &mut Vec<Declaration<'a>>,
) {
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        let kind = child.kind();
        if !child.is_named() || kind == "comment" {
            continue;
        }
        if let Some(body) = child
            .child_by_field_name("body")
            .filter(|b| b.kind() == "declaration_list")
        {
            collect_declaration(child, source, declarations);
            collect_children(body, source, declarations);
        } else if kind == "preproc_if" || kind == "preproc_else" || kind == "preproc_elif" {
            // Conditional branches hold real declarations; collect them.
            collect_children(child, source, declarations);
        } else {
            collect_declaration(child, source, declarations);
        }
    }
}

/// The `<exception>` tags in `docs`: their count and every `cref` value.
fn exception_tags(docs: &[String]) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut crefs = Vec::new();
    for tag in tag_slices(docs, "exception") {
        count += 1;
        if let Some(value) = attribute_value(tag, "cref") {
            crefs.push(value);
        }
    }
    (count, crefs)
}

/// Collect one declaration node's facts into `declarations`.
///
/// Skips nodes that carry no member facts: usings, namespaces, and
/// unrecognized kinds.
fn collect_declaration<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a str,
    declarations: &mut Vec<Declaration<'a>>,
) {
    let kind = member_kind(node.kind());
    if kind == ItemKind::Other || kind == ItemKind::Using || kind == ItemKind::Namespace {
        return;
    }

    // Shared facts: computed once per declaration, never per rule. The
    // <exception> facts are stamped in `run` after the can-throw
    // closure, which needs every declaration first.
    let non_private = visibility_of(node, source).is_some_and(|vis| vis != VisibilityTier::Private);
    let docs = doc_comment_texts(node, source);
    let param_scan = if non_private && PARAMETERIZED.contains(&kind) {
        let params = parameter_names(node, source);
        (!params.is_empty()).then(|| (params, param_tag_names(&docs)))
    } else {
        None
    };
    declarations.push(Declaration {
        node,
        source,
        kind,
        name: declaration_name(node, source),
        // The `///` doc run's line when present, else the declaration's own
        // row; doc_run_start_line shares the parse module's adjacency
        // contract with span building and doc collection.
        line: doc_run_start_line(node, source).unwrap_or_else(|| node.start_position().row + 1),
        docs,
        non_private,
        exception_scan: None,
        param_scan,
    });
}

/// The `name` attribute values of every `<param>` tag in `docs`.
fn param_tag_names(docs: &[String]) -> Vec<String> {
    tag_slices(docs, "param")
        .filter_map(|tag| attribute_value(tag, "name"))
        .collect()
}

/// The quoted value of `attribute` inside a `<tag ...>` opening-tag slice.
fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let needle = format!("{attribute}=");
    let pos = tag.find(&needle)?;
    let rest = tag[pos + needle.len()..].trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &rest[1..];
    let end = value.find(quote)?;
    Some(value[..end].to_string())
}

/// The opening-tag text of every `<name ...>` tag in `docs`, scanned one
/// `///` line at a time; a tag split across lines does not match.
///
/// Tag names match whole: `<paramref ...>` is not a `<param>` tag, and
/// likewise for any longer tag sharing the sought prefix.
fn tag_slices<'a>(docs: &'a [String], tag: &str) -> impl Iterator<Item = &'a str> {
    docs.iter().flat_map(move |line| {
        let needle = format!("<{tag}");
        let mut rest = line.as_str();
        let mut out = Vec::new();
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos..];
            // The byte after the tag name must end it: whitespace, `>`,
            // or the self-closing `/` of `<param/>`.
            let boundary = after[needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/');
            if boundary {
                let end = after.find('>').map_or(rest.len(), |gt| pos + gt);
                out.push(&rest[pos..end]);
            }
            rest = &rest[pos + needle.len()..];
        }
        out.into_iter()
    })
}

#[cfg(test)]
mod tests {
    use super::{run, tag_slices};
    use rust_llm_tidy_lint::check::CODE_MISSING_ERRORS;

    /// Full C# lint pass over `source`: the entry point every rule
    /// observes, shared by the can-throw tests.
    fn lint(source: &str) -> Vec<rust_llm_tidy_lint::Diagnostic> {
        let parsed = super::super::parse::parse(source).expect("test source must parse");
        run(&parsed)
    }

    /// The DOC002 findings' member names of the pass; members without a
    /// name cannot be flagged, so they never appear.
    fn missing_exception_names(source: &str) -> Vec<String> {
        lint(source)
            .into_iter()
            .filter(|d| d.code == CODE_MISSING_ERRORS)
            .filter_map(|d| d.item_name)
            .collect()
    }

    // ── nested-callable scan boundaries ──

    /// A returned lambda's body runs on the caller's schedule, so a
    /// throwing call inside it must not flag the enclosing member;
    /// a direct call in the member's own body still does.
    #[test]
    fn deferred_lambda_calls_do_not_flag_enclosing_member() {
        let source = "\
class C {
    void Thrower() { throw new System.Exception(); }
    /// <summary>Returns a deferred thrower.</summary>
    public System.Func<int> Deferred() {
        return () => { Thrower(); return 0; };
    }
    /// <summary>Calls the thrower now.</summary>
    public int Direct() { Thrower(); return 0; }
}
";

        assert_eq!(missing_exception_names(source), ["Direct".to_string()]);
    }

    /// An uncalled local function's body runs only when invoked, so its
    /// throwing call must not flag the enclosing member.
    #[test]
    fn local_function_calls_do_not_flag_enclosing_member() {
        let source = "\
class C {
    void Thrower() { throw new System.Exception(); }
    /// <summary>Wraps a local function.</summary>
    public int Outer() {
        int Local() { Thrower(); return 0; }
        return 1;
    }
}
";

        assert_eq!(missing_exception_names(source), Vec::<String>::new());
    }

    // ── whole-tag-name matching ──

    /// `<paramref>` shares the `<param` prefix but is its own tag, so a
    /// `param` scan must not count it.
    #[test]
    fn tag_slices_skips_tags_sharing_only_a_prefix() {
        let docs = vec![" <paramref name=\"key\"/>".to_string()];

        assert_eq!(tag_slices(&docs, "param").count(), 0);
    }

    /// A real `<param>` tag matches exactly once, whether it carries
    /// attributes, closes, or self-closes.
    #[test]
    fn tag_slices_matches_whole_tag_names() {
        let docs = vec![
            " <param name=\"key\">The key.</param>".to_string(),
            " <param>".to_string(),
            " <param/>".to_string(),
        ];

        let slices: Vec<&str> = tag_slices(&docs, "param").collect();
        assert_eq!(slices.len(), 3, "one slice per whole-name tag: {slices:?}");
        assert!(slices[0].starts_with("<param name=\"key\""));
    }
}
