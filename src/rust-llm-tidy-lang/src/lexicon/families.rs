//! The per-family lexical tables, the fail-closed reject predicates,
//! and the extension lookup: which comment markers, block pairs, and
//! string forms each language group's scan tracks.

/// Extension-to-lexicon table, sorted by extension (ASCII) so binary
/// search applies.
pub(super) const LEXED_EXTENSIONS: &[(&str, &Lexicon)] = &[
    ("ada", &DASH_ADA),
    ("bash", &HASH_SCRIPT),
    ("c", &SLASH),
    ("cc", &SLASH),
    ("clj", &SEMI_PLAIN),
    ("cljc", &SEMI_PLAIN),
    ("conf", &HASH_PLAIN),
    ("cpp", &SLASH),
    ("dart", &SLASH),
    ("el", &SEMI_BLOCK),
    ("elm", &DASH_ELM),
    ("erl", &PERCENT_ERL),
    ("go", &SLASH),
    ("h", &SLASH),
    ("hpp", &SLASH),
    ("hs", &DASH_HS),
    ("java", &SLASH),
    ("jl", &HASH_TRIPLE),
    ("js", &SLASH),
    ("kt", &SLASH),
    ("lisp", &SEMI_BLOCK),
    ("lua", &DASH_LUA),
    ("m", &PERCENT_MATLAB),
    ("mjs", &SLASH),
    ("nim", &HASH_TRIPLE),
    ("php", &SLASH),
    ("pl", &HASH_SCRIPT),
    ("py", &HASH_TRIPLE),
    ("pyi", &HASH_TRIPLE),
    ("r", &HASH_PLAIN),
    ("rb", &HASH_RUBY),
    ("scala", &SLASH),
    ("scm", &SEMI_BLOCK),
    ("sh", &HASH_SCRIPT),
    ("sql", &DASH_SQL),
    ("swift", &SLASH),
    ("tex", &PERCENT_TEX),
    ("ts", &SLASH),
    ("tsx", &SLASH),
    ("zig", &SLASH),
    ("zsh", &HASH_SCRIPT),
];
/// Ada: `--` comments only; apostrophes are attributes and character
/// literals, never string delimiters.
const DASH_ADA: Lexicon = Lexicon {
    line: "--",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Elm: `--` and nestable `{- -}` comments; single-line `"` strings and
/// `"""` multi-line strings.
const DASH_ELM: Lexicon = Lexicon {
    line: "--",
    block: Some(("{-", "-}")),
    triple: true,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Haskell: `--` and nestable `{- -}` comments; quasiquotes reject.
const DASH_HS: Lexicon = Lexicon {
    line: "--",
    block: Some(("{-", "-}")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::QuasiQuote],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Lua: `--` and `--[[ ]]` comments; long brackets reject.
const DASH_LUA: Lexicon = Lexicon {
    line: "--",
    block: Some(("--[[", "]]")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::LongBracket],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// SQL: `--` and `/* */` comments; strings and quoted identifiers are
/// single-line; dollar-quoted strings reject.
const DASH_SQL: Lexicon = Lexicon {
    line: "--",
    block: Some(("/*", "*/")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::DollarQuote],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// R and generic `#`-comment config formats: single-line strings only.
const HASH_PLAIN: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Ruby: opaque backtick literals, marked heredocs, percent literals.
const HASH_RUBY: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: true,
    template_holes: false,
    heredoc: Heredoc::Ruby,
    rejects: &[Reject::PercentLiteral],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: true,
    word_start_comments: true,
    block_markers_alone: false,
};
/// Shells and Perl: backtick literals plus shell-style heredocs.
const HASH_SCRIPT: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: true,
    template_holes: false,
    heredoc: Heredoc::Shell,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: true,
    word_start_comments: true,
    block_markers_alone: false,
};
/// Python, Julia, Nim: triple-quoted multi-line strings.
const HASH_TRIPLE: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: true,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Erlang: `%` comments; `'` atoms and `"` strings are single-line.
const PERCENT_ERL: Lexicon = Lexicon {
    line: "%",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::DollarPercent],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// MATLAB and Octave: `%` comments and `%{ %}` block comments (the
/// markers count only alone on their lines); `'` strings and
/// transposes and `"` strings, all single-line.
const PERCENT_MATLAB: Lexicon = Lexicon {
    line: "%",
    block: Some(("%{", "%}")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: true,
};
/// TeX: `%` comments; a backslash escapes the marker; verbatim
/// material rejects.
const PERCENT_TEX: Lexicon = Lexicon {
    line: "%",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::Verbatim],
    escaped_marker: true,
    single_quotes: false,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Common Lisp, Elisp, Scheme: `;` and nestable `#| |#` comments;
/// reader and character semicolons reject.
const SEMI_BLOCK: Lexicon = Lexicon {
    line: ";",
    block: Some(("#|", "|#")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::SemicolonLiteral],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: true,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Clojure: `;` comments; `"` strings span lines; `'` quotes values;
/// `\;` is the semicolon character.
const SEMI_PLAIN: Lexicon = Lexicon {
    line: ";",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::SemicolonLiteral],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: true,
    word_start_comments: false,
    block_markers_alone: false,
};
/// The `//` family: block comments, single-line strings, template
/// literals with `${}` holes, and `"""`/`'''` text blocks.
const SLASH: Lexicon = Lexicon {
    line: "//",
    block: Some(("/*", "*/")),
    triple: true,
    backtick: true,
    template_holes: true,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: false,
    word_start_comments: false,
    block_markers_alone: false,
};

/// The lexical forms one language group's scan tracks.
pub(super) struct Lexicon {
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
    /// punctuation instead: the Lisp quote operator, Ada attributes
    /// (`X'First`), Haskell names (`x'`), Elm and TeX text.
    pub(super) single_quotes: bool,
    /// Whether quoted strings may span lines natively (shells, Ruby,
    /// the Lisp family, Elm), so a quote left open at the line's end
    /// carries its state.
    pub(super) multiline_quotes: bool,
    /// Whether the comment marker opens a comment only at the start of a
    /// word (POSIX `#` rules; Ruby after a token), so regex literals and
    /// mid-word `#` stay code.
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
    /// Ruby percent literals (`%w[]`, `%Q(...)`): arbitrary delimiters.
    PercentLiteral,
    /// PostgreSQL dollar-quoted strings (`$$ ... $$`, `$tag$ ... $tag$`).
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
}

impl Reject {
    /// Whether the bytes at `i` open this literal form.
    pub(super) fn opens(self, bytes: &[u8], i: usize) -> bool {
        match self {
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

/// Whether the bytes at `i` open a Lisp semicolon that is not a
/// comment: a reader form (`#;`, `#\;`) or a character literal
/// (`\;`, `?;`, `?\;`). The `?` form needs a preceding
/// non-identifier byte, so `odd?;` keeps its comment.
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
