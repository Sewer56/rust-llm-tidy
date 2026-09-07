//! Regression coverage for literal payload and comment attribution.

use super::text_checks;
use rstest::rstest;

/// A natural prose line that triggers TEXT002 when measured as a comment.
const PROSE: &str = "This payload is intentionally long enough to exceed the configured line budget without being a source comment.";

#[rstest]
#[case::yaml_anchor("payload: &a |\n  # payload\n", "yaml")]
#[case::yaml_tag("payload: !!str >\n  payload\n", "yaml")]
#[case::cmake_bracket("set(payload [[text]])\n", "cmake")]
#[case::cmake_bracket_comment("#[==[text]==]\n", "cmake")]
#[case::powershell_here_string("$s = @'\npayload\n'@\n", "ps1")]
#[case::powershell_subexpression("$s = \"$(Get-Item 'file')\"\n", "ps1")]
fn lints_should_discard_findings_when_syntax_is_unmodeled(#[case] source: &str, #[case] ext: &str) {
    let comment = format!("# {PROSE}\n");
    assert!(!text_checks(&comment, ext).is_empty());
    let source = format!("{comment}{source}{comment}");

    let diagnostics = text_checks(&source, ext);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[rstest]
#[case::yaml_anchor("payload: &a |\n  # {prose}\n", "yaml")]
#[case::yaml_tag("payload: !!str >-\n  # {prose}\n", "yml")]
#[case::yaml_tag_anchor("payload: !!str &a |2-\n  # {prose}\n", "yaml")]
#[case::yaml_anchor_tag("payload: &a !!str >+\n  # {prose}\n", "yaml")]
#[case::yaml_sequence("- &a |\n  # {prose}\n", "yaml")]
#[case::yaml_comma("payload: token,# {prose}\n", "yaml")]
#[case::yaml_semicolon("payload: token;# {prose}\n", "yaml")]
#[case::yaml_single_quote("payload: 'C:\\' # short\n", "yaml")]
#[case::cmake_bracket("set(payload [[\n# {prose}\n]])\n", "cmake")]
#[case::cmake_bracket_level("set(payload [==[\n# {prose}\n]==])\n", "cmake")]
#[case::cmake_bracket_comment("#[=[\n# {prose}\n]=]\n", "cmake")]
#[case::powershell_escape("$s = \"`\" # {prose}\"\n", "ps1")]
#[case::powershell_escaped_hash("Write-Host `# {prose}\n", "psm1")]
#[case::powershell_here_string("$s = @\"\n\" # {prose}\n\"@\n", "psd1")]
#[case::powershell_subexpression("$s = \"$(Get-Item \"# {prose}\")\"\n", "ps1")]
fn lints_should_ignore_literal_payload(#[case] source: &str, #[case] ext: &str) {
    let source = source.replace("{prose}", PROSE);

    let diagnostics = text_checks(&source, ext);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[rstest]
#[case::yaml_apostrophe("name: don't\n# {prose}\n", "yaml", 2)]
#[case::yaml_embedded_quote("name: a \"quoted value\n# {prose}\n", "yaml", 2)]
#[case::yaml_trailing("name: don't # {prose}\n", "yml", 1)]
#[case::yaml_single_backslash("path: 'C:\\' # {prose}\n", "yaml", 1)]
#[case::yaml_doubled_quote("name: 'don''t' # {prose}\n", "yaml", 1)]
#[case::yaml_flow("names: [\"value\", 'other'] # {prose}\n", "yaml", 1)]
#[case::yaml_tag_quote("name: !!str \"value\" # {prose}\n", "yaml", 1)]
#[case::yaml_spanned("name: \"starts\n# payload\nends\"\n# {prose}\n", "yaml", 4)]
#[case::cmake_apostrophe("set(name don't)\n# {prose}\n", "cmake", 2)]
#[case::cmake_non_bracket("set(name [=value)\n# {prose}\n", "cmake", 2)]
#[case::powershell_backslash("$s = \"C:\\\" # {prose}\n", "ps1", 1)]
#[case::powershell_single_quote("$s = 'C:\\' # {prose}\n", "psm1", 1)]
#[case::powershell_doubled_quote("$s = 'don''t' # {prose}\n", "psd1", 1)]
#[case::powershell_even_backtick("$s = \"``\" # {prose}\n", "ps1", 1)]
#[case::powershell_literal_backtick("$s = '`' # {prose}\n", "ps1", 1)]
fn lints_should_measure_real_comments_after_values(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] line: usize,
) {
    let source = source.replace("{prose}", PROSE);

    let diagnostics = text_checks(&source, ext);
    let found: Vec<_> = diagnostics.iter().filter(|d| d.code == "TEXT002").collect();

    assert_eq!(found.len(), 1, "{diagnostics:?}");
    assert_eq!(found[0].line, line);
}
