//! The per-family lexical tables: which comment markers, block pairs,
//! and string forms each language group's scan tracks.

/// Extension-to-lexicon table, sorted by extension (ASCII) so binary
/// search applies.
pub(super) const LEXED_EXTENSIONS: &[(&str, &Lexicon)] = &[
    ("bash", &HASH_SCRIPT),
    ("c", &SLASH),
    ("cc", &SLASH),
    ("conf", &HASH_PLAIN),
    ("cpp", &SLASH),
    ("dart", &SLASH),
    ("go", &SLASH),
    ("h", &SLASH),
    ("hpp", &SLASH),
    ("java", &SLASH),
    ("jl", &HASH_TRIPLE),
    ("js", &SLASH),
    ("kt", &SLASH),
    ("mjs", &SLASH),
    ("nim", &HASH_TRIPLE),
    ("php", &SLASH),
    ("pl", &HASH_SCRIPT),
    ("py", &HASH_TRIPLE),
    ("pyi", &HASH_TRIPLE),
    ("r", &HASH_PLAIN),
    ("rb", &HASH_RUBY),
    ("scala", &SLASH),
    ("sh", &HASH_SCRIPT),
    ("swift", &SLASH),
    ("ts", &SLASH),
    ("tsx", &SLASH),
    ("zig", &SLASH),
    ("zsh", &HASH_SCRIPT),
];
/// R and generic `#`-comment config formats: single-line strings only.
const HASH_PLAIN: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    reject_percent: false,
    multiline_quotes: false,
    word_start_comments: false,
};
/// Ruby: opaque backtick literals, marked heredocs, percent literals.
const HASH_RUBY: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: true,
    template_holes: false,
    heredoc: Heredoc::Ruby,
    reject_percent: true,
    multiline_quotes: true,
    word_start_comments: true,
};
/// Shells and Perl: backtick literals plus shell-style heredocs.
const HASH_SCRIPT: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: false,
    backtick: true,
    template_holes: false,
    heredoc: Heredoc::Shell,
    reject_percent: false,
    multiline_quotes: true,
    word_start_comments: true,
};
/// Python, Julia, Nim: triple-quoted multi-line strings.
const HASH_TRIPLE: Lexicon = Lexicon {
    line: "#",
    block: None,
    triple: true,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    reject_percent: false,
    multiline_quotes: false,
    word_start_comments: false,
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
    reject_percent: false,
    multiline_quotes: false,
    word_start_comments: false,
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
    /// Whether a percent literal (`%w[]`) rejects the scan: Ruby only,
    /// because R uses `%` infix operators (`%in%`, `%%`).
    pub(super) reject_percent: bool,
    /// Whether quoted strings may span lines natively (shells, Ruby), so
    /// a quote left open at the line's end carries its state.
    pub(super) multiline_quotes: bool,
    /// Whether the comment marker opens a comment only at the start of a
    /// word (POSIX `#` rules; Ruby after a token), so regex literals and
    /// mid-word `#` stay code.
    pub(super) word_start_comments: bool,
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
