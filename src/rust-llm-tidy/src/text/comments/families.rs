//! The per-language lexicon constants and the extension lookup: which
//! comment markers, block pairs, and string forms each language group's
//! scan tracks.
//!
//! The shared [`Lexicon`] type and the fail-closed reject predicates
//! live in [`super::lexicon`].

use super::lexicon::{Heredoc, Lexicon, Reject, Syntax};

/// Extension-to-lexicon table, sorted by extension (ASCII) so binary
/// search applies.
pub(super) const LEXED_EXTENSIONS: &[(&str, &Lexicon)] = &[
    ("ada", &DASH_ADA),
    ("applescript", &DASH_APPLESCRIPT),
    ("bash", &HASH_SCRIPT),
    ("bst", &PERCENT_TEX),
    ("bzl", &HASH_TRIPLE),
    ("c", &SLASH),
    ("cc", &SLASH),
    ("clj", &SEMI_PLAIN),
    ("cljc", &SEMI_PLAIN),
    ("cls", &PERCENT_TEX),
    ("cmake", &HASH_CMAKE),
    ("conf", &HASH_PLAIN),
    ("cpp", &SLASH),
    ("dart", &SLASH),
    ("el", &SEMI_BLOCK),
    ("elm", &DASH_ELM),
    ("erl", &PERCENT_ERL),
    ("fish", &HASH_FISH),
    ("go", &SLASH),
    ("gql", &HASH_GRAPHQL),
    ("gradle", &SLASH),
    ("graphql", &HASH_GRAPHQL),
    ("groovy", &SLASH),
    ("h", &SLASH),
    ("hpp", &SLASH),
    ("hs", &DASH_HS),
    ("java", &SLASH),
    ("jl", &HASH_TRIPLE),
    ("js", &SLASH),
    ("json5", &SLASH),
    ("jsonc", &SLASH),
    ("ksh", &HASH_SCRIPT),
    ("kt", &SLASH),
    ("less", &SLASH),
    ("lisp", &SEMI_BLOCK),
    ("ltx", &PERCENT_TEX),
    ("lua", &DASH_LUA),
    ("m", &PERCENT_MATLAB),
    ("mjs", &SLASH),
    ("nim", &HASH_TRIPLE),
    ("php", &SLASH),
    ("pl", &HASH_SCRIPT),
    ("proto", &SLASH),
    ("ps1", &HASH_POWERSHELL),
    ("psd1", &HASH_POWERSHELL),
    ("psm1", &HASH_POWERSHELL),
    ("purs", &DASH_ELM),
    ("r", &HASH_PLAIN),
    ("rb", &HASH_RUBY),
    ("scala", &SLASH),
    ("scm", &SEMI_BLOCK),
    ("scss", &SLASH),
    ("sh", &HASH_SCRIPT),
    ("sol", &SLASH),
    ("sql", &DASH_SQL),
    ("sty", &PERCENT_TEX),
    ("sv", &SLASH_VERILOG),
    ("swift", &SLASH),
    ("tex", &PERCENT_TEX),
    ("thrift", &SLASH),
    ("toml", &HASH_TRIPLE),
    ("ts", &SLASH),
    ("tsx", &SLASH),
    ("v", &SLASH_VERILOG),
    ("vhd", &DASH_ADA),
    ("yaml", &HASH_YAML),
    ("yml", &HASH_YAML),
    ("zig", &SLASH),
    ("zsh", &HASH_SCRIPT),
];
/// Ada: `--` comments only; apostrophes are attributes and character
/// literals, never string delimiters.
const DASH_ADA: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
/// AppleScript: `--` comments and `(* *)` block comments; `"` strings
/// are single-line and apostrophes are plain text.
const DASH_APPLESCRIPT: Lexicon = Lexicon {
    syntax: Syntax::Common,
    line: "--",
    block: Some(("(*", "*)")),
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
/// CMake: `#` comments; quoted arguments may span lines.
const HASH_CMAKE: Lexicon = Lexicon {
    syntax: Syntax::Common,
    line: "#",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::CmakeBracket],
    escaped_marker: false,
    single_quotes: false,
    multiline_quotes: true,
    word_start_comments: false,
    block_markers_alone: false,
};
/// Fish: `#` comments at word start only, quotes span lines, and `<<`
/// is always an operator (fish has no heredocs).
const HASH_FISH: Lexicon = Lexicon {
    syntax: Syntax::Common,
    line: "#",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: true,
    word_start_comments: true,
    block_markers_alone: false,
};
/// GraphQL: `#` comments and `"""` block strings; apostrophes are
/// plain text.
const HASH_GRAPHQL: Lexicon = Lexicon {
    syntax: Syntax::Common,
    line: "#",
    block: None,
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
/// R and generic `#`-comment config formats: single-line strings only.
const HASH_PLAIN: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
/// PowerShell: `#` comments at word start only, `<# #>` block
/// comments, quotes span lines, and the backtick is an escape, never a
/// literal.
const HASH_POWERSHELL: Lexicon = Lexicon {
    syntax: Syntax::PowerShell,
    line: "#",
    block: Some(("<#", "#>")),
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: true,
    word_start_comments: true,
    block_markers_alone: false,
};
/// Ruby: opaque backtick literals, marked heredocs, percent literals.
const HASH_RUBY: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
/// Julia, Nim: triple-quoted multi-line strings.
const HASH_TRIPLE: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
/// YAML: `#` comments at word start only, quotes span lines; block
/// scalars reject the scan.
const HASH_YAML: Lexicon = Lexicon {
    syntax: Syntax::Yaml,
    line: "#",
    block: None,
    triple: false,
    backtick: false,
    template_holes: false,
    heredoc: Heredoc::None,
    rejects: &[Reject::BlockScalar],
    escaped_marker: false,
    single_quotes: true,
    multiline_quotes: true,
    word_start_comments: true,
    block_markers_alone: false,
};
/// Erlang: `%` comments; `'` atoms and `"` strings are single-line.
const PERCENT_ERL: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
/// MATLAB and Octave: `%` comments, `%{ %}` block comments, and `'`
/// strings and transposes and `"` strings, all single-line.
///
/// The block markers count only alone on their lines.
const PERCENT_MATLAB: Lexicon = Lexicon {
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
    syntax: Syntax::Common,
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
/// Verilog and SystemVerilog: the `//` family shape, but apostrophes
/// are width-separator punctuation (`8'h00`), never string delimiters.
const SLASH_VERILOG: Lexicon = Lexicon {
    syntax: Syntax::Common,
    line: "//",
    block: Some(("/*", "*/")),
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
