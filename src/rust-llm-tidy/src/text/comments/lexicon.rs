//! The lexicon types every comment family shares, plus the fail-closed
//! reject predicates and byte predicates those scans apply.
//!
//! Concrete per-language values live in [`super::families`].

/// The lexical forms one language group's scan tracks.
pub(super) struct Lexicon {
    /// Language-specific token boundaries and escape rules.
    pub(super) syntax: Syntax,
    /// The line-comment marker.
    pub(super) line: &'static str,
    /// The block-comment open/close pair.
    pub(super) block: Option<(&'static str, &'static str)>,
    /// Whether `"""` and `'''` open multi-line strings.
    pub(super) triple: bool,
    /// Whether a backtick opens a multi-line literal.
    pub(super) backtick: bool,
    /// Whether `${` holes inside backtick literals are walked (template
    /// languages) instead of read as literal text.
    pub(super) template_holes: bool,
    /// The heredoc recognition style.
    pub(super) heredoc: Heredoc,
    /// Fail-closed rejects: literal forms the scan does not model,
    /// whose payload could otherwise be misread as comments.
    pub(super) rejects: &'static [Reject],
    /// Whether a backslash escapes the comment marker (TeX): an
    /// odd-length backslash run makes the marker print literally
    /// instead of commenting.
    pub(super) escaped_marker: bool,
    /// Whether `'` opens a string literal. False where the apostrophe is
    /// punctuation instead.
    ///
    /// Punctuation cases: the Lisp quote operator, Ada attributes
    /// (`X'First`), Haskell names (`x'`), Elm and TeX text.
    pub(super) single_quotes: bool,
    /// Whether quoted strings may span lines natively (shells, Ruby,
    /// the Lisp family, Elm), so a quote left open at the line's end
    /// carries its state.
    pub(super) multiline_quotes: bool,
    /// Whether the comment marker opens a comment only at the start of a
    /// word (POSIX `#` rules; Ruby after a token).
    ///
    /// Outside that, regex literals and mid-word `#` stay code.
    pub(super) word_start_comments: bool,
    /// Whether the block pair's markers open and close only alone on
    /// their lines (MATLAB `%{`/`%}`); elsewhere the line marker
    /// comments, as MATLAB itself reads it.
    pub(super) block_markers_alone: bool,
}

/// How `<<DELIM` heredocs are recognized, when at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Heredoc {
    /// No heredoc syntax (`<<` is always an operator).
    None,
    /// Shell: `<<`, `<<-`, `<<'D'`, `<<"D"`, spaces allowed after `<<`.
    Shell,
    /// Ruby: marked forms only (`<<~D`, `<<-D`, `<<'D'`); bare `<<D`
    /// is ambiguous with the push/shift operators and rejects.
    Ruby,
}

/// A literal form outside the family's lexicon: opening one rejects the
/// whole scan rather than guessing at its payload.
#[derive(Clone, Copy)]
pub(super) enum Reject {
    /// CMake bracket arguments and bracket comments, including `=` levels.
    CmakeBracket,
    /// Ruby percent literals (`%w[]`, `%Q(...)`): arbitrary delimiters.
    PercentLiteral,
    /// PostgreSQL dollar-quoted strings.
    ///
    /// Forms: `$$ ... $$` and `$tag$ ... $tag$`.
    DollarQuote,
    /// Lua long strings and level-`=` comments (`[[`, `[=[`).
    LongBracket,
    /// Haskell quasiquotes and template brackets (`[name|`, `[|`).
    QuasiQuote,
    /// Lisp semicolons that are not comments: `#;` datum comments,
    /// `#\;` characters, Elisp `?;`/`?\;`, and Clojure `\;`.
    SemicolonLiteral,
    /// TeX verbatim material: `\verb` and verbatim-like environments.
    Verbatim,
    /// Erlang `$%` and `$\%`: the percent character literal.
    DollarPercent,
    /// YAML block-scalar indicators (`|`, `>`, with `-`/`+`/digit
    /// modifiers) at a value position.
    BlockScalar,
}

/// Token rules that cannot be expressed by comment and quote delimiters.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Syntax {
    /// Shared code-family rules with backslash escapes.
    Common,
    /// YAML plain scalars and whitespace-separated comments.
    Yaml,
    /// PowerShell backtick escapes and literal single-quoted strings.
    PowerShell,
}

impl Reject {
    /// Whether the bytes at `i` open this literal form.
    pub(super) fn opens(self, bytes: &[u8], i: usize) -> bool {
        match self {
            Reject::CmakeBracket => {
                let start = i + usize::from(bytes[i] == b'#');

                bytes.get(start) == Some(&b'[')
                    && bytes[start + 1..].iter().find(|&&b| b != b'=') == Some(&b'[')
            }
            Reject::PercentLiteral => bytes[i] == b'%' && percent_literal(bytes, i),
            Reject::DollarQuote => {
                bytes[i] == b'$' && bytes.get(ident_run_end(bytes, i + 1)) == Some(&b'$')
            }
            Reject::LongBracket => {
                bytes[i] == b'[' && matches!(bytes.get(i + 1), Some(b'[') | Some(b'='))
            }
            Reject::QuasiQuote => {
                bytes[i] == b'[' && bytes.get(ident_run_end(bytes, i + 1)) == Some(&b'|')
            }
            Reject::SemicolonLiteral => semicolon_literal(bytes, i),
            Reject::Verbatim => verbatim(bytes, i),
            Reject::DollarPercent => {
                bytes[i] == b'$'
                    && (bytes.get(i + 1) == Some(&b'%')
                        || (bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 2) == Some(&b'%')))
            }
            Reject::BlockScalar => matches!(bytes[i], b'|' | b'>') && block_scalar(bytes, i),
        }
    }
}

/// Whether the byte at `i` starts a word: line start, whitespace, or a
/// statement delimiter precedes it.
pub(super) fn comment_starts_word(bytes: &[u8], i: usize) -> bool {
    i == 0
        || matches!(
            bytes[i - 1],
            b' ' | b'\t'
                | b';'
                | b','
                | b'('
                | b')'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b'|'
                | b'&'
                | b'<'
                | b'>'
        )
}

/// Whether `b` may continue an identifier.
pub(super) fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Whether `b` may start a heredoc delimiter or identifier.
pub(super) fn ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

/// Whether the bytes at `i` open a YAML block scalar: `|` or `>` with
/// optional `-`/`+`/digit modifiers, then end of line or a trailing
/// comment.
///
/// Value positions:
///
/// - after `key:` or a `-` sequence entry, with optional spaces;
/// - alone on its line, heading a value on the following lines.
///
/// Anchors and tags may precede the indicator. Ambiguous header-shaped
/// suffixes also reject rather than risk measuring scalar payload.
fn block_scalar(bytes: &[u8], i: usize) -> bool {
    let mut j = i + 1;
    while matches!(bytes.get(j), Some(&b) if b.is_ascii_digit() || matches!(b, b'-' | b'+')) {
        j += 1;
    }
    // The header ends at end of line or a trailing `#` comment.
    let mut k = j;
    while matches!(bytes.get(k), Some(b' ') | Some(b'\t')) {
        k += 1;
    }
    match bytes.get(k) {
        None | Some(b'#') => {}
        Some(_) => return false,
    }
    let mut p = i;
    while p > 0 && matches!(bytes[p - 1], b' ' | b'\t') {
        p -= 1;
    }
    p == 0 || p < i || matches!(bytes[p - 1], b':' | b'-')
}

/// The index just past the identifier run starting at `j`.
fn ident_run_end(bytes: &[u8], mut j: usize) -> usize {
    while matches!(bytes.get(j), Some(&b) if ident_byte(b)) {
        j += 1;
    }
    j
}

/// Whether `%` at `bytes[i]` opens a Ruby percent literal: a typed form
/// (`%w[`) or any non-alphanumeric, non-space delimiter (`%(`).
fn percent_literal(bytes: &[u8], i: usize) -> bool {
    match bytes.get(i + 1) {
        Some(&b) if b"qQiIwWrsx".contains(&b) => {
            matches!(bytes.get(i + 2), Some(&d) if !d.is_ascii_alphanumeric())
        }
        Some(&b) => !b.is_ascii_alphanumeric() && !b.is_ascii_whitespace(),
        None => false,
    }
}

/// Whether the bytes at `i` open a Lisp semicolon that is not a comment.
///
/// Non-comment forms: a reader form (`#;`, `#\;`) or a character literal
/// (`\;`, `?;`, `?\;`). The `?` form needs a preceding non-identifier
/// byte, so `odd?;` keeps its comment.
fn semicolon_literal(bytes: &[u8], i: usize) -> bool {
    match bytes[i] {
        b'#' => {
            bytes.get(i + 1) == Some(&b';')
                || (bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 2) == Some(&b';'))
        }
        b'\\' => bytes.get(i + 1) == Some(&b';'),
        b'?' => bytes.get(i + 1) == Some(&b';') && (i == 0 || !ident_byte(bytes[i - 1])),
        _ => false,
    }
}

/// Whether the bytes at `i` open TeX verbatim material: `\verb` (with
/// its optional `*`) before a delimiter, or a `\begin{...}` whose
/// environment is verbatim-like.
fn verbatim(bytes: &[u8], i: usize) -> bool {
    if bytes[i] != b'\\' {
        return false;
    }
    if bytes[i..].starts_with(b"\\verb") {
        // A letter continues a longer control word (`\verbatiminput`);
        // anything else is `\verb`'s delimiter.
        return match bytes.get(i + 5) {
            None | Some(b'*') => true,
            Some(&b) => !b.is_ascii_alphabetic(),
        };
    }
    [
        "\\begin{verbatim",
        "\\begin{Verbatim",
        "\\begin{minted",
        "\\begin{lstlisting",
    ]
    .iter()
    .any(|open| bytes[i..].starts_with(open.as_bytes()))
}
